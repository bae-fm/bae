//! Re-export of coven's store key material plus bae's domain credentials.
//!
//! coven owns the key/keyring primitives and splits its keyring into two scopes:
//! `DeviceKeys`, the device-global Ed25519 signing identity (no store_id), and
//! `StoreKeys`, a store's encryption key, cloud credentials, and OAuth tokens
//! under store-scoped accounts. coven is domain-agnostic and reads secrets only
//! from the keyring, so bae's Discogs API key and MCP bearer token are layered
//! back onto `StoreKeys` by the extension trait below, under the same
//! `base:store_id` account scheme. In dev mode `config::seed_dev_keyring` bridges
//! `BAE_DISCOGS_API_KEY` into that account, as it does coven's own env vars.
use tracing::{info, warn};

pub use coven::{CloudHomeCredentials, DeviceKeys, KeyError, StoreKeys};
// coven's keyring read/write is a closed typed-slot API that can't name bae's
// accounts, so bae goes through `keyring_core` directly — against coven's service
// name, so both live in one keyring service.
use coven::keyring_service;

/// A namespaced keyring account, matching coven's own `base:store_id` scheme.
fn account(ks: &StoreKeys, base: &str) -> String {
    format!("{}:{}", base, ks.store_id())
}

fn map_keyring_error(e: keyring_core::Error) -> KeyError {
    KeyError::Persistence(e.to_string())
}

fn set_keyring_credential(
    ks: &StoreKeys,
    account_base: &str,
    value: &str,
    saved_message: &'static str,
) -> Result<(), KeyError> {
    keyring_core::Entry::new(keyring_service()?, &account(ks, account_base))
        .map_err(map_keyring_error)?
        .set_password(value)
        .map_err(map_keyring_error)?;
    info!("{saved_message}");
    Ok(())
}

fn delete_keyring_credential(
    ks: &StoreKeys,
    account_base: &str,
    deleted_message: &'static str,
    missing_message: Option<&'static str>,
) -> Result<(), KeyError> {
    match keyring_core::Entry::new(keyring_service()?, &account(ks, account_base))
        .map_err(map_keyring_error)?
        .delete_credential()
    {
        Ok(()) => {
            info!("{deleted_message}");
            Ok(())
        }
        Err(keyring_core::Error::NoEntry) => {
            if let Some(message) = missing_message {
                warn!("{message}");
            }
            Ok(())
        }
        Err(e) => Err(map_keyring_error(e)),
    }
}

fn read_keyring_credential(ks: &StoreKeys, account_base: &str) -> Result<Option<String>, KeyError> {
    match keyring_core::Entry::new(keyring_service()?, &account(ks, account_base))
        .map_err(map_keyring_error)?
        .get_password()
    {
        Ok(value) => Ok(Some(value)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(map_keyring_error(e)),
    }
}

/// bae-domain credentials layered on coven's `StoreKeys`: the Discogs API key
/// and the local MCP bearer token. Bring this trait into scope to call these on
/// a `StoreKeys`.
///
/// The getters return `Result<Option<T>, KeyError>` for the same reason coven's
/// do: `Ok(None)` is "not configured", `Err` is a real failure (keyring backend
/// error, malformed stored bytes) — collapsing those to `None` would hide corrupt
/// local state.
pub trait BaeStoreKeysExt {
    fn get_discogs_key(&self) -> Result<Option<String>, KeyError>;
    fn set_discogs_key(&self, value: &str) -> Result<(), KeyError>;
    fn delete_discogs_key(&self) -> Result<(), KeyError>;
    fn get_mcp_token(&self) -> Result<Option<String>, KeyError>;
    fn set_mcp_token(&self, value: &str) -> Result<(), KeyError>;
    /// Remove this library's encryption key from the keyring. The running sync
    /// manager still holds it in memory, so the session keeps working — the lock
    /// takes effect on the next launch, which lands on the unlock screen.
    /// Succeeds if the entry is already gone.
    fn forget_encryption_key(&self) -> Result<(), KeyError>;
}

impl BaeStoreKeysExt for StoreKeys {
    fn get_discogs_key(&self) -> Result<Option<String>, KeyError> {
        read_keyring_credential(self, "discogs_api_key")
    }

    fn set_discogs_key(&self, value: &str) -> Result<(), KeyError> {
        set_keyring_credential(
            self,
            "discogs_api_key",
            value,
            "Discogs API key saved to keyring",
        )
    }

    fn delete_discogs_key(&self) -> Result<(), KeyError> {
        delete_keyring_credential(
            self,
            "discogs_api_key",
            "Discogs API key deleted from keyring",
            Some("Tried to delete Discogs key but none was stored"),
        )
    }

    fn get_mcp_token(&self) -> Result<Option<String>, KeyError> {
        read_keyring_credential(self, "mcp_bearer_token")
    }

    fn set_mcp_token(&self, value: &str) -> Result<(), KeyError> {
        set_keyring_credential(
            self,
            "mcp_bearer_token",
            value,
            "MCP bearer token saved to keyring",
        )
    }

    fn forget_encryption_key(&self) -> Result<(), KeyError> {
        delete_keyring_credential(
            self,
            "encryption_master_key",
            "Forgot encryption key for active library",
            None,
        )
    }
}
