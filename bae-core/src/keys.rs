//! Re-export of coven's store key material plus bae's domain credentials.
//!
//! The key/keyring primitives live in coven now; this re-exports them so bae's
//! `crate::keys::…` call sites resolve unchanged. coven splits its keyring into
//! two scopes: `DeviceKeys` is the device-global Ed25519 signing identity
//! (stateless, no store_id), and `StoreKeys` carries a store's encryption key,
//! cloud credentials, and OAuth tokens under store-scoped accounts. coven is
//! domain-agnostic and reads secrets only from the keyring, so bae's Discogs API
//! key is layered back onto `StoreKeys` as another store-scoped keyring
//! credential via an extension trait, using coven's `keyring_service` /
//! `store_id` to scope a `keyring_core` entry it reads and writes directly
//! (coven's own keyring API is typed to its own slots). MCP's local bearer
//! token uses the same account
//! scheme. In dev mode bae's `BAE_DISCOGS_API_KEY` env value is bridged into
//! this account by `config::seed_dev_keyring`, the same way coven's own
//! encryption-key / credentials env vars are.
use tracing::{info, warn};

pub use coven::{CloudHomeCredentials, DeviceKeys, KeyError, StoreKeys};
// `keyring_service` names the keyring service the bae-domain credentials below
// (Discogs key, MCP token, encryption-key forget) live under; used only here, so
// it stays a private import rather than re-exported `bae_core::keys` surface.
// coven's own keyring read/write is a closed typed-slot API (`KeyringSlot`) that
// can't name bae's accounts, so bae reads and writes its accounts through
// `keyring_core` directly against that same service name.
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
/// Read getters return `Result<Option<T>, KeyError>` for the same reason as
/// coven's own getters: `Ok(None)` is "not configured," `Err` is a real
/// failure (keyring backend error, malformed stored bytes) — silently
/// collapsing those to `None` hides corrupt local state.
pub trait BaeStoreKeysExt {
    fn get_discogs_key(&self) -> Result<Option<String>, KeyError>;
    fn set_discogs_key(&self, value: &str) -> Result<(), KeyError>;
    fn delete_discogs_key(&self) -> Result<(), KeyError>;
    fn get_mcp_token(&self) -> Result<Option<String>, KeyError>;
    fn set_mcp_token(&self, value: &str) -> Result<(), KeyError>;
    /// Remove the active library's encryption key from the keyring. The
    /// running sync_manager still holds the key in memory, so this
    /// session keeps working — the lock only takes effect on next
    /// launch (which routes through UnlockView). Silently succeeds if
    /// the entry is already gone.
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
