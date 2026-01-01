use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::{primitives::ByteStreamError, Client, Error as S3Error};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, warn};
#[derive(Error, Debug)]
pub enum CloudStorageError {
    #[error("S3 error: {0}")]
    S3(#[from] Box<S3Error>),
    #[error("S3 SDK error: {0}")]
    SdkError(String),
    #[error("ByteStream error: {0}")]
    ByteStream(#[from] ByteStreamError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Download error: {0}")]
    Download(String),
}
/// S3 configuration for cloud storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub bucket_name: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub endpoint_url: Option<String>,
}
impl S3Config {
    pub fn validate(&self) -> Result<(), CloudStorageError> {
        if self.bucket_name.trim().is_empty() {
            return Err(CloudStorageError::Config(
                "Bucket name cannot be empty".to_string(),
            ));
        }
        if self.region.trim().is_empty() {
            return Err(CloudStorageError::Config(
                "Region cannot be empty".to_string(),
            ));
        }
        if self.access_key_id.trim().is_empty() {
            return Err(CloudStorageError::Config(
                "Access key ID cannot be empty".to_string(),
            ));
        }
        if self.secret_access_key.trim().is_empty() {
            return Err(CloudStorageError::Config(
                "Secret access key cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}
/// Trait for cloud storage operations (allows mocking for tests)
#[async_trait::async_trait]
pub trait CloudStorage: Send + Sync {
    async fn upload(&self, key: &str, data: &[u8]) -> Result<String, CloudStorageError>;
    async fn download(&self, storage_location: &str) -> Result<Vec<u8>, CloudStorageError>;
    async fn delete(&self, storage_location: &str) -> Result<(), CloudStorageError>;
}
/// Format AWS SDK error for better debugging
fn format_error_details(err: &dyn std::fmt::Debug) -> String {
    let err_str = format!("{:?}", err);
    if err_str.contains("RequestTimeTooSkewed") {
        return format!(
            "RequestTimeTooSkewed: Clock skew detected. Your system clock and MinIO server clock are out of sync (difference > 15 minutes). \
            Please sync your system clock or the MinIO server clock. Error details: {}",
            err_str,
        );
    }
    if let Some(code_start) = err_str.find("code: Some(\"") {
        let code_end = err_str[code_start + 11..].find('"').unwrap_or(0);
        let code = &err_str[code_start + 11..code_start + 11 + code_end];
        if let Some(msg_start) = err_str.find("message: Some(\"") {
            let msg_end = err_str[msg_start + 15..].find('"').unwrap_or(0);
            let msg = &err_str[msg_start + 15..msg_start + 15 + msg_end];
            return format!("{}: {}", code, msg);
        }
    }
    err_str
}
/// Production S3 cloud storage implementation
pub struct S3CloudStorage {
    client: Client,
    bucket_name: String,
}
impl S3CloudStorage {
    /// Create a new S3 cloud storage client
    pub async fn new(config: S3Config) -> Result<Self, CloudStorageError> {
        Self::new_with_bucket_creation(config, true).await
    }
    /// Create a new S3 cloud storage client with optional bucket creation
    pub async fn new_with_bucket_creation(
        config: S3Config,
        create_bucket: bool,
    ) -> Result<Self, CloudStorageError> {
        let credentials = Credentials::new(
            config.access_key_id,
            config.secret_access_key,
            None,
            None,
            "bae-s3-config",
        );
        let mut aws_config_builder = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.region))
            .credentials_provider(credentials);
        if let Some(endpoint) = &config.endpoint_url {
            let normalized_endpoint = endpoint.trim_end_matches('/').to_string();
            info!("Using custom S3 endpoint: {}", normalized_endpoint);
            aws_config_builder = aws_config_builder.endpoint_url(normalized_endpoint);
        } else {
            info!("Using default AWS S3 endpoint");
        }
        let aws_config = aws_config_builder.load().await;
        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true)
            .build();
        let client = Client::from_conf(s3_config);
        let bucket_name = config.bucket_name.clone();
        if create_bucket {
            info!("Checking if bucket '{}' exists...", bucket_name);
            match client.head_bucket().bucket(&bucket_name).send().await {
                Ok(_) => {
                    info!("Bucket '{}' already exists", bucket_name);
                }
                Err(e) => {
                    let err_details = format_error_details(&e);
                    debug!("Bucket check failed: {} ({:?})", err_details, e);
                    info!("Creating bucket '{}'", bucket_name);
                    match client.create_bucket().bucket(&bucket_name).send().await {
                        Ok(_) => {
                            info!("Bucket '{}' created successfully", bucket_name);
                        }
                        Err(create_err) => {
                            let create_err_details = format_error_details(&create_err);
                            let err_str = format!("{:?}", create_err);
                            if err_str.contains("BucketAlreadyOwnedByYou")
                                || err_str.contains("BucketAlreadyExists")
                            {
                                info!(
                                    "Bucket '{}' already exists (create returned: {})",
                                    bucket_name, create_err_details
                                );
                            } else {
                                warn!(
                                    "Failed to create bucket '{}': {}. Attempting to use it anyway...",
                                    bucket_name, create_err_details
                                );
                                match client
                                    .list_objects_v2()
                                    .bucket(&bucket_name)
                                    .max_keys(1)
                                    .send()
                                    .await
                                {
                                    Ok(_) => {
                                        info!(
                                            "Bucket '{}' is accessible despite creation error",
                                            bucket_name
                                        );
                                    }
                                    Err(list_err) => {
                                        let error_msg = format!(
                                            "Cannot access bucket '{}'. Create error: {}. List error: {}. Endpoint: {:?}",
                                            bucket_name,
                                            create_err,
                                            list_err,
                                            config.endpoint_url,
                                        );
                                        error!("{}", error_msg);
                                        return Err(CloudStorageError::SdkError(error_msg));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(S3CloudStorage {
            client,
            bucket_name,
        })
    }
    /// Generate S3 key using hash-based partitioning for better distribution
    fn object_key(&self, key: &str) -> String {
        if key.len() < 4 {
            return format!("files/misc/{}", key);
        }
        let prefix = &key[..2];
        let subprefix = &key[2..4];
        format!("files/{}/{}/{}", prefix, subprefix, key)
    }
}
#[async_trait::async_trait]
impl CloudStorage for S3CloudStorage {
    async fn upload(&self, key: &str, data: &[u8]) -> Result<String, CloudStorageError> {
        let s3_key = self.object_key(key);

        debug!("Uploading {} ({} bytes)", key, data.len());
        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&s3_key)
            .body(data.to_vec().into())
            .content_type("application/octet-stream")
            .send()
            .await
            .map_err(|e| CloudStorageError::SdkError(format!("Put object failed: {}", e)))?;
        let storage_location = format!("s3://{}/{}", self.bucket_name, s3_key);

        debug!("Successfully uploaded to {}", storage_location);
        Ok(storage_location)
    }

    async fn download(&self, storage_location: &str) -> Result<Vec<u8>, CloudStorageError> {
        let key = storage_location
            .strip_prefix(&format!("s3://{}/", self.bucket_name))
            .ok_or_else(|| {
                CloudStorageError::Download(format!("Invalid S3 location: {}", storage_location))
            })?;

        debug!("Downloading from {}", storage_location);
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await
            .map_err(|e| CloudStorageError::SdkError(format!("Get object failed: {}", e)))?;
        let data = response.body.collect().await?.into_bytes().to_vec();

        debug!("Successfully downloaded {} bytes", data.len());
        Ok(data)
    }

    async fn delete(&self, storage_location: &str) -> Result<(), CloudStorageError> {
        let key = storage_location
            .strip_prefix(&format!("s3://{}/", self.bucket_name))
            .ok_or_else(|| {
                CloudStorageError::Download(format!("Invalid S3 location: {}", storage_location))
            })?;

        debug!("Deleting from {}", storage_location);
        self.client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await
            .map_err(|e| CloudStorageError::SdkError(format!("Delete object failed: {}", e)))?;

        debug!("Successfully deleted from {}", storage_location);
        Ok(())
    }
}
