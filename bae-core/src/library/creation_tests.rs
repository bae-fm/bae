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

/// One person's library on S3, held by two of their devices: this device's
/// copy, and their phone, which then replaced their keys. This copy has synced
/// since and still holds the keys the phone replaced, so it can no longer
/// send; it is closed.
struct KeysReplacedElsewhere {
    store: coven::StoreId,
    /// This device's id in its copy.
    device: coven::DeviceId,
    /// The restore code this copy was made with, which no longer restores.
    stale_code: String,
    /// The person's current restore code, from the phone.
    current_code: String,
}

async fn keys_replaced_elsewhere(
    installation: &coven::TestCoven,
    app_dir: &AppDir,
    storage: &std::sync::Arc<coven::MemoryStorage>,
) -> KeysReplacedElsewhere {
    let (store, device, stale_code) =
        connected_library(installation, app_dir, storage, "Household").await;
    let phone_dir = tempfile::TempDir::new().unwrap();
    let (_sender, cancel) = watch::channel(false);
    let phone = restore_from_code(
        installation
            .another_device()
            .builder(AppDir::at(phone_dir.path()).store_layout())
            .storage_connector(std::sync::Arc::new(storage.for_device())),
        &stale_code,
        Some(owner_key()),
        "Phone",
        |_| {},
        &cancel,
    )
    .await
    .unwrap();
    let current_code = phone.replace_member_keys().await.unwrap();
    phone.close().await.unwrap();
    // This copy learns of the replacement the way the app does, from sync.
    let laptop = configure_store(installation.builder(app_dir.store_layout()))
        .storage_connector(std::sync::Arc::new(storage.for_device()))
        .open(store)
        .await
        .unwrap();
    laptop.start_sync().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let mut status = laptop.subscribe_sync_status();
        while !matches!(
            laptop.restore_code().await,
            Err(coven::SyncError::MemberKeysReplaced { .. })
        ) {
            laptop.sync_now();
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    laptop.close().await.unwrap();
    KeysReplacedElsewhere {
        store,
        device,
        stale_code,
        current_code: current_code.as_str().to_string(),
    }
}

/// The library opens on this device as `device`, named Household.
async fn opens_as(builder: coven::CovenBuilder, store: coven::StoreId, device: coven::DeviceId) {
    let handle = builder.open(store).await.unwrap();
    assert_eq!(handle.device_id(), device);
    assert_eq!(handle.store_name().await.unwrap(), "Household");
    handle.close().await.unwrap();
}

/// A restore over this device's copy that fails, whether the code is another
/// library's, is the one the replaced keys made, or storage can't be reached,
/// leaves the copy as it was: it opens under its old device id, and restoring
/// again with the current code then succeeds.
#[tokio::test]
async fn a_failed_restore_over_this_devices_copy_leaves_it_to_open_or_retry() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let other_storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let (_, _, other_code) =
        connected_library(&installation, &app_dir, &other_storage, "Elsewhere").await;
    let laptop = std::sync::Arc::new(storage.for_device());
    let builder = || {
        configure_store(installation.builder(app_dir.store_layout()))
            .storage_connector(laptop.clone())
    };
    let (_sender, cancel) = watch::channel(false);
    let store = replaced.store;
    let restore = |code: String| {
        let builder = builder();
        let cancel = cancel.clone();
        async move {
            restore_replacing_device(
                builder,
                store,
                &code,
                Some(owner_key()),
                "Laptop",
                |_| {},
                &cancel,
            )
            .await
        }
    };

    let Err(RestoreFromCodeError::Restore(refused)) = restore(other_code).await else {
        panic!("another library's code restores nothing here");
    };
    assert!(
        matches!(
            *refused,
            coven::BootstrapError::Code(coven::CodeError::WrongStore { expected, .. })
                if expected == replaced.store
        ),
        "{refused:?}"
    );
    opens_as(builder(), replaced.store, replaced.device).await;

    let Err(RestoreFromCodeError::Restore(stale)) = restore(replaced.stale_code.clone()).await
    else {
        panic!("the code the replaced keys made restores nothing");
    };
    assert!(
        matches!(
            *stale,
            coven::BootstrapError::Sync(coven::SyncError::MemberKeysReplaced { .. })
        ),
        "{stale:?}"
    );
    opens_as(builder(), replaced.store, replaced.device).await;

    laptop.set_online(false);
    assert!(restore(replaced.current_code.clone()).await.is_err());
    laptop.set_online(true);
    opens_as(builder(), replaced.store, replaced.device).await;

    let restored = restore(replaced.current_code.clone()).await.unwrap();
    assert_ne!(restored.device_id(), replaced.device);
    restored.close().await.unwrap();
}

/// The person's current code restores the library in place of this device's
/// copy: a new device id holds it, the copy that couldn't send is gone, and
/// the library opens as the restored one from then on.
#[tokio::test]
async fn restoring_with_the_current_code_leaves_the_restored_copy() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let builder = || {
        configure_store(installation.builder(app_dir.store_layout()))
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let (_sender, cancel) = watch::channel(false);

    let restored = restore_replacing_device(
        builder(),
        replaced.store,
        &replaced.current_code,
        Some(owner_key()),
        "Laptop",
        |_| {},
        &cancel,
    )
    .await
    .unwrap();
    let device = restored.device_id();
    assert_ne!(device, replaced.device);
    assert_eq!(restored.store_name().await.unwrap(), "Household");
    assert_eq!(
        restored.device_reset().await.unwrap(),
        Some(coven::DeviceReset {
            old_device: replaced.device,
            new_device: device,
        })
    );
    assert_eq!(
        restored.restore_code().await.unwrap(),
        replaced.current_code
    );
    restored.close().await.unwrap();
    assert_eq!(
        app_dir
            .store_layout()
            .stores()
            .await
            .unwrap()
            .into_iter()
            .map(|listed| listed.id)
            .collect::<Vec<_>>(),
        vec![replaced.store]
    );
    opens_as(builder(), replaced.store, device).await;
}

/// Poll `replacing` one step at a time and drop it at the first step after
/// which coven lists its replacement as unfinished: the swap, as when the app
/// quits there.
async fn stop_at_swap<T>(app_dir: &AppDir, replacing: impl std::future::Future<Output = T>) {
    let layout = app_dir.store_layout();
    let mut replacing = std::pin::pin!(replacing);
    loop {
        let step = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(replacing.as_mut(), cx))
        })
        .await;
        assert!(
            step.is_pending(),
            "the replacement finished before its swap was seen"
        );
        let listed = layout.stores().await.unwrap();
        if listed
            .iter()
            .any(|listed| listed.unfinished_replacement.is_some())
        {
            return;
        }
        tokio::task::yield_now().await;
    }
}

/// A restore that stops right after the restored copy took the place of this
/// device's copy, as when the app quits there, leaves the library listed as
/// waiting for that restore, and opening it says that only restoring again
/// finishes it. Restoring again with the code finishes it, and the library
/// opens as the restored device.
#[tokio::test]
async fn a_restore_stopped_after_its_swap_is_listed_and_finished_by_restoring_again() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let builder = || {
        configure_store(installation.builder(app_dir.store_layout()))
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let (_sender, cancel) = watch::channel(false);
    let store = replaced.store;
    let restore = || {
        let builder = builder();
        let cancel = cancel.clone();
        let code = replaced.current_code.clone();
        async move {
            restore_replacing_device(
                builder,
                store,
                &code,
                Some(owner_key()),
                "Laptop",
                |_| {},
                &cancel,
            )
            .await
        }
    };

    stop_at_swap(&app_dir, restore()).await;

    let libraries = crate::config::Config::discover_libraries(&app_dir)
        .await
        .unwrap();
    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].id, store.to_string());
    assert_eq!(libraries[0].name, "Household");
    assert!(
        matches!(
            libraries[0].state,
            crate::config::LibraryState::ReplacementUnfinished(coven::ReplacedBy::Restore)
        ),
        "{:?}",
        libraries[0].state
    );

    let opening = {
        let app_dir = app_dir.clone();
        let builder = builder();
        tokio::task::spawn_blocking(move || {
            crate::app::bootstrap(
                app_dir,
                store.to_string(),
                200,
                false,
                crate::diagnostics::Diagnostics::noop(),
                builder,
                |_, _| -> Result<(), crate::app::BootstrapError> {
                    panic!("a library whose restore is unfinished doesn't open")
                },
            )
        })
        .await
        .unwrap()
    };
    assert!(
        matches!(
            opening,
            Err(crate::app::BootstrapError::ReplacementUnfinished(
                coven::ReplacedBy::Restore
            ))
        ),
        "{opening:?}"
    );

    let restored = restore().await.unwrap();
    let device = restored.device_id();
    assert_ne!(device, replaced.device);
    restored.close().await.unwrap();
    let libraries = crate::config::Config::discover_libraries(&app_dir)
        .await
        .unwrap();
    assert!(
        matches!(
            libraries[..],
            [crate::config::LibraryInfo {
                state: crate::config::LibraryState::Openable(Some(_)),
                ..
            }]
        ),
        "{libraries:?}"
    );
    opens_as(builder(), store, device).await;
}

/// A library whose restore stopped after its swap can't open, so removing it
/// can't count what it hasn't sent and goes to coven's delete of a closed
/// store. coven refuses that delete while the replacement is unfinished, so
/// the removal changes nothing: the library stays listed as waiting for its
/// restore, the pointer still names it, and no removal record is left.
#[tokio::test]
async fn removing_a_library_whose_restore_stopped_after_its_swap_is_refused_unchanged() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path());
    let installation = std::sync::Arc::new(coven::TestCoven::new());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let builder = || {
        configure_store(installation.builder(app_dir.store_layout()))
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let store = replaced.store;
    crate::config::Config::with_defaults(store, app_dir.registered_library(&store.to_string()))
        .save_active_library(&app_dir)
        .unwrap();
    let (_sender, cancel) = watch::channel(false);
    stop_at_swap(
        &app_dir,
        restore_replacing_device(
            builder(),
            store,
            &replaced.current_code,
            Some(owner_key()),
            "Laptop",
            |_| {},
            &cancel,
        ),
    )
    .await;
    let counted = unsent_before_removal(builder(), store)
        .await
        .expect_err("counting opens the library, which only finishing the restore allows");
    assert!(
        format!("{counted:?}").contains("ReplacementUnfinished"),
        "{counted:?}"
    );

    let removal =
        remove_unopenable_library(&app_dir, &Keychain::InMemory(installation.clone()), store)
            .await
            .expect_err("coven refuses deleting a store whose replacement is unfinished");
    assert!(
        matches!(
            removal,
            LibraryRemovalError::Store(coven::StoreDeletionError::Lock(
                coven::StoreLockError::ReplacementUnfinished {
                    by: coven::ReplacedBy::Restore,
                    ..
                }
            ))
        ),
        "{removal:?}"
    );
    let libraries = crate::config::Config::discover_libraries(&app_dir)
        .await
        .unwrap();
    assert!(
        matches!(
            libraries[..],
            [crate::config::LibraryInfo {
                state: crate::config::LibraryState::ReplacementUnfinished(
                    coven::ReplacedBy::Restore
                ),
                ..
            }]
        ),
        "{libraries:?}"
    );
    assert_eq!(
        crate::config::Config::active_library_id(&app_dir).unwrap(),
        Some(store.to_string())
    );
    assert!(UnfinishedRemoval::find(&app_dir).await.unwrap().is_none());
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
