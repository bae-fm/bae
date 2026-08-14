//! Re-export of coven's store key material plus bae's domain credentials.
//!
//! The key/keyring primitives live in coven now; this re-exports them so bae's
//! `crate::keys::…` call sites resolve unchanged. coven's signing identity is
//! scoped per (store, device) — established via `IdentityCustody` as part of
//! creating/joining/restoring a store, the identity sibling of `KeyCustody`
//! for the master key — and `StoreKeys` carries a store's encryption key,
//! cloud credentials, and OAuth tokens under store-scoped accounts. coven is
//! domain-agnostic and reads secrets only from the keyring, so bae's Discogs API
//! key and MCP bearer token are layered back onto `StoreKeys` as another
//! store-scoped keyring credential via the extension trait below, under the same
//! `base:store_id` account scheme. In dev mode `config::seed_dev_keyring` bridges
//! `BAE_DISCOGS_API_KEY` into that account, as it does coven's own env vars.
pub use coven::{CloudHomeCredentials, KeyError, StoreKeys};

const DISCOGS_API_KEY: &str = "discogs_api_key";
const MCP_BEARER_TOKEN: &str = "mcp_bearer_token";
const SUBSONIC_PASSWORD: &str = "subsonic_password";

/// bae-domain credentials layered on coven's `StoreKeys`: the Discogs API key,
/// the local MCP bearer token, and the Subsonic server password. Bring this
/// trait into scope to call these on a `StoreKeys`.
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
    fn get_subsonic_password(&self) -> Result<Option<String>, KeyError>;
    fn set_subsonic_password(&self, value: &str) -> Result<(), KeyError>;
    /// Remove this library's encryption key from the keyring. The running sync
    /// manager still holds it in memory, so the session keeps working — the lock
    /// takes effect on the next launch, which lands on the unlock screen.
    /// Succeeds if the entry is already gone.
    fn forget_encryption_key(&self) -> Result<(), KeyError>;
}

impl BaeStoreKeysExt for StoreKeys {
    fn get_discogs_key(&self) -> Result<Option<String>, KeyError> {
        self.get_host_secret(DISCOGS_API_KEY)
    }

    fn set_discogs_key(&self, value: &str) -> Result<(), KeyError> {
        self.set_host_secret(DISCOGS_API_KEY, value)
    }

    fn delete_discogs_key(&self) -> Result<(), KeyError> {
        self.delete_host_secret(DISCOGS_API_KEY)
    }

    fn get_mcp_token(&self) -> Result<Option<String>, KeyError> {
        self.get_host_secret(MCP_BEARER_TOKEN)
    }

    fn set_mcp_token(&self, value: &str) -> Result<(), KeyError> {
        self.set_host_secret(MCP_BEARER_TOKEN, value)
    }

    fn get_subsonic_password(&self) -> Result<Option<String>, KeyError> {
        self.get_host_secret(SUBSONIC_PASSWORD)
    }

    fn set_subsonic_password(&self, value: &str) -> Result<(), KeyError> {
        self.set_host_secret(SUBSONIC_PASSWORD, value)
    }

    fn forget_encryption_key(&self) -> Result<(), KeyError> {
        self.delete_encryption_key()
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
