//! The OAuth applications bae runs its consumer-cloud sign-ins under.
//!
//! coven ships no OAuth client credentials of its own: every flow — the desktop
//! browser sign-in, the mobile host-driven redirect, restore, join, and opening a
//! library whose cloud home is Google Drive/Dropbox/OneDrive — reads them from a
//! [`coven::OAuthClients`] the host supplies. bae is one app per process with one
//! set of registered OAuth applications shared by every library, so the
//! registration lives here, once, next to [`crate::config::init_keyring`]: the
//! platform app calls `set_client_creds` at startup and every flow reads
//! [`clients`].
//!
//! Before registration — and in builds without the `oauth-providers` feature —
//! [`clients`] returns an empty set. That is the correct value for an S3,
//! CloudKit, or local-only library; an OAuth flow attempted against it fails at
//! the flow boundary naming the missing provider.

use std::sync::RwLock;

use coven::OAuthClients;

static CLIENTS: RwLock<Option<OAuthClients>> = RwLock::new(None);

/// Register the OAuth applications this installation signs in with. Called once
/// at startup, before any OAuth flow and before opening a library whose cloud
/// home is an OAuth provider. Registering again replaces the previous set.
#[cfg(feature = "oauth-providers")]
pub fn set_client_creds(
    creds: std::collections::HashMap<coven::CloudProvider, coven::OAuthClientCreds>,
) -> Result<(), coven::OAuthClientCredsError> {
    let clients = OAuthClients::new(creds)?;
    *CLIENTS.write().unwrap_or_else(|e| e.into_inner()) = Some(clients);
    Ok(())
}

/// The registered OAuth applications, or an empty set when none were registered.
pub fn clients() -> OAuthClients {
    CLIENTS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(OAuthClients::empty)
}
