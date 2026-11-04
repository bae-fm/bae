//! Request authentication: the salted-token check every endpoint sits behind.
//!
//! The server accepts only modern token auth. A client sends `u` (username), `t`
//! (token), and `s` (salt), where `t = md5(password + s)` in lowercase hex.
//! The server recomputes the token from its one configured password and the
//! client's salt and compares. Legacy `p=` cleartext/hex password auth is not
//! supported — every current Subsonic client sends tokens.
//!
//! The check runs as middleware over the whole router. On failure it emits a
//! proper Subsonic error envelope (in the requested format), never an HTTP 401,
//! because Subsonic clients read the error code out of the envelope body.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use bae_preferences::SubsonicCredential;
use md5::{Digest, Md5};

use crate::envelope::error_response;
use crate::error::SubError;
use crate::params::Params;

/// The minimum salt length Subsonic requires. A shorter salt weakens the token
/// derivation, so the server refuses it as a bad credential.
const MIN_SALT_LEN: usize = 6;

/// Verify a request against `credential`. On success the request proceeds; on
/// failure the response is a Subsonic error envelope in the requested format.
pub(crate) async fn require_auth(
    State(credential): State<Arc<SubsonicCredential>>,
    request: Request,
    next: Next,
) -> Response {
    let params = match Query::<HashMap<String, String>>::try_from_uri(request.uri()) {
        Ok(Query(map)) => Params(map),
        Err(_) => Params(HashMap::new()),
    };
    let format = params.format();
    if let Err(error) = check(&params, &credential) {
        return error_response(&format, &error);
    }
    next.run(request).await
}

fn check(params: &Params, credential: &SubsonicCredential) -> Result<(), SubError> {
    // An unconfigured credential (empty username or password) authenticates no
    // one. Without this guard the empty default would let a crafted request
    // through — `u=""` matches the empty username, and `t=md5("" + salt)` matches
    // the empty password — so the server would be open before any credential is
    // set. Reject before the comparisons.
    if credential.username.is_empty() || credential.password.is_empty() {
        return Err(SubError::wrong_credentials());
    }

    let username = params.require("u")?;
    let token = params.require("t")?;
    let salt = params.require("s")?;

    // A short salt is a rejected credential, not a missing parameter — the
    // parameter is present, it just fails the strength floor.
    if salt.len() < MIN_SALT_LEN {
        return Err(SubError::wrong_credentials());
    }
    if username != credential.username {
        return Err(SubError::wrong_credentials());
    }
    if !tokens_match(token, &credential.password, salt) {
        return Err(SubError::wrong_credentials());
    }
    Ok(())
}

/// The token a request carries for `password` and `salt`:
/// `md5(password + salt)` in lowercase hex.
pub fn token(password: &str, salt: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    hex::encode(hasher.finalize())
}

/// Whether `token` is [`token`] of `password` and `salt`. The comparison is
/// case-insensitive: the hex is conventionally lowercase, but a client that
/// upper-cases it still authenticates.
fn tokens_match(token: &str, password: &str, salt: &str) -> bool {
    token.eq_ignore_ascii_case(&self::token(password, salt))
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
