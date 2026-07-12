use tracing::{info, warn};

/// Read a dev-mode `BAE_*` env var. Absent or empty is `None` (the common case
/// for an unset secret); a present but non-UTF-8 value is a misconfigured `.env`,
/// warned about rather than silently treated as absent.
fn dev_env_secret(var: &str) -> Option<String> {
    match std::env::var(var) {
        Ok(value) if !value.is_empty() => Some(value),
        Ok(_) => None,
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(raw)) => {
            warn!("dev: ignoring non-UTF-8 {var}: {raw:?}");
            None
        }
    }
}

pub(super) fn dev_mode_enabled() -> bool {
    std::env::var("BAE_DEV_MODE").is_ok() || {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            dotenvy::dotenv().is_ok()
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            false
        }
    }
}

/// Bridge bae's dev-mode `BAE_*` env vars into coven's keyring for one library.
///
/// coven reads secrets only from the keyring, but in dev mode bae's live in env
/// vars (`.env`), so each present value is written into the account coven reads
/// from — through coven's own `StoreKeys` setters, not a hand-rolled entry.
/// A no-op in production, where coven reads the OS keyring directly.
///
/// Call once after `init_keyring` and before constructing coven's `StoreKeys`.
pub fn seed_dev_keyring(library_id: &str) {
    if !dev_mode_enabled() {
        return;
    }

    let keys = coven::StoreKeys::new(library_id.to_string());

    if let Some(key) = dev_env_secret("BAE_ENCRYPTION_KEY") {
        match keys.set_encryption_key(&key) {
            Ok(()) => info!("dev: seeded encryption key from env"),
            Err(e) => warn!("dev: failed to seed encryption key: {e}"),
        }
    }

    if let Some(creds_json) = dev_env_secret("BAE_CLOUD_HOME_CREDENTIALS") {
        match serde_json::from_str::<coven::CloudHomeCredentials>(&creds_json) {
            Ok(creds) => match keys.set_cloud_home_credentials(&creds) {
                Ok(()) => info!("dev: seeded cloud home credentials from env"),
                Err(e) => warn!("dev: failed to seed cloud home credentials: {e}"),
            },
            Err(e) => warn!("dev: ignoring malformed BAE_CLOUD_HOME_CREDENTIALS JSON: {e}"),
        }
    }

    // A bae-domain credential: no coven setter, so it goes through bae's own
    // keyring extension trait.
    if let Some(discogs) = dev_env_secret("BAE_DISCOGS_API_KEY") {
        use crate::keys::BaeStoreKeysExt;
        match keys.set_discogs_key(&discogs) {
            Ok(()) => info!("dev: seeded discogs api key from env"),
            Err(e) => warn!("dev: failed to seed discogs api key: {e}"),
        }
    }
}
