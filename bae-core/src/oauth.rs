//! Re-export of coven's OAuth helper. The OAuth flow lives in coven now; this
//! keeps bae's `crate::oauth::…` call sites resolving unchanged. Provider client
//! credentials are registered at startup via `coven::oauth::set_oauth_client_creds`.
pub use coven::oauth::*;
