use super::*;
use std::os::unix::fs::PermissionsExt;

fn profile(dir: &tempfile::TempDir, key_prefix: &str) -> String {
    let path = dir.path().join("profile.toml");
    std::fs::write(
        &path,
        format!(
            r#"
library_name = "dev"
identify_automatically = false
import_when_identified = false

[s3]
endpoint = "https://storage.example.test"
region = "region-1"
bucket = "bucket-name"
key_prefix = "{key_prefix}"
access_key_id = "access-key-id"
secret_access_key = "secret-access-key"

[discogs]
token = "discogs-token"
"#
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn the_profile_key_prefix_crosses_in_its_one_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let read = read_dev_reset_profile(profile(&dir, "bae-dima/"))
        .ok()
        .unwrap();

    assert_eq!(read.sync_config.key_prefix, "bae-dima");
}

/// A refused prefix keeps its class, so the welcome screen's line names the
/// folder at fault, and its detail names the profile's field.
#[test]
fn a_refused_profile_key_prefix_names_the_folder_and_the_field() {
    let dir = tempfile::tempdir().unwrap();
    let Err(BridgeError::Diagnostic { category, detail }) =
        read_dev_reset_profile(profile(&dir, "bae//dima"))
    else {
        panic!("a prefix with an empty folder is refused");
    };

    assert_eq!(
        category,
        crate::BridgeErrorCategory::S3Prefix {
            problem: crate::BridgeS3PrefixProblem::EmptySegment,
        }
    );
    assert!(detail.contains("s3.key_prefix"), "{detail}");
}
