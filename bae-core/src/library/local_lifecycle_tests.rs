use super::local_lifecycle::remove_local_library;
use crate::config::{AppDir, Config};
use coven::Storage;
use std::sync::Arc;
use tempfile::TempDir;

async fn library() -> (
    TempDir,
    AppDir,
    coven::CovenHandle,
    Arc<coven::MemoryStorage>,
) {
    let temp = TempDir::new().unwrap();
    let app_dir = AppDir::under_home(temp.path());
    let installation = coven::TestCoven::new();
    let storage = Arc::new(
        coven::MemoryStorage::builder()
            .clock(Arc::new(coven::SystemClock))
            .build()
            .unwrap(),
    );
    let handle = super::configure_store(installation.builder(app_dir.store_layout()))
        .storage_connector(storage.clone())
        .create_store("Library")
        .await
        .unwrap();
    let config = Config::with_defaults(
        handle.store_id(),
        app_dir.registered_library(&handle.store_id().to_string()),
    );
    config.save_preferences().unwrap();
    (temp, app_dir, handle, storage)
}

#[tokio::test]
async fn removing_an_inactive_library_preserves_the_active_pointer() {
    let (_temp, app_dir, handle, _storage) = library().await;
    let active = uuid::Uuid::new_v4().to_string();
    std::fs::write(app_dir.active_library_pointer(), &active).unwrap();
    let id = handle.store_id();
    remove_local_library(&app_dir, &handle, coven::IfUnsent::Refuse)
        .await
        .unwrap();
    assert!(app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(!app_dir.registered_library(&id.to_string()).exists());
    assert_eq!(
        std::fs::read_to_string(app_dir.active_library_pointer()).unwrap(),
        active
    );
}

#[tokio::test]
async fn removing_the_active_library_clears_its_pointer() {
    let (_temp, app_dir, handle, _storage) = library().await;
    let id = handle.store_id();
    std::fs::write(app_dir.active_library_pointer(), id.to_string()).unwrap();
    remove_local_library(&app_dir, &handle, coven::IfUnsent::Refuse)
        .await
        .unwrap();
    assert!(app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(!app_dir.registered_library(&id.to_string()).exists());
    assert!(!app_dir.active_library_pointer().exists());
}

#[tokio::test]
async fn offline_removal_keeps_the_store_preferences_and_active_pointer() {
    let (_temp, app_dir, handle, storage) = library().await;
    handle
        .setup_s3_storage(
            storage.config(),
            "Device",
            "key".into(),
            coven::SecretText::new("secret".into()),
        )
        .await
        .unwrap();
    let mut status = handle.subscribe_sync_status();
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        status.wait_for(|s| matches!(s, coven::SyncStatus::Synced { .. })),
    )
    .await
    .unwrap()
    .unwrap();
    let id = handle.store_id();
    std::fs::write(app_dir.active_library_pointer(), id.to_string()).unwrap();
    storage.set_online(false);
    let error = remove_local_library(&app_dir, &handle, coven::IfUnsent::Discard)
        .await
        .expect_err("leaving needs reachable storage");
    assert_eq!(error.category(), crate::ui::UiErrorCategory::Network);
    assert_eq!(handle.store_name().await.unwrap(), "Library");
    assert!(app_dir
        .registered_library(&id.to_string())
        .join(crate::config::PREFERENCES_FILENAME)
        .exists());
    assert_eq!(
        std::fs::read_to_string(app_dir.active_library_pointer()).unwrap(),
        id.to_string()
    );
    storage.set_online(true);
    remove_local_library(&app_dir, &handle, coven::IfUnsent::Discard)
        .await
        .unwrap();
    assert!(!app_dir.active_library_pointer().exists());
}

#[tokio::test]
async fn preferences_failure_reports_that_the_store_was_already_removed() {
    let (_temp, app_dir, handle, _storage) = library().await;
    std::fs::create_dir(app_dir.active_library_pointer()).unwrap();
    let error = remove_local_library(&app_dir, &handle, coven::IfUnsent::Refuse)
        .await
        .unwrap_err();
    assert!(app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(error.to_string().contains("store was removed"), "{error}");
    let super::LibraryRemovalError::Preferences { library, .. } = error else {
        panic!("only preferences remain to be removed");
    };
    std::fs::remove_dir(app_dir.active_library_pointer()).unwrap();
    std::fs::write(
        app_dir.active_library_pointer(),
        handle.store_id().to_string(),
    )
    .unwrap();
    library.finish(&app_dir).await.unwrap();
    assert!(!app_dir.active_library_pointer().exists());
}
