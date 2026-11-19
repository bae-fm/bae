//! HTTP Digest authentication (RFC 2617, without `qop`), which a receiver
//! with a password asks for on its RTSP requests: an AirPlay 2 receiver after
//! pair-verify, at SETUP, and an AirPlay 1 receiver on any request.
//!
//! The answer is OwnTone's (`src/outputs/airplay.c:721-820`, `raop.c:879-990`):
//! `response = MD5(MD5(user ":" realm ":" password) ":" nonce ":" MD5(method
//! ":" uri))`, with an empty username and lowercase hex, except for a
//! first-generation AirPort Express ([`DigestStyle::AirPortExpress`]).

use md5::{Digest, Md5};

/// How a receiver wants its Digest answer written: the username and the case
/// of the hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestStyle {
    /// An empty username and lowercase hex.
    Standard,
    /// The username `iTunes` and capital hex, which a first-generation AirPort
    /// Express wants (OwnTone `raop.c:894-915`, `:2220-2224`, `:4392-4394`).
    AirPortExpress,
}

impl DigestStyle {
    fn username(self) -> &'static str {
        match self {
            DigestStyle::Standard => "",
            DigestStyle::AirPortExpress => "iTunes",
        }
    }

    fn hex(self, bytes: impl AsRef<[u8]>) -> String {
        match self {
            DigestStyle::Standard => hex::encode(bytes),
            DigestStyle::AirPortExpress => hex::encode_upper(bytes),
        }
    }
}

/// The password a receiver asks for as HTTP Digest, with how it wants the
/// answer written.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DigestPassword {
    pub(crate) password: String,
    pub(crate) style: DigestStyle,
}

/// The password is a secret: debug output names the style only.
impl std::fmt::Debug for DigestPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DigestPassword").field("style", &self.style).finish_non_exhaustive()
    }
}

/// What a 401 means to a sender that holds a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unauthorized {
    /// The first 401: the receiver's challenge is kept, and the request goes
    /// again with the answer.
    Challenged,
    /// A 401 after the receiver was answered: it refuses the password.
    Refused,
    /// The first 401 states no Digest challenge bae can read.
    Unreadable,
}

/// A password, and the challenge that asked for it once the receiver has
/// asked: every later request carries the answer to it.
pub(crate) struct DigestAnswer {
    password: DigestPassword,
    challenge: Option<DigestChallenge>,
}

impl DigestAnswer {
    pub(crate) fn new(password: DigestPassword) -> Self {
        DigestAnswer { password, challenge: None }
    }

    /// The `Authorization` value for a `method` request to `uri`, once the
    /// receiver has challenged.
    pub(crate) fn authorization(&self, method: &str, uri: &str) -> Option<String> {
        let DigestPassword { password, style } = &self.password;
        Some(self.challenge.as_ref()?.authorization(*style, password, method, uri))
    }

    /// Take what a 401 means, given its `WWW-Authenticate` header: the first
    /// one keeps the challenge it states.
    pub(crate) fn unauthorized(&mut self, www_authenticate: Option<&str>) -> Unauthorized {
        if self.challenge.is_some() {
            return Unauthorized::Refused;
        }
        match www_authenticate.and_then(DigestChallenge::from_header) {
            Some(challenge) => {
                self.challenge = Some(challenge);
                Unauthorized::Challenged
            }
            None => Unauthorized::Unreadable,
        }
    }
}

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
    /// for a `method` request to `uri`, written as `style` says.
    pub(crate) fn authorization(
        &self,
        style: DigestStyle,
        password: &str,
        method: &str,
        uri: &str,
    ) -> String {
        let user = style.username();
        let md5_hex = |text: &str| style.hex(Md5::digest(text.as_bytes()));
        let credentials = md5_hex(&format!("{user}:{}:{password}", self.realm));
        let request = md5_hex(&format!("{method}:{uri}"));
        let response = md5_hex(&format!("{credentials}:{}:{request}", self.nonce));
        format!(
            "Digest username=\"{user}\", realm=\"{}\", nonce=\"{}\", uri=\"{uri}\", response=\"{response}\"",
            self.realm, self.nonce
        )
    }
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
