//! bae's sync boundary around coven.
//!
//! coven owns sync, keys, storage and database connections; bae's shared
//! rows, the columns that reference coven's files and its kinds of file are
//! declared with the schema, in `bae_records::synced`.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. Cloud file operations use the exact stored file reference.
pub use coven::{decode_code_info, restore_from_code};

pub mod membership;
pub mod restore_code;

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    /// The folder in the bucket the library lives under, parsed from what the
    /// person typed; slashes at its ends mean nothing, and empty is the
    /// bucket's root.
    pub key_prefix: coven::S3Prefix,
    pub access_key: String,
    pub secret_key: String,
    pub profile: Option<coven::S3Profile>,
}

impl S3ConfigData {
    /// The coven location these settings name, and the key that reaches it.
    pub fn into_location(
        self,
    ) -> Result<(coven::StorageConfig, coven::S3Credentials), url::ParseError> {
        let endpoint = self.endpoint.map(|value| value.parse()).transpose()?;
        Ok((
            coven::StorageConfig::S3 {
                bucket: self.bucket,
                region: self.region,
                endpoint,
                prefix: self.key_prefix,
                profile: self.profile,
            },
            coven::S3Credentials {
                access_key_id: self.access_key,
                secret_access_key: coven::SecretText::new(self.secret_key),
            },
        ))
    }
}
