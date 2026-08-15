use super::local_lifecycle::{remove_local_library_from_bae_dir, ActiveLibraryExpectation};
use serial_test::serial;
use tempfile::TempDir;

fn library_path(bae_dir: &std::path::Path, library_id: &str) -> std::path::PathBuf {
    crate::config::registered_library_path(bae_dir, library_id)
}

#[test]
#[serial]
fn remove_inactive_library_preserves_the_active_pointer() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path().join(".bae");
    let library_id = "library-being-removed";
    let active_library_id = "library-staying-active";
    let path = library_path(&bae_dir, library_id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("config.yaml"), b"unreadable config").unwrap();
    std::fs::write(bae_dir.join("active-library"), active_library_id).unwrap();
    remove_local_library_from_bae_dir(
        &bae_dir,
        library_id,
        ActiveLibraryExpectation::MayBeInactive,
    )
    .unwrap();

    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(bae_dir.join("active-library")).unwrap(),
        active_library_id
    );
}

#[test]
#[serial]
fn remove_active_library_clears_its_pointer() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path().join(".bae");
    let library_id = "active-library-id";
    let path = library_path(&bae_dir, library_id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(bae_dir.join("active-library"), library_id).unwrap();
    remove_local_library_from_bae_dir(
        &bae_dir,
        library_id,
        ActiveLibraryExpectation::MayBeInactive,
    )
    .unwrap();

    assert!(!path.exists());
    assert!(!bae_dir.join("active-library").exists());
}

#[test]
#[serial]
fn remove_local_library_rejects_a_path_instead_of_a_library_id() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path().join(".bae");
    let outside = bae_dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let error = remove_local_library_from_bae_dir(
        &bae_dir,
        "../outside",
        ActiveLibraryExpectation::MayBeInactive,
    )
    .expect_err("path-like library ids must be rejected");

    assert!(error.to_string().contains("invalid library id"));
    assert!(outside.exists());
}
