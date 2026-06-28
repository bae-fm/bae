//! Re-export of coven's key service plus bae's domain credentials.
//!
//! The key/keyring primitives live in coven now; this re-exports them so bae's
//! `crate::keys::…` call sites resolve unchanged. coven is domain-agnostic and
//! reads secrets only from the keyring, so bae's Discogs API key is layered back
//! onto coven's `KeyService` as another keyring-backed credential via an
//! extension trait, composing coven's `read_keyring` / `keyring_service` /
//! `library_id`. In dev mode bae's `BAE_DISCOGS_API_KEY` env value is bridged
//! into this account by `config::seed_dev_keyring`, the same way coven's own
//! encryption-key/credentials env vars are.
use tracing::{info, warn};

// `read_keyring` / `keyring_service` back the bae-domain keyring credentials
// below (Discogs key, encryption-key forget).
pub use coven::{keyring_service, read_keyring, CloudHomeCredentials, KeyError, KeyService};

/// A namespaced keyring account, matching coven's own `base:library_id` scheme.
fn account(ks: &KeyService, base: &str) -> String {
    format!("{}:{}", base, ks.library_id())
}

/// bae-domain credentials layered on coven's `KeyService`: the Discogs API key.
/// Bring this trait into scope to call these on a `KeyService`.
///
/// Read getters return `Result<Option<T>, KeyError>` for the same reason as
/// coven's own getters: `Ok(None)` is "not configured," `Err` is a real
/// failure (keyring backend error, malformed stored bytes) — silently
/// collapsing those to `None` hides corrupt local state.
pub trait BaeKeyServiceExt {
    fn get_discogs_key(&self) -> Result<Option<String>, KeyError>;
    fn set_discogs_key(&self, value: &str) -> Result<(), KeyError>;
    fn delete_discogs_key(&self) -> Result<(), KeyError>;
    /// Remove the active library's encryption key from the keyring. The
    /// running sync_manager still holds the key in memory, so this
    /// session keeps working — the lock only takes effect on next
    /// launch (which routes through UnlockView). Silently succeeds if
    /// the entry is already gone.
    fn forget_encryption_key(&self) -> Result<(), KeyError>;
}

impl BaeKeyServiceExt for KeyService {
    fn get_discogs_key(&self) -> Result<Option<String>, KeyError> {
        read_keyring(&account(self, "discogs_api_key"))
    }

    fn set_discogs_key(&self, value: &str) -> Result<(), KeyError> {
        keyring_core::Entry::new(keyring_service(), &account(self, "discogs_api_key"))?
            .set_password(value)?;
        info!("Discogs API key saved to keyring");
        Ok(())
    }

    fn delete_discogs_key(&self) -> Result<(), KeyError> {
        match keyring_core::Entry::new(keyring_service(), &account(self, "discogs_api_key"))?
            .delete_credential()
        {
            Ok(()) => {
                info!("Discogs API key deleted from keyring");
                Ok(())
            }
            Err(keyring_core::Error::NoEntry) => {
                warn!("Tried to delete Discogs key but none was stored");
                Ok(())
            }
            Err(e) => Err(KeyError::Keyring(e)),
        }
    }

    fn forget_encryption_key(&self) -> Result<(), KeyError> {
        match keyring_core::Entry::new(keyring_service(), &account(self, "encryption_master_key"))?
            .delete_credential()
        {
            Ok(()) => {
                info!("Forgot encryption key for active library");
                Ok(())
            }
            Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyError::Keyring(e)),
        }
    }
}
