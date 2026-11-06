use super::*;
use std::os::unix::fs::PermissionsExt;

fn profile(dir: &tempfile::TempDir) -> String {
    let path = dir.path().join("profile.toml");
    std::fs::write(
        &path,
        r#"
library_name = "dev"
identify_automatically = false
import_when_identified = false

[s3]
endpoint = "https://storage.example.test"
region = "region-1"
bucket = "bucket-name"
access_key_id = "access-key-id"
secret_access_key = "secret-access-key"

[discogs]
token = "discogs-token"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path.to_string_lossy().into_owned()
}

/// A dev library takes its bucket whole: its storage crosses as the bucket
/// the profile names.
#[test]
fn the_profile_crosses_with_its_bucket() {
    let dir = tempfile::tempdir().unwrap();
    let read = read_dev_reset_profile(profile(&dir)).ok().unwrap();

    assert_eq!(read.sync_config.bucket, "bucket-name");
}

/// A dev reset removes every library, empties bae's directory, creates the
/// profile's library and sets it up, writing each step as it starts; the
/// first failure, here storage that refuses the connection, stops it and is
/// returned, with the new library closed.
#[cfg(desktop)]
#[test]
fn a_dev_reset_reports_each_step_until_one_fails() {
    use crate::types::{BridgeFastValue, BridgeSetupProgress, BridgeSetupStep as Step};

    let root = tempfile::tempdir().unwrap();
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let caller = tokio::runtime::Runtime::new().unwrap();
    for name in ["First", "Second"] {
        caller
            .block_on(
                host.clone().create_library(
                    Some(name.into()),
                    crate::BridgeLibrarySetup::new(host.clone()),
                ),
            )
            .unwrap();
    }
    let stray = root.path().join(".bae").join("stray-file");
    std::fs::write(&stray, "left by an earlier run").unwrap();
    let setup = crate::BridgeLibrarySetup::new(host.clone());

    let error = caller
        .block_on(host.clone().dev_reset(
            BridgeDevResetProfile {
                library_name: "Dev".into(),
                sync_config: crate::BridgeSaveSyncConfig {
                    bucket: "bucket".into(),
                    region: "us-east-1".into(),
                    endpoint: Some("http://127.0.0.1:1".into()),
                    access_key: "access-key".into(),
                    secret_key: "secret-key".into(),
                    profile: None,
                },
                discogs_token: "discogs-token".into(),
                identify_automatically: false,
                import_when_identified: true,
            },
            "Test Mac".into(),
            setup.clone(),
        ))
        .expect_err("storage that refuses the connection stops the reset");
    assert!(!matches!(error, BridgeError::Cancelled), "{error:?}");

    let step = |step, done, total| {
        Some(BridgeFastValue::SetupProgress { progress: BridgeSetupProgress { step, done, total } })
    };
    assert_eq!(
        setup.written(),
        vec![
            step(Step::FinishingRemovals, 0, 0),
            step(Step::RemovingLibraries, 0, 2),
            step(Step::RemovingLibraries, 1, 2),
            step(Step::ClearingAppData, 0, 0),
            step(Step::CreatingLibrary, 0, 0),
            step(Step::SettingUpLibrary, 0, 0),
            step(Step::ConnectingStorage, 0, 0),
            None,
        ]
    );
    assert!(!stray.exists(), "bae's directory was emptied");
    let libraries = caller.block_on(host.clone().list_libraries()).unwrap();
    assert_eq!(
        libraries.iter().map(|library| library.name.as_str()).collect::<Vec<_>>(),
        vec!["Dev"],
        "the old libraries are gone and the profile's was created"
    );
}
