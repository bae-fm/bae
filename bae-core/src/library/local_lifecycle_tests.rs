use super::local_lifecycle::remove_local_library;
use crate::config::AppDir;
use tempfile::TempDir;

#[test]
fn remove_inactive_library_preserves_the_active_pointer() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(tmp.path());
    crate::config::install_test_keyring();
    let library_id = "library-being-removed";
    let active_library_id = "library-staying-active";
    let path = app_dir.registered_library(library_id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("config.yaml"), b"unreadable config").unwrap();
    std::fs::write(app_dir.active_library_pointer(), active_library_id).unwrap();
    remove_local_library(&app_dir, library_id).unwrap();

    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(app_dir.active_library_pointer()).unwrap(),
        active_library_id
    );
}

#[test]
fn remove_active_library_clears_its_pointer() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(tmp.path());
    crate::config::install_test_keyring();
    let library_id = "active-library-id";
    let path = app_dir.registered_library(library_id);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(app_dir.active_library_pointer(), library_id).unwrap();
    remove_local_library(&app_dir, library_id).unwrap();

    assert!(!path.exists());
    assert!(!app_dir.active_library_pointer().exists());
}

#[test]
fn remove_local_library_rejects_a_path_instead_of_a_library_id() {
    let tmp = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(tmp.path());
    let outside = tmp.path().join(".bae").join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let error = remove_local_library(&app_dir, "../outside")
        .expect_err("path-like library ids must be rejected");

    assert!(error.to_string().contains("invalid library id"));
    assert!(outside.exists());
}
