//! Storage reader utilities for creating storage clients from profiles
use crate::cloud_storage::{CloudStorage, CloudStorageError, S3CloudStorage};
use crate::db::{DbStorageProfile, StorageLocation};
use std::sync::Arc;
use tracing::debug;

/// Create a storage reader from a profile.
///
/// For cloud profiles: creates S3CloudStorage from profile credentials
/// For local profiles: returns LocalFileStorage that reads from disk
pub async fn create_storage_reader(
    profile: &DbStorageProfile,
) -> Result<Arc<dyn CloudStorage>, CloudStorageError> {
    debug!(
        "Creating storage reader for profile '{}' (id={}, location={:?})",
        profile.name, profile.id, profile.location
    );

    match profile.location {
        StorageLocation::Cloud => {
            let s3_config = profile.to_s3_config().ok_or_else(|| {
                CloudStorageError::Config("Missing S3 credentials in profile".into())
            })?;
            let client = S3CloudStorage::new(s3_config).await?;
            Ok(Arc::new(client))
        }
        StorageLocation::Local => Ok(Arc::new(LocalFileStorage)),
    }
}

/// Local file storage that reads files from disk paths.
pub struct LocalFileStorage;

#[async_trait::async_trait]
impl CloudStorage for LocalFileStorage {
    async fn upload(&self, path: &str, data: &[u8]) -> Result<String, CloudStorageError> {
        tokio::fs::write(path, data).await?;
        Ok(path.to_string())
    }

    async fn download(&self, path: &str) -> Result<Vec<u8>, CloudStorageError> {
        tokio::fs::read(path).await.map_err(CloudStorageError::Io)
    }

    async fn delete(&self, path: &str) -> Result<(), CloudStorageError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(CloudStorageError::Io)
    }
}
