//! Storage trait and implementation

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

use crate::cloud_storage::CloudStorageManager;
use crate::db::{DbStorageProfile, StorageLocation};
use crate::encryption::EncryptionService;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(String),

    #[error("Storage not configured")]
    NotConfigured,

    #[error("Operation not supported for this storage configuration")]
    NotSupported(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Cloud storage error: {0}")]
    Cloud(String),
}

/// Trait for reading/writing release files to storage
///
/// Abstracts over different storage configurations (local/cloud, encrypted/plain, chunked/raw).
/// Implementations apply the appropriate transforms based on the StorageProfile.
#[async_trait]
pub trait ReleaseStorage: Send + Sync {
    /// Read a file from storage
    async fn read_file(&self, release_id: &str, filename: &str) -> Result<Vec<u8>, StorageError>;

    /// Write a file to storage
    async fn write_file(
        &self,
        release_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<(), StorageError>;

    /// List all files for a release
    async fn list_files(&self, release_id: &str) -> Result<Vec<String>, StorageError>;

    /// Check if a file exists
    async fn file_exists(&self, release_id: &str, filename: &str) -> Result<bool, StorageError>;

    /// Delete a file from storage
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
}

impl ReleaseStorageImpl {
    /// Create storage for local raw files (no encryption, no chunking)
    pub fn new_local_raw(profile: DbStorageProfile) -> Self {
        Self {
            profile,
            encryption: None,
            cloud: None,
        }
    }

    /// Create storage with encryption service
    pub fn new_with_encryption(profile: DbStorageProfile, encryption: EncryptionService) -> Self {
        Self {
            profile,
            encryption: Some(encryption),
            cloud: None,
        }
    }

    /// Create storage with cloud backend
    pub fn new_with_cloud(profile: DbStorageProfile, cloud: Arc<CloudStorageManager>) -> Self {
        Self {
            profile,
            encryption: None,
            cloud: Some(cloud),
        }
    }

    /// Create storage with both encryption and cloud
    pub fn new_full(
        profile: DbStorageProfile,
        encryption: EncryptionService,
        cloud: Arc<CloudStorageManager>,
    ) -> Self {
        Self {
            profile,
            encryption: Some(encryption),
            cloud: Some(cloud),
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
            .ok_or_else(|| StorageError::NotConfigured)?;

        let (ciphertext, nonce) = encryption
            .encrypt(data)
            .map_err(|e| StorageError::Encryption(e.to_string()))?;

        // Prepend nonce to ciphertext (12 bytes nonce + ciphertext)
        let mut result = nonce;
        result.extend(ciphertext);
        Ok(result)
    }

    /// Decrypt data if encryption is enabled
    fn decrypt_if_needed(&self, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if !self.profile.encrypted {
            return Ok(data.to_vec());
        }

        let encryption = self
            .encryption
            .as_ref()
            .ok_or_else(|| StorageError::NotConfigured)?;

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
}

#[async_trait]
impl ReleaseStorage for ReleaseStorageImpl {
    async fn read_file(&self, release_id: &str, filename: &str) -> Result<Vec<u8>, StorageError> {
        if self.profile.chunked {
            return Err(StorageError::NotSupported(
                "Chunked storage not yet implemented".to_string(),
            ));
        }

        // Read raw data from storage backend
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
    ) -> Result<(), StorageError> {
        if self.profile.chunked {
            return Err(StorageError::NotSupported(
                "Chunked storage not yet implemented".to_string(),
            ));
        }

        // Encrypt if needed
        let data_to_store = self.encrypt_if_needed(data)?;

        // Write to storage backend
        match self.profile.location {
            StorageLocation::Local => {
                let path = self.file_path(release_id, filename);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&path, &data_to_store).await?;
            }
            StorageLocation::Cloud => {
                let cloud = self.cloud.as_ref().ok_or(StorageError::NotConfigured)?;
                let key = self.cloud_key(release_id, filename);
                cloud
                    .upload_chunk_data(&key, &data_to_store)
                    .await
                    .map_err(|e| StorageError::Cloud(e.to_string()))?;
            }
        }

        Ok(())
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
        DbStorageProfile::new(
            "Test Local Raw",
            StorageLocation::Local,
            temp_dir.path().to_str().unwrap(),
            false, // not encrypted
            false, // not chunked
        )
    }

    fn local_encrypted_profile(temp_dir: &TempDir) -> DbStorageProfile {
        DbStorageProfile::new(
            "Test Local Encrypted",
            StorageLocation::Local,
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

    #[tokio::test]
    async fn test_write_and_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ReleaseStorageImpl::new_local_raw(local_raw_profile(&temp_dir));

        let release_id = "test-release-123";
        let filename = "cover.jpg";
        let data = b"fake image data";

        // Write
        storage
            .write_file(release_id, filename, data)
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
            .write_file(release_id, "yes.txt", b"hello")
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
            .write_file(release_id, "track01.flac", b"audio1")
            .await
            .unwrap();
        storage
            .write_file(release_id, "track02.flac", b"audio2")
            .await
            .unwrap();
        storage
            .write_file(release_id, "cover.jpg", b"image")
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
            .write_file(release_id, "delete-me.txt", b"bye")
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
            .write_file(release_id, filename, data)
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
