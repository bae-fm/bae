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
use bae_core::config::SubsonicCredential;
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

/// Whether `token` equals `md5(password + salt)`. The comparison is
/// case-insensitive: the hex is conventionally lowercase, but a client that
/// upper-cases it still authenticates.
fn tokens_match(token: &str, password: &str, salt: &str) -> bool {
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    let expected = hex::encode(hasher.finalize());
    token.eq_ignore_ascii_case(&expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Params {
        Params(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    fn token(password: &str, salt: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(password.as_bytes());
        hasher.update(salt.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn credential() -> SubsonicCredential {
        SubsonicCredential {
            username: "listener".to_string(),
            password: "hunter2".to_string(),
        }
    }

    #[test]
    fn valid_token_passes() {
        let salt = "abcdef";
        let p = params(&[
            ("u", "listener"),
            ("s", salt),
            ("t", &token("hunter2", salt)),
        ]);
        assert!(check(&p, &credential()).is_ok());
    }

    #[test]
    fn wrong_token_is_40() {
        let p = params(&[("u", "listener"), ("s", "abcdef"), ("t", "deadbeef")]);
        assert_eq!(check(&p, &credential()).unwrap_err().code, 40);
    }

    #[test]
    fn missing_salt_or_token_is_10() {
        let salt = "abcdef";
        let missing_salt = params(&[("u", "listener"), ("t", &token("hunter2", salt))]);
        assert_eq!(check(&missing_salt, &credential()).unwrap_err().code, 10);
        let missing_token = params(&[("u", "listener"), ("s", salt)]);
        assert_eq!(check(&missing_token, &credential()).unwrap_err().code, 10);
    }

    #[test]
    fn short_salt_is_40() {
        let salt = "abcde"; // five chars
        let p = params(&[
            ("u", "listener"),
            ("s", salt),
            ("t", &token("hunter2", salt)),
        ]);
        assert_eq!(check(&p, &credential()).unwrap_err().code, 40);
    }

    #[test]
    fn token_hex_is_case_insensitive() {
        let salt = "abcdef";
        let upper = token("hunter2", salt).to_uppercase();
        let p = params(&[("u", "listener"), ("s", salt), ("t", &upper)]);
        assert!(check(&p, &credential()).is_ok());
    }

    /// The empty default credential authenticates no one: a request crafted to
    /// match it (empty username, a valid-for-empty-password token) is rejected
    /// with code 40. Without the empty-credential guard this request would pass
    /// (`"" == ""` and the token matches md5 of the empty password), leaving the
    /// server open before any credential is configured.
    #[test]
    fn empty_credential_rejects_the_matching_request() {
        let salt = "abcdef";
        let request = params(&[("u", ""), ("s", salt), ("t", &token("", salt))]);
        assert_eq!(
            check(&request, &SubsonicCredential::empty())
                .unwrap_err()
                .code,
            40
        );
    }
}
