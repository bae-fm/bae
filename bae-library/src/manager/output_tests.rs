use super::*;

#[test]
fn replace_output_dir_restores_prior_export_when_staging_rename_fails() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let final_dir = temp.path().join("album");
    std::fs::create_dir(&final_dir).expect("create prior export dir");
    std::fs::write(final_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write prior marker");
    std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

    let missing_staging = temp.path().join("missing-staging");
    replace_output_dir(&missing_staging, &final_dir, "album", "release-1")
        .expect_err("missing staging fails");

    assert_eq!(std::fs::read(final_dir.join("prior.txt")).expect("read prior export"), b"prior");
    assert!(!temp.path().join(".album.replace-release-1").exists());
}

#[test]
fn replace_output_dir_refuses_to_replace_unmarked_directory() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let final_dir = temp.path().join("album");
    std::fs::create_dir(&final_dir).expect("create existing dir");
    std::fs::write(final_dir.join("user-file.txt"), b"user data").expect("write user file");

    let staging_dir = temp.path().join("staging");
    std::fs::create_dir(&staging_dir).expect("create staging dir");
    std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write marker");
    std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

    let error = replace_output_dir(&staging_dir, &final_dir, "album", "release-1")
        .expect_err("unmarked existing target must be refused");

    assert!(error.to_string().contains("is not a prior bae output"), "unexpected error: {error}",);
    assert_eq!(
        std::fs::read(final_dir.join("user-file.txt")).expect("read user file"),
        b"user data",
    );
    assert!(staging_dir.exists());
    assert!(!temp.path().join(".album.replace-release-1").exists());
}

#[test]
fn replace_output_dir_replaces_marked_directory() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let final_dir = temp.path().join("album");
    std::fs::create_dir(&final_dir).expect("create prior export dir");
    std::fs::write(final_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write prior marker");
    std::fs::write(final_dir.join("prior.txt"), b"prior").expect("write prior export");

    let staging_dir = temp.path().join("staging");
    std::fs::create_dir(&staging_dir).expect("create staging dir");
    std::fs::write(staging_dir.join(OUTPUT_MARKER_FILE), b"release-1").expect("write marker");
    std::fs::write(staging_dir.join("export.txt"), b"export").expect("write export");

    replace_output_dir(&staging_dir, &final_dir, "album", "release-1")
        .expect("marked prior export can be replaced");

    assert_eq!(std::fs::read(final_dir.join("export.txt")).expect("read export"), b"export",);
    assert!(!final_dir.join("prior.txt").exists());
    assert!(!temp.path().join(".album.replace-release-1").exists());
}
