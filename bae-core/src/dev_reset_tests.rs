use super::*;
use std::os::unix::fs::PermissionsExt;

const PROFILE: &str = r#"
library_name = "dev"
identify_automatically = false
import_when_identified = true

[s3]
endpoint = "https://storage.example.test"
region = "region-1"
bucket = "bucket-name"
key_prefix = "prefix/path"
access_key_id = "access-key-id"
secret_access_key = "secret-access-key"

[discogs]
token = "discogs-token"
"#;

fn write(dir: &tempfile::TempDir, text: &str, mode: u32) -> PathBuf {
    let path = dir.path().join("profile.toml");
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

#[test]
fn a_private_profile_reads_into_the_library_it_describes() {
    let dir = tempfile::tempdir().unwrap();
    let profile = DevResetProfile::read(&write(&dir, PROFILE, 0o600)).unwrap();

    assert_eq!(profile.library_name.as_str(), "dev");
    assert!(!profile.identify_automatically);
    assert!(profile.import_when_identified);
    assert_eq!(profile.discogs_token, "discogs-token");
    assert_eq!(profile.s3.bucket, "bucket-name");
    assert_eq!(profile.s3.region, "region-1");
    assert_eq!(
        profile.s3.endpoint.as_deref(),
        Some("https://storage.example.test")
    );
    assert_eq!(profile.s3.key_prefix.as_str(), "prefix/path");
    assert_eq!(profile.s3.access_key, "access-key-id");
    assert_eq!(profile.s3.secret_key, "secret-access-key");
    assert!(profile.s3.profile.is_none());
}

#[test]
fn a_profile_others_can_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = DevResetProfile::read(&write(&dir, PROFILE, 0o644))
        .err()
        .expect("a readable profile is refused");

    assert!(matches!(
        error,
        DevResetProfileError::Exposed { mode: 0o644, .. }
    ));
}

#[test]
fn an_empty_credential_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let text = PROFILE.replace("\"secret-access-key\"", "\"\"");
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("an empty secret is refused");

    assert!(matches!(
        error,
        DevResetProfileError::Empty {
            field: "s3.secret_access_key",
            ..
        }
    ));
}

/// The key prefix goes through storage setup's parse: slashes at its ends
/// mean nothing.
#[test]
fn a_key_prefix_with_slashes_at_its_ends_reads_as_its_folder_path() {
    let dir = tempfile::tempdir().unwrap();
    for typed in ["bae-dima/", "/bae-dima", "/bae-dima/"] {
        let text = PROFILE.replace("\"prefix/path\"", &format!("\"{typed}\""));
        let profile = DevResetProfile::read(&write(&dir, &text, 0o600)).unwrap();

        assert_eq!(profile.s3.key_prefix.as_str(), "bae-dima", "{typed}");
    }
}

/// A prefix that can't be a folder path fails while the profile is read,
/// naming the field and the folder at fault, before the reset removes
/// anything.
#[test]
fn a_key_prefix_that_is_no_folder_path_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    for (typed, refusal) in [
        ("prefix//path", coven::S3PrefixError::EmptySegment),
        (
            "prefix/pa.th",
            coven::S3PrefixError::InvalidCharacter {
                segment: "pa.th".into(),
                character: '.',
            },
        ),
    ] {
        let text = PROFILE.replace("\"prefix/path\"", &format!("\"{typed}\""));
        let error = DevResetProfile::read(&write(&dir, &text, 0o600))
            .err()
            .expect("a prefix that is no folder path is refused");

        let DevResetProfileError::Prefix {
            field: "s3.key_prefix",
            error: parsed,
            ..
        } = &error
        else {
            panic!("expected a refused key prefix, got {error}");
        };
        assert_eq!(*parsed, refusal, "{typed}");
        assert!(error.to_string().contains("s3.key_prefix"), "{error}");
    }
}

/// A prefix of only slashes is the bucket's root, which the reset would empty
/// whole.
#[test]
fn a_key_prefix_of_only_slashes_is_refused_as_the_whole_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let text = PROFILE.replace("\"prefix/path\"", "\"/\"");
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("the bucket's root is refused");

    assert!(
        matches!(
            error,
            DevResetProfileError::WholeBucket {
                field: "s3.key_prefix",
                ..
            }
        ),
        "{error}"
    );
}

/// toml's own message quotes the offending line; the profile's error must
/// not, since that line may hold a credential.
#[test]
fn a_malformed_profile_names_the_line_without_repeating_it() {
    let dir = tempfile::tempdir().unwrap();
    let text = PROFILE.replace(
        "secret_access_key = \"secret-access-key\"",
        "secret_access_key = secret-access-key",
    );
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("a malformed profile is refused");

    let DevResetProfileError::Malformed { line, .. } = &error else {
        panic!("expected a malformed profile, got {error}");
    };
    assert_eq!(*line, Some(12));
    assert!(!error.to_string().contains("secret-access-key"));
}

#[test]
fn an_unknown_field_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let text = format!("{PROFILE}\n[other]\nkey = \"value\"\n");
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("an unknown section is refused");

    assert!(matches!(error, DevResetProfileError::Malformed { .. }));
}
