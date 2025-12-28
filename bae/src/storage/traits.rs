//! Storage trait and implementation

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use crate::cloud_storage::CloudStorageManager;
use crate::db::{Database, DbChunk, DbFile, DbFileChunk, DbStorageProfile, StorageLocation};
use crate::encryption::EncryptionService;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {0}")]
    #[allow(dead_code)] // Used by test-only trait methods
    NotFound(String),

    #[error("Storage not configured")]
    NotConfigured,

    #[error("Operation not supported for this storage configuration")]
    #[allow(dead_code)] // Used by test-only trait methods
    NotSupported(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Cloud storage error: {0}")]
    Cloud(String),

    #[error("Database error: {0}")]
    Database(String),
}

/// Progress callback type: (bytes_written, total_bytes)
pub type ProgressCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

/// Trait for reading/writing release files to storage
///
/// Abstracts over different storage configurations (local/cloud, encrypted/plain, chunked/raw).
/// Implementations apply the appropriate transforms based on the StorageProfile.
#[async_trait]
pub trait ReleaseStorage: Send + Sync {
    /// Read a file from storage (test-only currently)
    #[allow(dead_code)]
    async fn read_file(&self, release_id: &str, filename: &str) -> Result<Vec<u8>, StorageError>;

    /// Write a file to storage with progress reporting.
    ///
    /// For chunked storage: reports after each chunk completes.
    /// For non-chunked storage: streams write in 1MB batches.
    ///
    /// `start_chunk_index` is used for sequential chunk numbering across files.
    /// Returns the next chunk index (for chaining writes).
    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
        start_chunk_index: i32,
        on_progress: ProgressCallback,
    ) -> Result<i32, StorageError>;

    /// List all files for a release (test-only currently)
    #[allow(dead_code)]
    async fn list_files(&self, release_id: &str) -> Result<Vec<String>, StorageError>;

    /// Check if a file exists (test-only currently)
    #[allow(dead_code)]
    async fn file_exists(&self, release_id: &str, filename: &str) -> Result<bool, StorageError>;

    /// Delete a file from storage (test-only currently)
    #[allow(dead_code)]
    async fn delete_file(&self, release_id: &str, filename: &str) -> Result<(), StorageError>;
}

/// Storage implementation that applies transforms based on StorageProfile flags
///
/// Handles all 8 combinations of (local/cloud) × (encrypted/plain) × (chunked/raw).
/// Transforms are applied in sequence: chunk → encrypt → store.
#[derive(Clone)]
pub struct ReleaseStorageImpl {
    profile: DbStorageProfile,
    encryption: Option<EncryptionService>,
    cloud: Option<Arc<CloudStorageManager>>,
    database: Option<Arc<Database>>,
    chunk_size_bytes: usize,
}

impl ReleaseStorageImpl {
    /// Default chunk size: 1MB (for test helpers)
    #[cfg(test)]
    const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

    /// Create storage for local raw files (no encryption, no chunking) - test only
    #[cfg(test)]
    pub fn new_local_raw(profile: DbStorageProfile) -> Self {
        Self {
            profile,
            encryption: None,
            cloud: None,
            database: None,
            chunk_size_bytes: Self::DEFAULT_CHUNK_SIZE,
        }
    }

    /// Create storage with encryption service (test-only)
    #[cfg(test)]
    pub fn new_with_encryption(profile: DbStorageProfile, encryption: EncryptionService) -> Self {
        Self {
            profile,
            encryption: Some(encryption),
            cloud: None,
            database: None,
            chunk_size_bytes: Self::DEFAULT_CHUNK_SIZE,
        }
    }

    /// Create fully configured storage (all features)
    pub fn new_full(
        profile: DbStorageProfile,
        encryption: Option<EncryptionService>,
        cloud: Option<Arc<CloudStorageManager>>,
        database: Arc<Database>,
        chunk_size_bytes: usize,
    ) -> Self {
        Self {
            profile,
            encryption,
            cloud,
            database: Some(database),
            chunk_size_bytes,
        }
    }

    /// Get the local path for a release's files
    fn release_path(&self, release_id: &str) -> PathBuf {
        PathBuf::from(&self.profile.location_path).join(release_id)
    }

    /// Get the full path for a specific file
    fn file_path(&self, release_id: &str, filename: &str) -> PathBuf {
        self.release_path(release_id).join(filename)
    }

    /// Encrypt data if encryption is enabled
    fn encrypt_if_needed(&self, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if !self.profile.encrypted {
            return Ok(data.to_vec());
        }

        let encryption = self
            .encryption
            .as_ref()
            .ok_or(StorageError::NotConfigured)?;

        let (ciphertext, nonce) = encryption
            .encrypt(data)
            .map_err(|e| StorageError::Encryption(e.to_string()))?;

        // Prepend nonce to ciphertext (12 bytes nonce + ciphertext)
        let mut result = nonce;
        result.extend(ciphertext);
        Ok(result)
    }

    /// Decrypt data if encryption is enabled (used by test-only read methods)
    #[allow(dead_code)]
    fn decrypt_if_needed(&self, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if !self.profile.encrypted {
            return Ok(data.to_vec());
        }

        let encryption = self
            .encryption
            .as_ref()
            .ok_or(StorageError::NotConfigured)?;

        if data.len() < 12 {
            return Err(StorageError::Encryption(
                "Data too short for nonce".to_string(),
            ));
        }

        let (nonce, ciphertext) = data.split_at(12);

        encryption
            .decrypt(ciphertext, nonce)
            .map_err(|e| StorageError::Encryption(e.to_string()))
    }

    /// Generate a storage key for cloud storage
    fn cloud_key(&self, release_id: &str, filename: &str) -> String {
        format!("{}/{}", release_id, filename)
    }

    /// Write chunked file data with progress reporting (internal helper).
    async fn write_chunked<F>(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
        start_chunk_index: i32,
        on_progress: &F,
    ) -> Result<i32, StorageError>
    where
        F: Fn(usize, usize) + Send + Sync + ?Sized,
    {
        use futures::stream::{self, StreamExt};

        let db = self.database.clone().ok_or(StorageError::NotConfigured)?;

        // Create file record
        let format = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_lowercase();
        let db_file = DbFile::new(release_id, filename, data.len() as i64, &format);
        let file_id = db_file.id.clone();

        db.insert_file(&db_file)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let num_chunks = data.len().div_ceil(self.chunk_size_bytes);
        let total_bytes = data.len();

        // Report initial progress
        on_progress(0, total_bytes);

        // Phase 1: Prepare all chunks (encrypt if needed)
        let mut prepared_chunks: Vec<(String, i32, Vec<u8>, usize, usize)> =
            Vec::with_capacity(num_chunks);
        let mut offset = 0usize;
        let mut chunk_index = start_chunk_index;

        while offset < data.len() {
            let end = std::cmp::min(offset + self.chunk_size_bytes, data.len());
            let chunk_data = &data[offset..end];
            let original_len = chunk_data.len();

            let chunk_to_store = self.encrypt_if_needed(chunk_data)?;
            let chunk_id = Uuid::new_v4().to_string();

            // Track cumulative bytes for progress (end position of this chunk)
            prepared_chunks.push((chunk_id, chunk_index, chunk_to_store, original_len, end));

            offset = end;
            chunk_index += 1;
        }

        // Create directory once for local storage
        let chunks_dir = match self.profile.location {
            StorageLocation::Local => {
                let dir = self.release_path(release_id).join("chunks");
                tokio::fs::create_dir_all(&dir).await?;
                Some(dir)
            }
            StorageLocation::Cloud => None,
        };

        let cloud = self.cloud.clone();
        let release_id_owned = release_id.to_string();

        // Phase 2: Write chunks in parallel with per-chunk progress reporting
        // Use atomic counter to track progress across concurrent chunk completions
        let bytes_written = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let results: Vec<Result<(), StorageError>> = stream::iter(prepared_chunks.into_iter().map(
            |(chunk_id, idx, encrypted_data, original_len, cumulative_bytes)| {
                let chunks_dir = chunks_dir.clone();
                let cloud = cloud.clone();
                let db = db.clone();
                let release_id = release_id_owned.clone();
                let file_id = file_id.clone();
                let bytes_written = bytes_written.clone();

                async move {
                    // Write to storage
                    let storage_location = if let Some(dir) = chunks_dir {
                        let chunk_path = dir.join(&chunk_id);
                        tokio::fs::write(&chunk_path, &encrypted_data).await?;
                        chunk_path.display().to_string()
                    } else {
                        let cloud = cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                        let key = format!("{}/chunks/{}", release_id, chunk_id);
                        cloud
                            .upload_chunk_data(&key, &encrypted_data)
                            .await
                            .map_err(|e| StorageError::Cloud(e.to_string()))?
                    };

                    // Insert chunk record
                    let db_chunk = DbChunk::from_release_chunk(
                        &release_id,
                        &chunk_id,
                        idx,
                        encrypted_data.len(),
                        &storage_location,
                    );
                    db.insert_chunk(&db_chunk)
                        .await
                        .map_err(|e| StorageError::Database(e.to_string()))?;

                    // Insert file-chunk mapping
                    let file_chunk =
                        DbFileChunk::new(&file_id, &chunk_id, idx, 0, original_len as i64);
                    db.insert_file_chunk(&file_chunk)
                        .await
                        .map_err(|e| StorageError::Database(e.to_string()))?;

                    // Update progress (chunks complete out of order, so use max)
                    bytes_written.fetch_max(cumulative_bytes, std::sync::atomic::Ordering::SeqCst);

                    Ok(())
                }
            },
        ))
        .buffer_unordered(16)
        .inspect(|result| {
            // Report progress after each chunk completes (success or failure)
            if result.is_ok() {
                let current = bytes_written.load(std::sync::atomic::Ordering::SeqCst);
                on_progress(current, total_bytes);
            }
        })
        .collect()
        .await;

        // Check for errors
        for result in results {
            result?;
        }

        Ok(chunk_index)
    }

    /// Write non-chunked file with progress reporting (internal helper).
    async fn write_non_chunked<F>(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
        on_progress: &F,
    ) -> Result<(), StorageError>
    where
        F: Fn(usize, usize) + Send + Sync + ?Sized,
    {
        use tokio::io::AsyncWriteExt;

        let total_bytes = data.len();

        // Report initial progress
        on_progress(0, total_bytes);

        // Encrypt whole file if needed
        let data_to_store = self.encrypt_if_needed(data)?;

        let storage_path = match self.profile.location {
            StorageLocation::Local => {
                let path = self.file_path(release_id, filename);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }

                // Stream write in 1MB batches for progress reporting
                let batch_size = 1_048_576; // 1MB
                let file = tokio::fs::File::create(&path).await?;
                let mut writer = tokio::io::BufWriter::new(file);

                let mut bytes_written = 0usize;
                for chunk in data_to_store.chunks(batch_size) {
                    writer.write_all(chunk).await?;
                    bytes_written += chunk.len();

                    // Report progress based on original data size (pre-encryption)
                    // Scale from encrypted size back to original size
                    let progress_bytes = if data_to_store.len() != data.len() {
                        (bytes_written as f64 * data.len() as f64 / data_to_store.len() as f64)
                            as usize
                    } else {
                        bytes_written
                    };
                    on_progress(progress_bytes.min(total_bytes), total_bytes);
                }
                writer.flush().await?;

                path.display().to_string()
            }
            StorageLocation::Cloud => {
                // For cloud, we upload atomically (no streaming progress within upload)
                // But we report 0% then 100%
                let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                let key = self.cloud_key(release_id, filename);
                let storage_location = cloud
                    .upload_chunk_data(&key, &data_to_store)
                    .await
                    .map_err(|e| StorageError::Cloud(e.to_string()))?;

                on_progress(total_bytes, total_bytes);
                storage_location
            }
        };

        // Create DbFile record
        if let Some(db) = &self.database {
            let format = std::path::Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin")
                .to_lowercase();
            let mut db_file = DbFile::new(release_id, filename, data.len() as i64, &format);
            db_file.source_path = Some(storage_path);

            db.insert_file(&db_file)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Read chunked file data (used by test-only read_file)
    #[allow(dead_code)]
    async fn read_chunked(
        &self,
        release_id: &str,
        filename: &str,
    ) -> Result<Vec<u8>, StorageError> {
        let db = self.database.as_ref().ok_or(StorageError::NotConfigured)?;

        // Find the file record
        let file = db
            .get_file_by_release_and_filename(release_id, filename)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?
            .ok_or_else(|| StorageError::NotFound(filename.to_string()))?;

        // Get file-chunk mappings
        let file_chunks = db
            .get_file_chunks(&file.id)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if file_chunks.is_empty() {
            return Err(StorageError::NotFound(format!(
                "No chunks found for file: {}",
                filename
            )));
        }

        // Read and reassemble chunks
        let mut data = Vec::with_capacity(file.file_size as usize);

        for fc in file_chunks {
            // Get chunk metadata
            let chunk = db
                .get_chunk_by_id(&fc.chunk_id)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?
                .ok_or_else(|| {
                    StorageError::NotFound(format!("Chunk not found: {}", fc.chunk_id))
                })?;

            // Read chunk data
            let encrypted_chunk = match self.profile.location {
                StorageLocation::Local => {
                    let chunk_path = PathBuf::from(&chunk.storage_location);
                    tokio::fs::read(&chunk_path).await.map_err(|e| {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            StorageError::NotFound(chunk_path.display().to_string())
                        } else {
                            StorageError::Io(e)
                        }
                    })?
                }
                StorageLocation::Cloud => {
                    let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                    cloud
                        .download_chunk(&chunk.storage_location)
                        .await
                        .map_err(|e| StorageError::Cloud(e.to_string()))?
                }
            };

            // Decrypt if needed
            let decrypted = self.decrypt_if_needed(&encrypted_chunk)?;

            // Extract the portion of this chunk that belongs to our file
            let start = fc.byte_offset as usize;
            let end = start + fc.byte_length as usize;
            data.extend_from_slice(&decrypted[start..end]);
        }

        Ok(data)
    }
}

#[async_trait]
impl ReleaseStorage for ReleaseStorageImpl {
    async fn read_file(&self, release_id: &str, filename: &str) -> Result<Vec<u8>, StorageError> {
        if self.profile.chunked {
            return self.read_chunked(release_id, filename).await;
        }

        // Non-chunked: read whole file from storage backend
        let raw_data = match self.profile.location {
            StorageLocation::Local => {
                let path = self.file_path(release_id, filename);
                tokio::fs::read(&path).await.map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        StorageError::NotFound(path.display().to_string())
                    } else {
                        StorageError::Io(e)
                    }
                })?
            }
            StorageLocation::Cloud => {
                let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                let key = self.cloud_key(release_id, filename);
                cloud
                    .download_chunk(&key)
                    .await
                    .map_err(|e| StorageError::Cloud(e.to_string()))?
            }
        };

        // Decrypt if needed
        self.decrypt_if_needed(&raw_data)
    }

    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
        start_chunk_index: i32,
        on_progress: ProgressCallback,
    ) -> Result<i32, StorageError> {
        if self.profile.chunked {
            self.write_chunked(release_id, filename, data, start_chunk_index, &*on_progress)
                .await
        } else {
            self.write_non_chunked(release_id, filename, data, &*on_progress)
                .await?;
            Ok(start_chunk_index)
        }
    }

    async fn list_files(&self, release_id: &str) -> Result<Vec<String>, StorageError> {
        match self.profile.location {
            StorageLocation::Local => {
                let release_path = self.release_path(release_id);
                if !release_path.exists() {
                    return Ok(Vec::new());
                }

                let mut files = Vec::new();
                let mut entries = tokio::fs::read_dir(&release_path).await?;

                while let Some(entry) = entries.next_entry().await? {
                    if entry.file_type().await?.is_file() {
                        if let Some(name) = entry.file_name().to_str() {
                            files.push(name.to_string());
                        }
                    }
                }

                Ok(files)
            }
            StorageLocation::Cloud => {
                // Cloud listing would require S3 list objects API
                Err(StorageError::NotSupported(
                    "Cloud file listing not yet implemented".to_string(),
                ))
            }
        }
    }

    async fn file_exists(&self, release_id: &str, filename: &str) -> Result<bool, StorageError> {
        match self.profile.location {
            StorageLocation::Local => {
                let path = self.file_path(release_id, filename);
                Ok(path.exists())
            }
            StorageLocation::Cloud => {
                // Could use S3 head object, but not implemented yet
                Err(StorageError::NotSupported(
                    "Cloud file existence check not yet implemented".to_string(),
                ))
            }
        }
    }

    async fn delete_file(&self, release_id: &str, filename: &str) -> Result<(), StorageError> {
        match self.profile.location {
            StorageLocation::Local => {
                let path = self.file_path(release_id, filename);
                if path.exists() {
                    tokio::fs::remove_file(&path).await?;
                }
                Ok(())
            }
            StorageLocation::Cloud => {
                let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                let key = self.cloud_key(release_id, filename);
                cloud
                    .delete_chunk(&key)
                    .await
                    .map_err(|e| StorageError::Cloud(e.to_string()))?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn local_raw_profile(temp_dir: &TempDir) -> DbStorageProfile {
        DbStorageProfile::new_local(
            "Test Local Raw",
            temp_dir.path().to_str().unwrap(),
            false, // not encrypted
            false, // not chunked
        )
    }

    #[cfg(feature = "test-utils")]
    fn local_encrypted_profile(temp_dir: &TempDir) -> DbStorageProfile {
        DbStorageProfile::new_local(
            "Test Local Encrypted",
            temp_dir.path().to_str().unwrap(),
            true,  // encrypted
            false, // not chunked
        )
    }

    #[cfg(feature = "test-utils")]
    fn test_encryption_service() -> EncryptionService {
        // 32-byte test key
        EncryptionService::new_with_key(vec![0u8; 32])
    }

    /// No-op progress callback for tests
    fn no_progress() -> ProgressCallback {
        Box::new(|_, _| {})
    }

    #[tokio::test]
    async fn test_write_and_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new_local_raw(local_raw_profile(&temp_dir));

        let release_id = "test-release-123";
        let filename = "cover.jpg";
        let data = b"fake image data";

        // Write
        storage
            .write_file(release_id, filename, data, 0, no_progress())
            .await
            .unwrap();

        // Read back
        let read_data = storage.read_file(release_id, filename).await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new_local_raw(local_raw_profile(&temp_dir));

        let release_id = "test-release-456";

        // Doesn't exist yet
        assert!(!storage.file_exists(release_id, "nope.txt").await.unwrap());

        // Write it
        storage
            .write_file(release_id, "yes.txt", b"hello", 0, no_progress())
            .await
            .unwrap();

        // Now exists
        assert!(storage.file_exists(release_id, "yes.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_files() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new_local_raw(local_raw_profile(&temp_dir));

        let release_id = "test-release-789";

        // Empty initially
        let files = storage.list_files(release_id).await.unwrap();
        assert!(files.is_empty());

        // Add some files
        storage
            .write_file(release_id, "track01.flac", b"audio1", 0, no_progress())
            .await
            .unwrap();
        storage
            .write_file(release_id, "track02.flac", b"audio2", 0, no_progress())
            .await
            .unwrap();
        storage
            .write_file(release_id, "cover.jpg", b"image", 0, no_progress())
            .await
            .unwrap();

        // List them
        let mut files = storage.list_files(release_id).await.unwrap();
        files.sort();
        assert_eq!(files, vec!["cover.jpg", "track01.flac", "track02.flac"]);
    }

    #[tokio::test]
    async fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new_local_raw(local_raw_profile(&temp_dir));

        let release_id = "test-release-del";

        // Write and verify
        storage
            .write_file(release_id, "delete-me.txt", b"bye", 0, no_progress())
            .await
            .unwrap();
        assert!(storage
            .file_exists(release_id, "delete-me.txt")
            .await
            .unwrap());

        // Delete
        storage
            .delete_file(release_id, "delete-me.txt")
            .await
            .unwrap();
        assert!(!storage
            .file_exists(release_id, "delete-me.txt")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_read_nonexistent_returns_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new_local_raw(local_raw_profile(&temp_dir));

        let result = storage.read_file("no-release", "no-file.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[cfg(feature = "test-utils")]
    #[tokio::test]
    async fn test_encrypted_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let encryption = test_encryption_service();
        let storage =
            ReleaseStorageImpl::new_with_encryption(local_encrypted_profile(&temp_dir), encryption);

        let release_id = "test-encrypted-123";
        let filename = "secret.txt";
        let data = b"this is secret data";

        // Write encrypted
        storage
            .write_file(release_id, filename, data, 0, no_progress())
            .await
            .unwrap();

        // Verify file on disk is NOT plaintext
        let raw_path = temp_dir.path().join(release_id).join(filename);
        let raw_data = std::fs::read(&raw_path).unwrap();
        assert_ne!(raw_data, data); // Should be encrypted
        assert!(raw_data.len() > data.len()); // Encrypted data is larger (nonce + auth tag)

        // Read back should decrypt
        let read_data = storage.read_file(release_id, filename).await.unwrap();
        assert_eq!(read_data, data);
    }
}
