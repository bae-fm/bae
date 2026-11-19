//! The HTTP Digest a test receiver with a password asks for: it answers a
//! request that lacks the right answer with a 401 and a challenge, and checks
//! the answer against a response it computes from the RFC's formula, typed
//! out apart from [`super::digest`].

use md5::{Digest, Md5};

use super::digest::DigestStyle;
use super::rtsp::{Method, ParsedRequest};

const REALM: &str = "raop";
const NONCE: &str = "5f3a91c2";

/// The header a 401 carries to challenge the sender.
pub(crate) const CHALLENGE: &str =
    "WWW-Authenticate: Digest realm=\"raop\", nonce=\"5f3a91c2\"\r\n";

/// Which requests the receiver wants the password on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Challenges {
    /// Every request, OPTIONS included (shairport-sync).
    EveryRequest,
    /// The session's own requests, ANNOUNCE and after, but not OPTIONS or the
    /// POSTs that pair or verify the device.
    Session,
}

/// One request the gate took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Taken {
    pub(crate) method: String,
    /// What it answered: its status.
    pub(crate) status: u16,
    /// Whether the request carried an `Authorization` header.
    pub(crate) authorized: bool,
}

/// The password a receiver asks for, and what it was asked.
pub(crate) struct DigestGate {
    password: String,
    style: DigestStyle,
    challenges: Challenges,
    /// Every request it checked, in order, over every connection.
    pub(crate) taken: Vec<Taken>,
}

impl DigestGate {
    pub(crate) fn new(password: &str, style: DigestStyle, challenges: Challenges) -> Self {
        DigestGate { password: password.to_string(), style, challenges, taken: Vec::new() }
    }

    /// Change the password, as a person does on the receiver.
    pub(crate) fn set_password(&mut self, password: &str) {
        self.password = password.to_string();
    }

    /// The methods of the requests it answered with `status`.
    pub(crate) fn methods_answered(&self, status: u16) -> Vec<&str> {
        self.taken.iter().filter(|t| t.status == status).map(|t| t.method.as_str()).collect()
    }

    /// Whether the request may go on: it needs no password here, or it
    /// carries the answer to the challenge. Records the request.
    pub(super) fn lets_through(&mut self, request: &ParsedRequest) -> bool {
        let needs_password = self.challenges == Challenges::EveryRequest
            || !matches!(request.method, Method::Options | Method::Post);
        let allowed = !needs_password || self.answers_correctly(request);
        self.taken.push(Taken {
            method: request.method.as_str().to_string(),
            status: if allowed { 200 } else { 401 },
            authorized: request.header("Authorization").is_some(),
        });
        allowed
    }

    /// Whether the request carries the answer to this receiver's challenge
    /// for its own method and URI, from its own username and password, with
    /// the hex in the case its style wants.
    fn answers_correctly(&self, request: &ParsedRequest) -> bool {
        let Some(header) = request.header("Authorization") else {
            return false;
        };
        let field = |name: &str| {
            let after = header.split(&format!("{name}=\"")).nth(1)?;
            Some(after.split('"').next()?.to_string())
        };
        let (user, hex): (&str, fn(Vec<u8>) -> String) = match self.style {
            DigestStyle::Standard => ("", hex::encode),
            DigestStyle::AirPortExpress => ("iTunes", hex::encode_upper),
        };
        let md5 = |text: String| hex(Md5::digest(text.as_bytes()).to_vec());
        let ha1 = md5(format!("{user}:{REALM}:{}", self.password));
        let ha2 = md5(format!("{}:{}", request.method.as_str(), request.uri));
        let expected = md5(format!("{ha1}:{NONCE}:{ha2}"));
        header.starts_with("Digest ")
            && field("username").as_deref() == Some(user)
            && field("realm").as_deref() == Some(REALM)
            && field("nonce").as_deref() == Some(NONCE)
            && field("uri").as_deref() == Some(request.uri.as_str())
            && field("response").as_deref() == Some(expected.as_str())
    }
}
