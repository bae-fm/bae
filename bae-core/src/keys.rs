//! Names for bae-owned secrets stored through Coven's host-secret capability.

pub(crate) const DISCOGS_API_KEY: &str = "discogs_api_key";
pub(crate) const MCP_BEARER_TOKEN: &str = "mcp_bearer_token";
pub(crate) const SUBSONIC_PASSWORD: &str = "subsonic_password";

/// Every secret bae keeps in a library's keyring beside coven's own, which a
/// library's removal from this device deletes with it.
pub(crate) const HOST_SECRET_NAMES: &[&str] =
    &[DISCOGS_API_KEY, MCP_BEARER_TOKEN, SUBSONIC_PASSWORD];
