use super::*;

#[tokio::test]
async fn creating_a_library_establishes_its_identity_without_marking_it_active() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = coven::TestCoven::new();
    let handle = create_library(
        installation.builder(app_dir.store_layout()),
        crate::library_name::LibraryName::parse("Created Library").unwrap(),
    )
    .await
    .unwrap();
    let store = handle.store_id();
    let device = handle.device_id();
    assert_eq!(handle.store_name().await.unwrap(), "Created Library");
    assert!(!app_dir.active_library_pointer().exists());
    handle.close().await.unwrap();
    let reopened = configure_store(installation.builder(app_dir.store_layout()))
        .open(store)
        .await
        .unwrap();
    assert_eq!(reopened.device_id(), device);
    assert_eq!(reopened.store_name().await.unwrap(), "Created Library");
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn failed_creation_removes_the_partial_library() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = coven::TestCoven::new();
    installation.fail_next_keychain_operation();
    let error = create_library(
        installation.builder(app_dir.store_layout()),
        crate::library_name::LibraryName::parse("Failed Library").unwrap(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.category(), crate::ui::UiErrorCategory::KeyringLocked);
    assert!(app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(!app_dir.active_library_pointer().exists());
}
