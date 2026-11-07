use super::*;

fn params(pairs: &[(&str, &str)]) -> Params {
    Params(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

/// The one fixed oracle for the token every other test builds requests with:
/// md5("hunter2" + "abcdef") in lowercase hex.
#[test]
fn token_is_the_md5_of_password_then_salt() {
    assert_eq!(token("hunter2", "abcdef"), "53b35391fb917b85a4bda14204fedf4f");
}

fn credential() -> SubsonicCredential {
    SubsonicCredential { username: "listener".to_string(), password: "hunter2".to_string() }
}

#[test]
fn valid_token_passes() {
    let salt = "abcdef";
    let p = params(&[("u", "listener"), ("s", salt), ("t", &token("hunter2", salt))]);
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
    let p = params(&[("u", "listener"), ("s", salt), ("t", &token("hunter2", salt))]);
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
    assert_eq!(check(&request, &SubsonicCredential::empty()).unwrap_err().code, 40);
}
