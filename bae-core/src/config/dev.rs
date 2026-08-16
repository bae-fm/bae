use tracing::warn;

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

pub(crate) struct DevSecrets {
    pub(crate) master_key: Option<String>,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) discogs_api_key: Option<String>,
}

/// Read dev-mode secrets once at the composition root. The opened Coven handle
/// remains their only route into custody; bae never binds Coven's retained key
/// store itself.
pub(crate) fn dev_secrets() -> DevSecrets {
    if !dev_mode_enabled() {
        return DevSecrets {
            master_key: None,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            discogs_api_key: None,
        };
    }
    DevSecrets {
        master_key: dev_env_secret("BAE_ENCRYPTION_KEY"),
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        discogs_api_key: dev_env_secret("BAE_DISCOGS_API_KEY"),
    }
}
