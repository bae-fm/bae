use super::*;
use crate::{remove_unopenable_library, unsent_before_removal, UnfinishedRemoval};
use bae_app_dir::AppDir;
use coven::Storage as _;

#[path = "bootstrap_tests/settings.rs"]
mod settings;

#[tokio::test]
async fn creating_a_library_establishes_its_identity_without_marking_it_active() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let handle = create_library(
        bae_store::test_devices::test_builder(&installation),
        bae_store::EnteredName::parse("Created Library").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        "Test Device",
        bae_store::TestSources::system().clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let store = handle.store_id();
    let device = handle.device_id();
    assert_eq!(handle.store_name().await.unwrap(), "Created Library");
    assert!(!app_dir.active_library_pointer().exists());
    handle.close().await.unwrap();
    let reopened =
        configure_store(bae_store::test_devices::test_builder(&installation), crate::SCHEMA)
            .open(store, |_| ())
            .await
            .unwrap();
    assert_eq!(reopened.device_id(), device);
    assert_eq!(reopened.store_name().await.unwrap(), "Created Library");
    reopened.close().await.unwrap();
}

/// The waiting screen holds the join's checks exactly while it shows: a
/// repeated show or hide changes nothing, and showing again holds anew.
#[test]
fn the_waiting_screen_holds_the_checks_while_it_shows() {
    let control = JoinControl::new();
    assert!(!control.holds_checks_often());
    control.set_screen_shown(true);
    control.set_screen_shown(true);
    assert!(control.holds_checks_often());
    control.set_screen_shown(false);
    control.set_screen_shown(false);
    assert!(!control.holds_checks_often());
    control.set_screen_shown(true);
    assert!(control.holds_checks_often());
}

/// A join's cancel takes the join back only until its request may have been
/// sent: while it opens its database and while it connects and sends.
#[test]
fn a_join_can_be_cancelled_until_its_request_may_have_been_sent() {
    let open = coven::Progress { step: coven::OpenStep::Migrating, done: 1, total: 4 };
    let load = coven::Progress { step: coven::LoadStep::FindingKeys, done: 0, total: 1 };
    assert!(join_cancellable(&coven::BootstrapStatus::Opening(open)));
    assert!(join_cancellable(&coven::BootstrapStatus::Connecting));
    for status in [
        coven::BootstrapStatus::WaitingForApproval {
            confirmation: coven::ConfirmationNumber(41_778),
        },
        coven::BootstrapStatus::Loading(load),
        coven::BootstrapStatus::Committing,
        coven::BootstrapStatus::Pending(Vec::new()),
    ] {
        assert!(!join_cancellable(&status), "{status:?}");
    }
}

#[tokio::test]
async fn a_library_s_connections_keep_bae_s_page_cache_size() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let handle = create_library(
        bae_store::test_devices::test_builder(&installation),
        bae_store::EnteredName::parse("Cached Library").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        "Test Device",
        bae_store::TestSources::system().clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
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

/// The library keeps the name the creator typed, trimmed, and asks for their
/// profile under it.
#[tokio::test]
async fn a_created_library_keeps_its_creator_s_name_for_their_profile() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let sources = bae_store::TestSources::system();
    let handle = create_library(
        sources.builder(&installation),
        bae_store::EnteredName::parse("Created Library").unwrap(),
        bae_store::EnteredName::parse("  Robin ").unwrap(),
        "Test Device",
        sources.clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let store = bae_store::test_devices::store_under(handle.clone(), sources.clock().clone());
    let kept = store
        .read(|sql| {
            Ok(sql.query_row("SELECT name FROM creator_name", [], |row| row.get::<_, String>(0))?)
        })
        .await
        .unwrap();
    assert_eq!(kept, "Robin");
    handle.close().await.unwrap();
}

/// A creator's name that can't be kept leaves no library behind.
#[tokio::test]
async fn a_creator_s_name_that_cant_be_kept_leaves_no_library() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let sources = bae_store::TestSources::system();
    let error = create_keeping(
        sources.builder(&installation),
        bae_store::EnteredName::parse("Failed Library").unwrap(),
        "Test Device",
        sources.clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
        |_| async {
            Err(bae_store::DbError::from(coven::CovenError::Database(coven::DbError::StoreClosed)))
        },
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(error, CreateLibraryError::CreatorName(_)), "{error:?}");
    assert!(installation.stores().await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_creation_removes_the_partial_library() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    installation.fail_next_keychain_operation();
    let error = create_library(
        bae_store::test_devices::test_builder(&installation),
        bae_store::EnteredName::parse("Failed Library").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        "Test Device",
        bae_store::TestSources::system().clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::KeyringLocked);
    assert!(installation.stores().await.unwrap().is_empty());
    assert!(!app_dir.active_library_pointer().exists());
}

/// Coven refuses to reset a library that never set up storage, since there is
/// nothing in storage to load, and changes nothing: the library keeps its
/// device.
#[tokio::test]
async fn resetting_an_unconnected_store_is_refused_and_changes_nothing() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let handle = create_library(
        bae_store::test_devices::test_builder(&installation),
        bae_store::EnteredName::parse("Local Library").unwrap(),
        bae_store::EnteredName::parse("Creator").unwrap(),
        "Test Device",
        bae_store::TestSources::system().clock().clone(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let store = handle.store_id();
    let device = handle.device_id();
    handle.close().await.unwrap();
    let refused = reset_device(
        bae_store::test_devices::test_builder(&installation),
        store,
        "Replacement",
        |_| {},
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .expect_err("a library with no storage has nothing to reset from");
    assert!(
        matches!(&refused, RestoreFromCodeError::Restore(error)
            if matches!(**error, coven::BootstrapError::Sync(coven::SyncError::NoStorage))),
        "{refused:?}"
    );
    let reopened =
        configure_store(bae_store::test_devices::test_builder(&installation), crate::SCHEMA)
            .open(store, |_| ())
            .await
            .unwrap();
    assert_eq!(reopened.device_id(), device);
    reopened.close().await.unwrap();
}

/// A library with S3 storage set up on `installation`, closed, with the
/// restore code its setup returned.
async fn connected_library(
    installation: &coven::Coven,
    storage: &std::sync::Arc<coven::MemoryStorage>,
    name: &str,
) -> (coven::StoreId, coven::DeviceId, String) {
    let handle =
        configure_store(bae_store::test_devices::test_builder(installation), crate::SCHEMA)
            .storage_connector(storage.clone())
            .create_store(name, "Test Device", &[], |_, _| Ok(()))
            .await
            .unwrap();
    let setup = handle
        .setup_s3_storage(
            storage.config(),
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
    installation: &coven::Coven,
    storage: &std::sync::Arc<coven::MemoryStorage>,
) -> KeysReplacedElsewhere {
    let (store, device, stale_code) = connected_library(installation, storage, "Household").await;
    let phone_dir = tempfile::TempDir::new().unwrap();
    let phone_app_dir = AppDir::at(phone_dir.path()).unwrap();
    let (_sender, cancel) = watch::channel(false);
    let phone = restore_from_code(
        bae_store::test_devices::test_builder(
            &installation.another_device(phone_app_dir.coven_app_dir()),
        )
        .storage_connector(std::sync::Arc::new(storage.for_device())),
        &stale_code,
        Some(owner_key()),
        "Phone",
        |_| {},
        &cancel,
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let current_code = phone.replace_member_keys().await.unwrap();
    phone.close().await.unwrap();
    // This copy learns of the replacement the way the app does, from sync.
    let laptop =
        configure_store(bae_store::test_devices::test_builder(installation), crate::SCHEMA)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
            .open(store, |_| ())
            .await
            .unwrap();
    laptop.start_sync().await.unwrap();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
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
    let handle = configure_store(builder, crate::SCHEMA).open(store, |_| ()).await.unwrap();
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
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let other_storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &storage).await;
    let (_, _, other_code) = connected_library(&installation, &other_storage, "Elsewhere").await;
    let laptop = std::sync::Arc::new(storage.for_device());
    let builder =
        || bae_store::test_devices::test_builder(&installation).storage_connector(laptop.clone());
    let (_sender, cancel) = watch::channel(false);
    let store = replaced.store;
    let restore = |code: String| {
        let cancel = cancel.clone();
        async move {
            restore_replacing_device(
                builder(),
                store,
                &code,
                Some(owner_key()),
                "Laptop",
                |_| {},
                |_| {},
                &cancel,
                &bae_diagnostics::Diagnostics::noop(),
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
    assert_eq!(
        bae_diagnostics::bootstrap_category(&stale),
        bae_diagnostics::UiErrorCategory::MemberKeysReplaced
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
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &storage).await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
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
        &bae_diagnostics::Diagnostics::noop(),
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
        installation
            .stores()
            .await
            .unwrap()
            .into_iter()
            .map(|listed| listed.id())
            .collect::<Vec<_>>(),
        vec![replaced.store]
    );
    opens_as(builder(), replaced.store, device).await;
}

/// Poll `work` one step at a time and drop it at the first step after which
/// `stopped` holds, as when the app quits there.
async fn stop_once<T, Stopped: std::future::Future<Output = bool>>(
    work: impl std::future::Future<Output = T>,
    stopped: impl Fn() -> Stopped,
) {
    let mut work = std::pin::pin!(work);
    loop {
        let step = std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(work.as_mut(), cx))
        })
        .await;
        assert!(step.is_pending(), "the work finished before it was seen to stop");
        if stopped().await {
            return;
        }
        tokio::task::yield_now().await;
    }
}

/// Drop `replacing` at the first step after which coven lists its
/// replacement as unfinished: its record.
async fn stop_once_recorded<T>(
    installation: &coven::Coven,
    replacing: impl std::future::Future<Output = T>,
) {
    stop_once(replacing, || async {
        installation.stores().await.unwrap().iter().any(|listed| {
            matches!(
                listed,
                coven::ListedStore::Readable(coven::StoreInfo {
                    unfinished: Some(coven::Unfinished::Replacement(_)),
                    ..
                })
            )
        })
    })
    .await;
}

/// A restore that stops right after it recorded itself, as when the app
/// quits there, leaves the library listed as waiting for that restore, and
/// opening it says that only restoring again finishes it. Restoring again with the code finishes it, and the library
/// opens as the restored device.
#[tokio::test]
async fn a_restore_stopped_after_its_record_is_listed_and_finished_by_restoring_again() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &storage).await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let (_sender, cancel) = watch::channel(false);
    let store = replaced.store;
    let restore = || {
        let cancel = cancel.clone();
        let code = replaced.current_code.clone();
        async move {
            restore_replacing_device(
                builder(),
                store,
                &code,
                Some(owner_key()),
                "Laptop",
                |_| {},
                |_| {},
                &cancel,
                &bae_diagnostics::Diagnostics::noop(),
            )
            .await
        }
    };

    stop_once_recorded(&installation, restore()).await;

    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap();
    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].id, store.to_string());
    assert_eq!(libraries[0].name.as_deref(), Some("Household"));
    assert!(
        matches!(
            libraries[0].state,
            bae_app_dir::LibraryState::ReplacementUnfinished(coven::ReplacedBy::Restore)
        ),
        "{:?}",
        libraries[0].state
    );

    let opening = configure_store(builder(), crate::SCHEMA).open(store, |_| ()).await;
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
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap();
    assert!(
        matches!(
            libraries[..],
            [bae_app_dir::LibraryInfo { state: bae_app_dir::LibraryState::Openable(Some(_)), .. }]
        ),
        "{libraries:?}"
    );
    opens_as(builder(), store, device).await;
}

/// A restore that stopped after its record finishes from what coven kept on
/// the device, without the person's restore code, and the library opens as
/// the restored device.
#[tokio::test]
async fn a_restore_stopped_after_its_record_finishes_without_the_code() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &storage).await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let (_sender, cancel) = watch::channel(false);
    let store = replaced.store;
    stop_once_recorded(
        &installation,
        restore_replacing_device(
            builder(),
            store,
            &replaced.current_code,
            Some(owner_key()),
            "Laptop",
            |_| {},
            |_| {},
            &cancel,
            &bae_diagnostics::Diagnostics::noop(),
        ),
    )
    .await;

    let finished = super::resume_replacement(
        builder(),
        store,
        |_| {},
        &cancel,
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let device = finished.device_id();
    assert_ne!(device, replaced.device);
    finished.close().await.unwrap();
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap();
    assert!(
        matches!(
            libraries[..],
            [bae_app_dir::LibraryInfo { state: bae_app_dir::LibraryState::Openable(Some(_)), .. }]
        ),
        "{libraries:?}"
    );
    opens_as(builder(), store, device).await;
}

/// A reset that stopped after its record finishes from what coven kept on the
/// device, and the library opens as the reset device.
#[tokio::test]
async fn a_reset_stopped_after_its_record_finishes_from_what_coven_kept() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, device, _) = connected_library(&installation, &storage, "Household").await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    stop_once_recorded(
        &installation,
        reset_device(
            builder(),
            store,
            "Replacement",
            |_| {},
            &bae_diagnostics::Diagnostics::noop(),
        ),
    )
    .await;
    let libraries = app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap();
    assert!(
        matches!(
            libraries[..],
            [bae_app_dir::LibraryInfo {
                state: bae_app_dir::LibraryState::ReplacementUnfinished(coven::ReplacedBy::Reset),
                ..
            }]
        ),
        "{libraries:?}"
    );

    let (_sender, cancel) = watch::channel(false);
    let finished = super::resume_replacement(
        builder(),
        store,
        |_| {},
        &cancel,
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    let reset = finished.device_id();
    assert_ne!(reset, device);
    finished.close().await.unwrap();
    opens_as(builder(), store, reset).await;
}

/// A cancel stops finishing a restore, which coven honors, and not a
/// reset, which coven goes on with once it is under way.
#[test]
fn only_finishing_a_restore_can_be_cancelled() {
    assert!(replacement_cancellable(coven::ReplacedBy::Restore));
    assert!(!replacement_cancellable(coven::ReplacedBy::Reset));
}

/// A restore cancelled after this device registered stays unfinished, and
/// says so: not a settings fault, which the person can't fix.
#[test]
fn a_restore_cancelled_after_registering_says_it_stays_unfinished() {
    let error = restore_error(coven::BootstrapError::RestoreRegistered);
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::RestoreRegistered);
}

/// A library whose restore stopped after its record can't open, so removing it
/// can't count what it hasn't sent and goes to coven's delete of a closed
/// store. That delete takes both copies, the one the restore loaded and the
/// one it replaced: nothing of the library is left listed, its files and the
/// pointer naming it are gone, and no removal record is left.
#[tokio::test]
async fn removing_a_library_whose_restore_stopped_after_its_record_deletes_both_copies() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let replaced = keys_replaced_elsewhere(&installation, &storage).await;
    let builder = || {
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let store = replaced.store;
    app_dir.save_active_library(store).unwrap();
    let (_sender, cancel) = watch::channel(false);
    stop_once_recorded(
        &installation,
        restore_replacing_device(
            builder(),
            store,
            &replaced.current_code,
            Some(owner_key()),
            "Laptop",
            |_| {},
            |_| {},
            &cancel,
            &bae_diagnostics::Diagnostics::noop(),
        ),
    )
    .await;
    let counted = unsent_before_removal(
        builder(),
        store,
        |_| (),
        std::sync::Arc::new(coven::SystemClock),
        &bae_diagnostics::Diagnostics::noop(),
    )
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
        &installation,
        &bae_diagnostics::Diagnostics::noop(),
        store,
        &|_| (),
    )
    .await
    .unwrap();
    assert!(installation.stores().await.unwrap().is_empty());
    assert_eq!(store_files(), Vec::<String>::new());
    assert!(!app_dir.registered_library(&store.to_string()).exists());
    assert!(app_dir
        .discover_libraries(&installation, coven::task::HeardBugs::shared().as_ref())
        .await
        .unwrap()
        .is_empty());
    assert_eq!(app_dir.active_library_id().unwrap(), None);
    assert!(UnfinishedRemoval::find(
        &app_dir,
        &installation,
        &bae_diagnostics::Diagnostics::noop()
    )
    .await
    .unwrap()
    .is_none());
}

/// A restore onto a new device that stopped after it recorded itself, as
/// when the app quits there, is listed as under way with its library's id,
/// and finishes from what coven kept, without the code. Nothing is listed
/// after.
#[tokio::test]
async fn a_restore_onto_a_new_device_stopped_after_its_record_finishes_without_the_code() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, _, code) = connected_library(&installation, &storage, "Household").await;
    let phone_dir = tempfile::TempDir::new().unwrap();
    let phone = installation.another_device(AppDir::at(phone_dir.path()).unwrap().coven_app_dir());
    let builder = || {
        bae_store::test_devices::test_builder(&phone)
            .storage_connector(std::sync::Arc::new(storage.for_device()))
    };
    let (_sender, cancel) = watch::channel(false);
    stop_once(
        restore_from_code(
            builder(),
            &code,
            Some(owner_key()),
            "Phone",
            |_| {},
            &cancel,
            &bae_diagnostics::Diagnostics::noop(),
        ),
        || async {
            !unfinished_restores(&builder(), &bae_diagnostics::Diagnostics::noop())
                .await
                .unwrap()
                .is_empty()
        },
    )
    .await;

    let [restore] =
        &unfinished_restores(&builder(), &bae_diagnostics::Diagnostics::noop()).await.unwrap()[..]
    else {
        panic!("one restore under way");
    };
    assert_eq!(restore.store_id, store);
    let restored =
        resume_restore(builder, restore, |_| {}, &cancel, &bae_diagnostics::Diagnostics::noop())
            .await
            .unwrap();
    assert_eq!(restored.store_id(), store);
    assert_eq!(restored.store_name().await.unwrap(), "Household");
    restored.close().await.unwrap();
    assert!(unfinished_restores(&builder(), &bae_diagnostics::Diagnostics::noop())
        .await
        .unwrap()
        .is_empty());
}

/// A library restored onto a new device reads its history from storage under
/// the transfer limits a library starts at, which it opens under too.
#[tokio::test]
async fn a_restore_onto_a_new_device_loads_at_a_new_librarys_transfer_limits() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (_, _, code) = connected_library(&installation, &storage, "Household").await;
    let phone_dir = tempfile::TempDir::new().unwrap();
    let phone_app_dir = AppDir::at(phone_dir.path()).unwrap();
    let (_sender, cancel) = watch::channel(false);

    let phone = restore_from_code(
        bae_store::test_devices::test_builder(
            &installation.another_device(phone_app_dir.coven_app_dir()),
        )
        .storage_connector(std::sync::Arc::new(storage.for_device())),
        &code,
        Some(owner_key()),
        "Phone",
        |_| {},
        &cancel,
        &bae_diagnostics::Diagnostics::noop(),
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

/// The code a device shows, in the groups it shows it in, restores a library
/// as typed or pasted: the spaces drop before it decodes.
#[tokio::test]
async fn a_restore_code_in_the_groups_it_is_shown_in_restores_the_library() {
    let temp = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, _, code) = connected_library(&installation, &storage, "Household").await;
    let shown = crate::sync::restore_code::grouped(&code);
    assert!(shown.contains(' '));
    assert!(matches!(
        crate::sync::decode_code_info(&shown).unwrap(),
        coven::CodeInfo::Restore(metadata) if metadata.store_id == store
    ));
    let phone_dir = tempfile::TempDir::new().unwrap();
    let phone_app_dir = AppDir::at(phone_dir.path()).unwrap();
    let (_sender, cancel) = watch::channel(false);

    let phone = restore_from_code(
        bae_store::test_devices::test_builder(
            &installation.another_device(phone_app_dir.coven_app_dir()),
        )
        .storage_connector(std::sync::Arc::new(storage.for_device())),
        &shown,
        Some(owner_key()),
        "Phone",
        |_| {},
        &cancel,
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    assert_eq!(phone.store_id(), store);
    phone.close().await.unwrap();
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
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (store, device, _) = connected_library(&installation, &storage, "Household").await;
    let statuses = std::sync::Mutex::new(Vec::new());

    let reset = reset_device(
        bae_store::test_devices::test_builder(&installation)
            .storage_connector(std::sync::Arc::new(storage.for_device())),
        store,
        "Replacement",
        |status| statuses.lock().unwrap().push(status),
        &bae_diagnostics::Diagnostics::noop(),
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
