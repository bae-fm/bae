use super::*;
use crate::{remove_unopenable_library, unsent_before_removal, Keychain, UnfinishedRemoval};
use bae_app_dir::AppDir;
use coven::Storage as _;

#[tokio::test]
async fn creating_a_library_establishes_its_identity_without_marking_it_active() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let handle = create_library(
        installation.builder(app_dir.store_layout()),
        bae_db::EnteredName::parse("Created Library").unwrap(),
    )
    .await
    .unwrap();
    let store = handle.store_id();
    let device = handle.device_id();
    assert_eq!(handle.store_name().await.unwrap(), "Created Library");
    assert!(!app_dir.active_library_pointer().exists());
    handle.close().await.unwrap();
    let reopened = configure_store(installation.builder(app_dir.store_layout()))
        .open(store, |_| ())
        .await
        .unwrap();
    assert_eq!(reopened.device_id(), device);
    assert_eq!(reopened.store_name().await.unwrap(), "Created Library");
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn a_library_s_connections_keep_bae_s_page_cache_size() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let handle = create_library(
        installation.builder(app_dir.store_layout()),
        bae_db::EnteredName::parse("Cached Library").unwrap(),
    )
    .await
    .unwrap();
    #[cfg(desktop)]
    let mebibytes = 64;
    #[cfg(not(desktop))]
    let mebibytes = 16;
    assert_eq!(
        handle.sqlite_settings().await.unwrap().cache_size,
        coven::CacheSize::Kibibytes(mebibytes * 1024)
    );
    handle.close().await.unwrap();
}

#[tokio::test]
async fn failed_creation_removes_the_partial_library() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    installation.fail_next_keychain_operation();
    let error = create_library(
        installation.builder(app_dir.store_layout()),
        bae_db::EnteredName::parse("Failed Library").unwrap(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::KeyringLocked);
    assert!(app_dir.store_layout().stores().await.unwrap().is_empty());
    assert!(!app_dir.active_library_pointer().exists());
}

#[tokio::test]
async fn resetting_an_unconnected_store_replaces_the_device_and_keeps_its_name() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let handle = create_library(
        installation.builder(app_dir.store_layout()),
        bae_db::EnteredName::parse("Local Library").unwrap(),
    )
    .await
    .unwrap();
    let store = handle.store_id();
    let device = handle.device_id();
    handle.close().await.unwrap();
    let reopened =
        reset_device(installation.builder(app_dir.store_layout()), store, "Replacement", |_| {})
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
            &|_| (),
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
    let phone_app_dir = AppDir::at(phone_dir.path()).unwrap();
    let (_sender, cancel) = watch::channel(false);
    let phone = restore_from_code(
        installation
            .another_device()
            .builder(phone_app_dir.store_layout())
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
        .open(store, |_| ())
        .await
        .unwrap();
    laptop.start_sync().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let mut status = laptop.test_sync_status();
        while !matches!(
            laptop.restore_code().await,
            Err(coven::SyncError::MemberKeysReplaced { .. })
        ) {
            laptop.sync_now();
            status.next().await;
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
    let handle = configure_store(builder).open(store, |_| ()).await.unwrap();
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
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let other_storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let (_, _, other_code) =
        connected_library(&installation, &app_dir, &other_storage, "Elsewhere").await;
    let laptop = std::sync::Arc::new(storage.for_device());
    let builder = || installation.builder(app_dir.store_layout()).storage_connector(laptop.clone());
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
        matches!(*stale, coven::BootstrapError::Sync(coven::SyncError::MemberKeysReplaced { .. })),
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
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let builder = || {
        installation
            .builder(app_dir.store_layout())
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
        Some(coven::DeviceReset { old_device: replaced.device, new_device: device })
    );
    assert_eq!(restored.restore_code().await.unwrap(), replaced.current_code);
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
        assert!(step.is_pending(), "the replacement finished before its swap was seen");
        let listed = layout.stores().await.unwrap();
        if listed.iter().any(|listed| listed.unfinished_replacement.is_some()) {
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
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let builder = || {
        installation
            .builder(app_dir.store_layout())
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
                |_| {},
                &cancel,
            )
            .await
        }
    };

    stop_at_swap(&app_dir, restore()).await;

    let libraries = app_dir.discover_libraries().await.unwrap();
    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].id, store.to_string());
    assert_eq!(libraries[0].name, "Household");
    assert!(
        matches!(
            libraries[0].state,
            bae_app_dir::LibraryState::ReplacementUnfinished(coven::ReplacedBy::Restore)
        ),
        "{:?}",
        libraries[0].state
    );

    let opening = configure_store(builder()).open(store, |_| ()).await;
    assert!(
        matches!(
            opening,
            Err(coven::CovenError::Lock(coven::StoreLockError::ReplacementUnfinished {
                by: coven::ReplacedBy::Restore,
                ..
            }))
        ),
        "{:?}",
        opening.err()
    );

    let restored = restore().await.unwrap();
    let device = restored.device_id();
    assert_ne!(device, replaced.device);
    restored.close().await.unwrap();
    let libraries = app_dir.discover_libraries().await.unwrap();
    assert!(
        matches!(
            libraries[..],
            [bae_app_dir::LibraryInfo { state: bae_app_dir::LibraryState::Openable(Some(_)), .. }]
        ),
        "{libraries:?}"
    );
    opens_as(builder(), store, device).await;
}

/// A library whose restore stopped after its swap can't open, so removing it
/// can't count what it hasn't sent and goes to coven's delete of a closed
/// store. That delete takes both copies, the one the restore loaded and the
/// one it replaced: nothing of the library is left listed, its files and the
/// pointer naming it are gone, and no removal record is left.
#[tokio::test]
async fn removing_a_library_whose_restore_stopped_after_its_swap_deletes_both_copies() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = std::sync::Arc::new(coven::TestCoven::new());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let builder = || {
        installation
            .builder(app_dir.store_layout())
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let store = replaced.store;
    app_dir.save_active_library(store).unwrap();
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
            |_| {},
            &cancel,
        ),
    )
    .await;
    let counted = unsent_before_removal(builder(), store, |_| ())
        .await
        .expect_err("counting opens the library, which only finishing the restore allows");
    assert!(format!("{counted:?}").contains("ReplacementUnfinished"), "{counted:?}");

    // The restored copy waits in a hidden directory inside the store's.
    let store_files = || {
        std::fs::read_dir(temp.path().join("stores"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(&store.to_string()))
            .collect::<Vec<_>>()
    };
    assert!(store_files().contains(&store.to_string()));

    remove_unopenable_library(
        &app_dir,
        &bae_diagnostics::Diagnostics::noop(),
        &Keychain::InMemory(installation.clone()),
        store,
        &|_| (),
    )
    .await
    .unwrap();
    assert!(app_dir.store_layout().stores().await.unwrap().is_empty());
    assert_eq!(store_files(), Vec::<String>::new());
    assert!(!app_dir.registered_library(&store.to_string()).exists());
    assert!(app_dir.discover_libraries().await.unwrap().is_empty());
    assert_eq!(app_dir.active_library_id().unwrap(), None);
    assert!(UnfinishedRemoval::find(&app_dir, &bae_diagnostics::Diagnostics::noop())
        .await
        .unwrap()
        .is_none());
}

/// A library restored onto a new device reads its history from storage under
/// the transfer limits a library starts at, which it opens under too.
#[tokio::test]
async fn a_restore_onto_a_new_device_loads_at_a_new_librarys_transfer_limits() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (_, _, code) = connected_library(&installation, &app_dir, &storage, "Household").await;
    let phone_dir = tempfile::TempDir::new().unwrap();
    let phone_app_dir = AppDir::at(phone_dir.path()).unwrap();
    let (_sender, cancel) = watch::channel(false);

    let phone = restore_from_code(
        installation
            .another_device()
            .builder(phone_app_dir.store_layout())
            .storage_connector(std::sync::Arc::new(storage.for_device())),
        &code,
        Some(owner_key()),
        "Phone",
        |_| {},
        &cancel,
    )
    .await
    .unwrap();
    let default = std::num::NonZeroUsize::new(bae_app_dir::DEFAULT_CONCURRENT_TRANSFERS as usize);
    assert_eq!(
        phone.transfer_limits(),
        coven::TransferLimits { uploads: default.unwrap(), downloads: default.unwrap() }
    );
    phone.close().await.unwrap();
}

/// Restoring over this device's copy reads the library's history under the
/// transfer limits a library starts at: the copy it replaces kept its
/// preferences in its own store, and the copy put in its place starts at a
/// new library's.
#[tokio::test]
async fn a_restore_over_this_devices_copy_starts_at_a_new_librarys_preferences() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &app_dir, &storage).await;
    let (_sender, cancel) = watch::channel(false);

    let restored = restore_replacing_device(
        installation
            .builder(app_dir.store_layout())
            .storage_connector(std::sync::Arc::new(storage.for_device())),
        replaced.store,
        &replaced.current_code,
        Some(owner_key()),
        "Laptop",
        |_| {},
        |_| {},
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(restored.transfer_limits(), bae_app_dir::default_transfer_limits());
    let database = bae_db::Database::from_handle(
        restored.clone(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
    );
    assert_eq!(
        bae_preferences::preferences(&database).await.unwrap(),
        bae_preferences::Preferences::default()
    );
    restored.close().await.unwrap();
}

/// Resetting a connected library loads it from storage again, and tells its
/// caller how the load goes, as a restore does: its steps in the order they
/// run, through to the last slot applied. coven hands a caller slower than
/// the load its newest report, so a step that ends between two of the
/// caller's reads, such as reading a short history, may never reach it.
#[tokio::test]
async fn a_reset_reports_loading_the_library_in_order_to_its_end() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::TestCoven::new();
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, device, _) =
        connected_library(&installation, &app_dir, &storage, "Household").await;
    let statuses = std::sync::Mutex::new(Vec::new());

    let reset = reset_device(
        installation
            .builder(app_dir.store_layout())
            .storage_connector(std::sync::Arc::new(storage.for_device())),
        store,
        "Replacement",
        |status| statuses.lock().unwrap().push(status),
    )
    .await
    .unwrap();
    assert_ne!(reset.device_id(), device);
    reset.close().await.unwrap();
    let loading: Vec<coven::Progress<coven::LoadStep>> = statuses
        .into_inner()
        .unwrap()
        .into_iter()
        .filter_map(|status| match status {
            coven::BootstrapStatus::Loading(progress) => Some(progress),
            _ => None,
        })
        .collect();
    let order = |step: &coven::LoadStep| match step {
        coven::LoadStep::FindingKeys => 0,
        coven::LoadStep::Streaming(_) => 1,
        coven::LoadStep::Installing => 2,
        coven::LoadStep::InstallingSlots => 3,
        coven::LoadStep::Reading => 4,
        coven::LoadStep::Applying => 5,
    };
    assert!(
        loading.windows(2).all(|pair| order(&pair[0].step) <= order(&pair[1].step)),
        "{loading:?}"
    );
    // A library with no snapshot yet loads by applying its whole history,
    // and the report of the last slot applied always arrives.
    let last = loading.last().expect("the reset reports its load");
    assert_eq!(last.step, coven::LoadStep::Applying, "{loading:?}");
    assert_eq!(last.done, last.total, "{loading:?}");
}
