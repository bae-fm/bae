use super::*;
use md5::{Digest, Md5};

/// RFC 2617's response, computed here apart from `authorization`.
fn expected_response(realm: &str, password: &str, nonce: &str, method: &str, uri: &str) -> String {
    let md5 = |text: String| hex::encode(Md5::digest(text.as_bytes()));
    let ha1 = md5(format!(":{realm}:{password}"));
    let ha2 = md5(format!("{method}:{uri}"));
    md5(format!("{ha1}:{nonce}:{ha2}"))
}

#[test]
fn a_challenge_reads_its_realm_and_nonce() {
    let challenge =
        DigestChallenge::from_header(r#"Digest realm="airplay", nonce="9a4b3c""#).unwrap();
    assert_eq!(challenge, DigestChallenge { realm: "airplay".into(), nonce: "9a4b3c".into() });
}

#[test]
fn a_challenge_without_a_nonce_or_of_another_scheme_is_not_digest() {
    assert_eq!(DigestChallenge::from_header(r#"Digest realm="airplay""#), None);
    assert_eq!(DigestChallenge::from_header(r#"Basic realm="airplay""#), None);
}

#[test]
fn the_answer_hashes_the_password_with_an_empty_username() {
    let challenge = DigestChallenge { realm: "airplay".into(), nonce: "9a4b3c".into() };
    let header = challenge.authorization("hunter2", "SETUP", "rtsp://10.0.0.5/1");
    let response = expected_response("airplay", "hunter2", "9a4b3c", "SETUP", "rtsp://10.0.0.5/1");
    assert_eq!(
        header,
        format!(
            r#"Digest username="", realm="airplay", nonce="9a4b3c", uri="rtsp://10.0.0.5/1", response="{response}""#
        )
    );
}
