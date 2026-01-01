//! Storage trait and implementation
use crate::cloud_storage::{CloudStorage, S3CloudStorage};
use crate::db::{Database, DbChunk, DbFile, DbFileChunk, DbStorageProfile, StorageLocation};
use crate::encryption::EncryptionService;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::info;
use uuid::Uuid;
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage not configured")]
    NotConfigured,
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Cloud storage error: {0}")]
    Cloud(String),
    #[error("Database error: {0}")]
    Database(String),
}
/// Progress callback type: (bytes_written, total_bytes)
pub type ProgressCallback = Box<dyn Fn(usize, usize) + Send + Sync>;
/// Trait for writing release files to storage during import
///
/// Abstracts over different storage configurations (local/cloud, encrypted/plain, chunked/raw).
/// Implementations apply the appropriate transforms based on the StorageProfile.
#[async_trait]
pub trait ReleaseStorage: Send + Sync {
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
}
/// Storage implementation that applies transforms based on StorageProfile flags
///
/// Handles all 8 combinations of (local/cloud) × (encrypted/plain) × (chunked/raw).
/// Transforms are applied in sequence: chunk → encrypt → store.
#[derive(Clone)]
pub struct ReleaseStorageImpl {
    profile: DbStorageProfile,
    encryption: Option<EncryptionService>,
    cloud: Option<Arc<dyn CloudStorage>>,
    database: Option<Arc<Database>>,
    chunk_size_bytes: usize,
}
impl ReleaseStorageImpl {
    /// Create storage from a profile, creating S3 client from profile credentials if needed.
    pub async fn from_profile(
        profile: DbStorageProfile,
        encryption: Option<EncryptionService>,
        database: Arc<Database>,
        chunk_size_bytes: usize,
    ) -> Result<Self, StorageError> {
        let cloud: Option<Arc<dyn CloudStorage>> = if profile.location == StorageLocation::Cloud {
            let s3_config = profile
                .to_s3_config()
                .ok_or_else(|| StorageError::Cloud("Missing S3 credentials in profile".into()))?;
            let client = S3CloudStorage::new(s3_config)
                .await
                .map_err(|e| StorageError::Cloud(e.to_string()))?;
            info!("Created S3 client for profile: {}", profile.name);
            Some(Arc::new(client))
        } else {
            None
        };
        Ok(Self {
            profile,
            encryption,
            cloud,
            database: Some(database),
            chunk_size_bytes,
        })
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
        let mut result = nonce;
        result.extend(ciphertext);
        Ok(result)
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
        on_progress(0, total_bytes);
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
            prepared_chunks.push((chunk_id, chunk_index, chunk_to_store, original_len, end));
            offset = end;
            chunk_index += 1;
        }
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
                    let storage_location = if let Some(dir) = chunks_dir {
                        let chunk_path = dir.join(&chunk_id);
                        tokio::fs::write(&chunk_path, &encrypted_data).await?;
                        chunk_path.display().to_string()
                    } else {
                        let cloud = cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                        let key = format!("{}/chunks/{}", release_id, chunk_id);
                        cloud
                            .upload_chunk(&key, &encrypted_data)
                            .await
                            .map_err(|e| StorageError::Cloud(e.to_string()))?
                    };
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
                    let file_chunk =
                        DbFileChunk::new(&file_id, &chunk_id, idx, 0, original_len as i64);
                    db.insert_file_chunk(&file_chunk)
                        .await
                        .map_err(|e| StorageError::Database(e.to_string()))?;
                    bytes_written.fetch_max(cumulative_bytes, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }
            },
        ))
        .buffer_unordered(16)
        .inspect(|result| {
            if result.is_ok() {
                let current = bytes_written.load(std::sync::atomic::Ordering::SeqCst);
                on_progress(current, total_bytes);
            }
        })
        .collect()
        .await;
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
        on_progress(0, total_bytes);
        let data_to_store = self.encrypt_if_needed(data)?;
        let storage_path = match self.profile.location {
            StorageLocation::Local => {
                let path = self.file_path(release_id, filename);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let batch_size = 1_048_576;
                let file = tokio::fs::File::create(&path).await?;
                let mut writer = tokio::io::BufWriter::new(file);
                let mut bytes_written = 0usize;
                for chunk in data_to_store.chunks(batch_size) {
                    writer.write_all(chunk).await?;
                    bytes_written += chunk.len();
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
                let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                let key = self.cloud_key(release_id, filename);
                let storage_location = cloud
                    .upload_chunk(&key, &data_to_store)
                    .await
                    .map_err(|e| StorageError::Cloud(e.to_string()))?;
                on_progress(total_bytes, total_bytes);
                storage_location
            }
        };
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
}
#[async_trait]
impl ReleaseStorage for ReleaseStorageImpl {
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
}
