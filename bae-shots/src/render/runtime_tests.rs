use super::*;

const SCRIPT: &[u8] = b"window.__dcHostRuntime = true;";

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn copy_record(sha256: &str) -> String {
    format!(
        "sha256 = \"{sha256}\"\nversion = \"v1\"\nsource = \"https://example.test/copy\"\nread_at = \"2026-11-11T00:00:00Z\"\n"
    )
}

fn live_record(sha256: &str) -> String {
    format!(
        "canvas = \"https://example.test/live\"\ncanvas_version = \"v2\"\ntype_release = \"t\"\nruntime = \"artifact-type/dc-runtime.js\"\nruntime_sha256 = \"{sha256}\"\npublished_at = \"2026-11-12T00:00:00Z\"\npublished_by = \"design\"\n"
    )
}

/// A root whose copy matches its record.
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("bae-shots/canvas")).unwrap();
    std::fs::write(copy_path(root.path()), SCRIPT).unwrap();
    write(root.path(), COPY_RECORD, &copy_record(&sha256(SCRIPT)));
    root
}

#[test]
fn sha256_is_the_hex_digest() {
    assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn equal_hashes_pass() {
    let root = root();
    write(root.path(), LIVE_RECORD, &live_record(&sha256(SCRIPT)));
    let checked = check(root.path()).unwrap();
    assert_eq!(checked.sha256, sha256(SCRIPT));
    assert_eq!(checked.version, "v1");
    assert_eq!(checked.live_published_at, "2026-11-12T00:00:00Z");
}

#[test]
fn a_changed_copy_fails_naming_both_hashes() {
    let root = root();
    write(root.path(), LIVE_RECORD, &live_record(&sha256(SCRIPT)));
    std::fs::write(copy_path(root.path()), b"edited").unwrap();
    let error = check(root.path()).unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, RuntimeError::CopyChanged { .. }));
    assert!(message.contains(&sha256(b"edited")), "{message}");
    assert!(message.contains(&sha256(SCRIPT)), "{message}");
    assert!(message.contains("To refresh the copy"), "{message}");
    assert!(message.contains("https://example.test/copy"), "{message}");
}

#[test]
fn a_changed_live_record_fails_with_both_hashes_and_when_it_was_published() {
    let root = root();
    write(root.path(), LIVE_RECORD, &live_record("f".repeat(64).as_str()));
    let error = check(root.path()).unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, RuntimeError::LiveChanged { .. }));
    assert!(message.contains(&sha256(SCRIPT)), "{message}");
    assert!(message.contains(&"f".repeat(64)), "{message}");
    assert!(message.contains("version v1"), "{message}");
    assert!(message.contains("version v2"), "{message}");
    assert!(message.contains("2026-11-12T00:00:00Z by design"), "{message}");
    assert!(message.contains("To refresh the copy"), "{message}");
    assert!(message.contains("https://example.test/live"), "{message}");
}

#[test]
fn a_missing_live_record_fails_loudly() {
    let root = root();
    let error = check(root.path()).unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, RuntimeError::LiveRecordMissing { .. }));
    assert!(message.contains("design/mocks/runtime.toml is missing"), "{message}");
    assert!(message.contains("design session"), "{message}");
}

#[test]
fn a_missing_copy_or_copy_record_is_unreadable() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(check(root.path()), Err(RuntimeError::Unreadable { .. })));
    std::fs::create_dir_all(root.path().join("bae-shots/canvas")).unwrap();
    std::fs::write(copy_path(root.path()), SCRIPT).unwrap();
    assert!(matches!(check(root.path()), Err(RuntimeError::Unreadable { .. })));
}

#[test]
fn a_record_with_an_unknown_key_is_refused() {
    let root = root();
    write(root.path(), LIVE_RECORD, &format!("{}extra = 1\n", live_record(&sha256(SCRIPT))));
    assert!(matches!(check(root.path()), Err(RuntimeError::BadRecord { .. })));
}
