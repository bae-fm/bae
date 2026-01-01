use crate::cloud_storage::{CloudStorage, CloudStorageError};
use std::collections::HashMap;
use std::sync::Mutex;

/// Mock cloud storage for testing.
/// Stores files in memory instead of uploading to S3.
pub struct MockCloudStorage {
    /// Public for test assertions
    pub files: Mutex<HashMap<String, Vec<u8>>>,
}

impl Default for MockCloudStorage {
    fn default() -> Self {
        MockCloudStorage {
            files: Mutex::new(HashMap::new()),
        }
    }
}

impl MockCloudStorage {
    /// Create a new mock cloud storage instance
    #[allow(unused)]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl CloudStorage for MockCloudStorage {
    async fn upload(&self, key: &str, data: &[u8]) -> Result<String, CloudStorageError> {
        let location = format!(
            "s3://test-bucket/files/{}/{}/{}",
            &key[0..2],
            &key[2..4],
            key,
        );
        self.files
            .lock()
            .unwrap()
            .insert(location.clone(), data.to_vec());
        Ok(location)
    }

    async fn download(&self, storage_location: &str) -> Result<Vec<u8>, CloudStorageError> {
        self.files
            .lock()
            .unwrap()
            .get(storage_location)
            .cloned()
            .ok_or_else(|| {
                CloudStorageError::Download(format!("File not found: {}", storage_location))
            })
    }

    async fn delete(&self, storage_location: &str) -> Result<(), CloudStorageError> {
        self.files.lock().unwrap().remove(storage_location);
        Ok(())
    }
}
