use tracing::{info, warn};

/// Read a dev-mode `BAE_*` env var, distinguishing the three outcomes that
/// matter: absent (skip silently — the common case for an unset secret), present
/// but non-UTF-8 (a misconfigured `.env`, warned and skipped so it isn't
/// silently treated as absent), and present with a non-empty value (returned for
/// seeding). An empty value is treated as absent.
pub(super) fn dev_env_secret(var: &str) -> Option<String> {
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

type CloudHomeS3Setter = fn(&mut coven::CloudHomeConfig, String);

pub(super) fn overlay_cloud_home_s3_env(cloud_home: &mut coven::CloudHomeConfig) {
    let overlays: [(&str, CloudHomeS3Setter); 4] = [
        ("BAE_CLOUD_HOME_S3_BUCKET", |cloud_home, value| {
            cloud_home.s3_bucket = Some(value)
        }),
        ("BAE_CLOUD_HOME_S3_REGION", |cloud_home, value| {
            cloud_home.s3_region = Some(value)
        }),
        ("BAE_CLOUD_HOME_S3_ENDPOINT", |cloud_home, value| {
            cloud_home.s3_endpoint = Some(value)
        }),
        ("BAE_CLOUD_HOME_S3_KEY_PREFIX", |cloud_home, value| {
            cloud_home.s3_key_prefix = Some(value)
        }),
    ];

    for (var, apply) in overlays {
        if let Some(value) = dev_env_secret(var) {
            apply(cloud_home, value);
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
/// coven's `KeyService` reads secrets from the keyring. In dev mode bae's secrets
/// live in env vars (`.env` / `BAE_ENCRYPTION_KEY` / `BAE_CLOUD_HOME_CREDENTIALS`
/// / `BAE_DISCOGS_API_KEY`), so before coven reads them bae seeds each present
/// env value into the keyring account coven reads from — through coven's own
/// `KeyService` setters, not a hand-rolled keyring entry. Production is a no-op:
/// `is_dev_mode()` is false, so coven reads the OS keyring directly.
///
/// Call once after the keyring store is installed (`init_keyring`) and the
/// `library_id` is known, before constructing coven's `KeyService`.
pub fn seed_dev_keyring(library_id: &str) {
    if !dev_mode_enabled() {
        return;
    }

    let keys = coven::KeyService::new(library_id.to_string());

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

    // bae's own Discogs API key — a bae-domain credential with no coven setter,
    // written through bae's own keyring path (`BaeKeyServiceExt::set_discogs_key`).
    if let Some(discogs) = dev_env_secret("BAE_DISCOGS_API_KEY") {
        use crate::keys::BaeKeyServiceExt;
        match keys.set_discogs_key(&discogs) {
            Ok(()) => info!("dev: seeded discogs api key from env"),
            Err(e) => warn!("dev: failed to seed discogs api key: {e}"),
        }
    }
}
