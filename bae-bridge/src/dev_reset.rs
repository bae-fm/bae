//! The profile a debug app's dev reset recreates its library from. Only debug
//! macOS builds carry it.

use crate::{BridgeError, BridgeS3Profile, BridgeSaveSyncConfig};

/// A dev library's profile. Mirrors `bae_core::dev_reset::DevResetProfile`.
#[derive(uniffi::Record)]
pub struct BridgeDevResetProfile {
    pub library_name: String,
    pub sync_config: BridgeSaveSyncConfig,
    pub discogs_token: String,
    pub identify_automatically: bool,
    pub import_when_identified: bool,
}

/// Read the dev profile at `path`, which must be mode 600.
#[uniffi::export]
pub fn read_dev_reset_profile(path: String) -> Result<BridgeDevResetProfile, BridgeError> {
    let bae_core::dev_reset::DevResetProfile {
        library_name,
        s3,
        discogs_token,
        identify_automatically,
        import_when_identified,
    } = bae_core::dev_reset::DevResetProfile::read(std::path::Path::new(&path))
        .map_err(profile_error)?;
    Ok(BridgeDevResetProfile {
        library_name: library_name.into_string(),
        sync_config: BridgeSaveSyncConfig {
            bucket: s3.bucket,
            region: s3.region,
            endpoint: s3.endpoint,
            key_prefix: s3.key_prefix.into(),
            access_key: s3.access_key,
            secret_key: s3.secret_key,
            profile: s3.profile.map(BridgeS3Profile::from_core),
        },
        discogs_token,
        identify_automatically,
        import_when_identified,
    })
}

/// A refused key prefix keeps its own class, so the line names the folder at
/// fault; the detail names the profile and its `s3.key_prefix` field. Any
/// other profile failure is a configuration error.
fn profile_error(error: bae_core::dev_reset::DevResetProfileError) -> BridgeError {
    use bae_core::dev_reset::DevResetProfileError as E;
    match &error {
        E::Prefix { error: refusal, .. } => {
            BridgeError::from_core(bae_core::ui::UiError::diagnostic(
                bae_core::ui::UiErrorCategory::S3Prefix(refusal.clone()),
                &error,
            ))
        }
        E::Read { .. }
        | E::Exposed { .. }
        | E::Malformed { .. }
        | E::Empty { .. }
        | E::WholeBucket { .. } => BridgeError::config(&error),
    }
}

#[cfg(test)]
#[path = "dev_reset_tests.rs"]
mod tests;
