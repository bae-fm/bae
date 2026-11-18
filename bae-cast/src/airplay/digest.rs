//! HTTP Digest authentication (RFC 2617, without `qop`), which a receiver
//! with a password asks for on its RTSP requests: an AirPlay 2 receiver after
//! pair-verify, at SETUP, and an AirPlay 1 receiver on any request.
//!
//! The answer is OwnTone's (`src/outputs/airplay.c:721-820`, `raop.c:879-990`):
//! an empty username, and `response = MD5(MD5(":" realm ":" password) ":"
//! nonce ":" MD5(method ":" uri))` in lowercase hex.

use md5::{Digest, Md5};

/// The `realm` and `nonce` a receiver's `WWW-Authenticate: Digest …` states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DigestChallenge {
    realm: String,
    nonce: String,
}

impl DigestChallenge {
    /// Read a `WWW-Authenticate` value. `None` for a scheme other than
    /// Digest, or one without a realm and a nonce.
    pub(crate) fn from_header(value: &str) -> Option<Self> {
        let params = value.trim().strip_prefix("Digest")?;
        let mut realm = None;
        let mut nonce = None;
        for param in params.split(',') {
            let Some((key, value)) = param.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"').to_string();
            match key.trim() {
                "realm" => realm = Some(value),
                "nonce" => nonce = Some(value),
                _ => {}
            }
        }
        Some(DigestChallenge { realm: realm?, nonce: nonce? })
    }

    /// The `Authorization` value that answers this challenge with `password`
    /// for a `method` request to `uri`.
    pub(crate) fn authorization(&self, password: &str, method: &str, uri: &str) -> String {
        let credentials = md5_hex(&format!(":{}:{password}", self.realm));
        let request = md5_hex(&format!("{method}:{uri}"));
        let response = md5_hex(&format!("{credentials}:{}:{request}", self.nonce));
        format!(
            "Digest username=\"\", realm=\"{}\", nonce=\"{}\", uri=\"{uri}\", response=\"{response}\"",
            self.realm, self.nonce
        )
    }
}

fn md5_hex(text: &str) -> String {
    hex::encode(Md5::digest(text.as_bytes()))
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
