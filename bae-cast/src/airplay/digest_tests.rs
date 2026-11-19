use super::*;

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

/// The answer is RFC 2617's response without `qop` and with an empty
/// username. The expected response is a fixed value, its MD5s computed apart
/// from this code (Python's hashlib):
/// `MD5(MD5(":airplay:hunter2") ":9a4b3c:" MD5("SETUP:rtsp://10.0.0.5/1"))`.
#[test]
fn the_answer_hashes_the_password_with_an_empty_username() {
    let challenge = DigestChallenge { realm: "airplay".into(), nonce: "9a4b3c".into() };
    assert_eq!(
        challenge.authorization(DigestStyle::Standard, "hunter2", "SETUP", "rtsp://10.0.0.5/1"),
        r#"Digest username="", realm="airplay", nonce="9a4b3c", uri="rtsp://10.0.0.5/1", response="61b3736a0f06beac20506e1cc717d4dc""#
    );
}

/// A first-generation AirPort Express wants the username `iTunes` and capital
/// hex, in the inner hashes too, which are hashed as text. The expected
/// response is a fixed value computed apart from this code (Python's hashlib,
/// each MD5 in capitals): `MD5(MD5("iTunes:raop:hunter2") ":9a4b3c:"
/// MD5("OPTIONS:*"))`.
#[test]
fn an_airport_express_answer_has_the_username_itunes_and_capital_hex() {
    let challenge = DigestChallenge { realm: "raop".into(), nonce: "9a4b3c".into() };
    assert_eq!(
        challenge.authorization(DigestStyle::AirPortExpress, "hunter2", "OPTIONS", "*"),
        r#"Digest username="iTunes", realm="raop", nonce="9a4b3c", uri="*", response="67C4333A674254D2423BEA888CA7555C""#
    );
}

/// The same challenge answered the standard way, for an OPTIONS request.
#[test]
fn a_standard_answer_to_options_uses_the_uri_star() {
    let challenge = DigestChallenge { realm: "raop".into(), nonce: "9a4b3c".into() };
    assert_eq!(
        challenge.authorization(DigestStyle::Standard, "hunter2", "OPTIONS", "*"),
        r#"Digest username="", realm="raop", nonce="9a4b3c", uri="*", response="ff8da56431d8938488e6d7dc55c21c66""#
    );
}
