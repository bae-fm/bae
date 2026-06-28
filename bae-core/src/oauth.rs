//! Re-export of coven's OAuth helper. The OAuth flow lives in coven now; this
//! keeps bae's `crate::oauth::…` call sites resolving unchanged. Provider client
//! credentials are registered at startup via `coven::oauth::set_oauth_client_creds`.
// `OAuthTokens`/`OAuthClientCreds` are the DTOs bae passes through setup/restore.
pub use coven::{set_oauth_client_creds, OAuthClientCreds, OAuthTokens};
#[cfg(feature = "oauth-providers")]
pub use coven::{authorize_provider, build_authorize_request_for_provider, exchange_code_for_provider};
