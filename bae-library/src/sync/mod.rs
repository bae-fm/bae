//! bae's sync boundary around coven.
//!
//! coven owns sync, keys, storage and database connections; bae's shared
//! rows, the columns that reference coven's files and its kinds of file are
//! declared with the schema, in `bae_records::synced`.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. Cloud file operations use the exact stored file reference.
pub use coven::restore_from_code;

/// What a code a person entered holds: typed in the groups it shows in or
/// pasted, as [`restore_code::entered`] reads it.
pub fn decode_code_info(text: &str) -> Result<coven::CodeInfo, coven::CodeError> {
    coven::decode_code_info(&restore_code::entered(text))
}

pub mod membership;
pub mod restore_code;

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    pub profile: Option<coven::S3Profile>,
}

/// The S3 location these fields name: `endpoint` is parsed here, the one
/// place a typed endpoint becomes a URL.
pub fn s3_location(
    bucket: String,
    region: String,
    endpoint: Option<String>,
    profile: Option<coven::S3Profile>,
) -> Result<coven::StorageConfig, url::ParseError> {
    let endpoint = endpoint.map(|value| value.parse()).transpose()?;
    Ok(coven::StorageConfig::S3 { bucket, region, endpoint, profile })
}

impl S3ConfigData {
    /// The coven location these settings name, and the key that reaches it.
    pub fn into_location(
        self,
    ) -> Result<(coven::StorageConfig, coven::S3Credentials), url::ParseError> {
        Ok((
            s3_location(self.bucket, self.region, self.endpoint, self.profile)?,
            coven::S3Credentials {
                access_key_id: self.access_key,
                secret_access_key: coven::SecretText::new(self.secret_key),
            },
        ))
    }
}
