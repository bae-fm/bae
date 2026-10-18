use super::*;
use coven::Storage as _;

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

#[tokio::test]
async fn resetting_an_unconnected_store_replaces_the_device_and_keeps_its_name() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = coven::TestCoven::new();
    let handle = create_library(
        installation.builder(app_dir.store_layout()),
        crate::library_name::LibraryName::parse("Local Library").unwrap(),
    )
    .await
    .unwrap();
    let store = handle.store_id();
    let device = handle.device_id();
    handle.close().await.unwrap();
    let reopened = reset_device(
        installation.builder(app_dir.store_layout()),
        store,
        "Replacement",
    )
    .await
    .unwrap();
    assert_ne!(reopened.device_id(), device);
    assert_eq!(reopened.store_name().await.unwrap(), "Local Library");
    reopened.close().await.unwrap();
}

/// A library with S3 storage set up on `installation`, closed, with the
/// restore code its setup returned.
async fn connected_library(
    installation: &coven::TestCoven,
    app_dir: &AppDir,
    storage: &std::sync::Arc<coven::MemoryStorage>,
    name: &str,
) -> (coven::StoreId, coven::DeviceId, String) {
    let handle = configure_store(installation.builder(app_dir.store_layout()))
        .storage_connector(storage.clone())
        .create_store(name)
        .await
        .unwrap();
    let setup = handle
        .setup_s3_storage(
            storage.config(),
            "Device",
            "owner-key".into(),
            coven::SecretText::new("owner-secret".into()),
        )
        .await
        .unwrap();
    let ids = (handle.store_id(), handle.device_id());
    handle.close().await.unwrap();
    (ids.0, ids.1, setup.restore_code.as_str().to_string())
}

fn owner_key() -> coven::S3Credentials {
    coven::S3Credentials {
        access_key_id: "owner-key".into(),
        secret_access_key: coven::SecretText::new("owner-secret".into()),
    }
}

/// Restoring over this device's copy takes the person's restore code for that
/// library: another library's code changes nothing, and the library's own
/// code discards the copy and registers this device again under a new id.
#[tokio::test]
async fn restoring_over_this_devices_copy_needs_that_librarys_code() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = std::sync::Arc::new(coven::TestCoven::new());
    let keychain = Keychain::InMemory(installation.clone());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let other_storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, device, code) =
        connected_library(&installation, &app_dir, &storage, "Household").await;
    let (_, _, other_code) =
        connected_library(&installation, &app_dir, &other_storage, "Elsewhere").await;
    let (_sender, cancel) = watch::channel(false);
    let builder = || {
        configure_store(installation.builder(app_dir.store_layout()))
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };

    let Err(refused) = restore_replacing_device(
        &keychain,
        &app_dir,
        builder(),
        store,
        &other_code,
        Some(owner_key()),
        "Device",
        |_| {},
        &cancel,
    )
    .await
    else {
        panic!("another library's code restores nothing here");
    };
    assert!(matches!(refused, RestoreFromCodeError::OtherLibrary { .. }));
    assert_eq!(refused.category(), crate::ui::UiErrorCategory::Config);
    assert!(app_dir
        .store_layout()
        .stores()
        .await
        .unwrap()
        .iter()
        .any(|listed| listed.id == store));

    let restored = restore_replacing_device(
        &keychain,
        &app_dir,
        builder(),
        store,
        &code,
        Some(owner_key()),
        "Device",
        |_| {},
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(restored.store_id(), store);
    assert_ne!(restored.device_id(), device);
    assert_eq!(restored.store_name().await.unwrap(), "Household");
    restored.close().await.unwrap();
}

#[tokio::test]
async fn saved_codes_for_libraries_on_this_device_are_not_offered() {
    let temp = tempfile::TempDir::new().unwrap();
    let layout = AppDir::at(temp.path()).store_layout();
    let here = coven::TestCoven::new()
        .create_closed_store(
            &layout,
            "On This Device",
            std::sync::Arc::new(coven::UuidIds),
        )
        .await
        .unwrap()
        .id();
    let elsewhere = coven::StoreId(uuid::Uuid::new_v4());
    let code = |store_id| coven::SavedRestoreCode {
        store_id,
        location: coven::StorageConfig::CloudKit {
            container: "iCloud.fm.bae.desktop".into(),
            owner: "owner".into(),
            zone: "zone".into(),
        },
        has_s3_credentials: false,
    };
    let offered = restorable_codes(vec![code(here), code(elsewhere)], &layout)
        .await
        .unwrap();
    assert_eq!(offered, vec![code(elsewhere)]);
}
