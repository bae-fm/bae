use super::*;
use serial_test::serial;

#[test]
fn creation_errors_preserve_their_user_facing_category_through_rollback() {
    let identity = CreateLibraryError::Identity(Box::new(coven::IdentityError::AlreadyEstablished));
    assert_eq!(identity.category(), crate::ui::UiErrorCategory::Keyring);

    let rollback = CreateLibraryError::Rollback {
        failure: Box::new(identity),
        rollback: std::io::Error::other("remove partial library"),
    };
    assert_eq!(rollback.category(), crate::ui::UiErrorCategory::Keyring);
}

#[test]
#[serial]
fn creating_a_library_establishes_its_identity_without_marking_it_active() {
    let temp = tempfile::TempDir::new().unwrap();
    crate::config::install_test_keyring();
    let config = create_library_in_bae_dir(
        temp.path(),
        crate::library_name::LibraryName::parse("Created Library").unwrap(),
        &coven::SequentialIdProvider::new("created-library"),
    )
    .expect("create library");

    assert!(!temp.path().join("active-library").exists());

    let handle = Arc::new(crate::config::ConfigHandle::new(config))
        .coven_builder()
        .synced_tables(crate::sync::synced_tables())
        .oauth_clients(crate::oauth::clients())
        .migrations(crate::migrations::all())
        .open()
        .expect("reopen created library");
    assert!(matches!(
        handle.initialize_identity(),
        Err(coven::IdentityError::AlreadyEstablished)
    ));
}

#[test]
#[serial]
fn failed_creation_removes_the_partial_library() {
    let temp = tempfile::TempDir::new().unwrap();
    crate::config::install_test_keyring();
    let library_path = crate::config::registered_library_path(temp.path(), "failed-library-0");
    let database_path = StoreDir::new(library_path.clone()).db_path();
    std::fs::create_dir_all(&database_path).unwrap();

    create_library_in_bae_dir(
        temp.path(),
        crate::library_name::LibraryName::parse("Failed Library").unwrap(),
        &coven::SequentialIdProvider::new("failed-library"),
    )
    .expect_err("a directory at the database path must fail creation");

    assert!(!library_path.exists());
}
