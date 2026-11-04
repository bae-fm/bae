use super::{
    finish_removals, remove_local_library, remove_unopenable_library, LibraryRemovalError,
    UnfinishedRemoval,
};
use bae_app_dir::AppDir;
use bae_diagnostics::Diagnostics;
use coven::Storage;
use std::sync::Arc;
use tempfile::TempDir;

struct Home {
    _temp: TempDir,
    app_dir: AppDir,
    installation: coven::Coven,
    storage: Arc<coven::MemoryStorage>,
}

impl Home {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let app_dir = AppDir::under_home(temp.path()).unwrap();
        Self {
            _temp: temp,
            installation: coven::Coven::in_memory(app_dir.coven_app_dir()),
            app_dir,
            storage: Arc::new(
                coven::MemoryStorage::builder()
                    .clock(Arc::new(coven::SystemClock))
                    .build()
                    .unwrap(),
            ),
        }
    }

    /// A library with bae's own directory for it, as one this device has
    /// opened.
    async fn library(&self, name: &str) -> coven::CovenHandle {
        let handle = bae_db::configure_store(self.installation.builder())
            .storage_connector(self.storage.clone())
            .create_store(name)
            .await
            .unwrap();
        std::fs::create_dir_all(self.directory(handle.store_id()).join("cache")).unwrap();
        handle
    }

    /// Give `handle`'s library its cloud home, which saves the person's
    /// restore code, and wait for it to sync.
    async fn connect(&self, handle: &coven::CovenHandle) {
        handle
            .setup_s3_storage(
                self.storage.config(),
                "Device",
                "key".into(),
                coven::SecretText::new("secret".into()),
                &|_| (),
            )
            .await
            .unwrap();
        handle.test_sync_status().wait_for(|s| s.state == coven::SyncState::Synced).await;
    }

    /// The libraries whose saved restore codes this device offers, with the
    /// name each is offered under.
    async fn offered_codes(&self) -> Vec<(coven::StoreId, String)> {
        crate::saved_restore_codes(&self.installation)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| match entry {
                coven::SavedRestoreEntry::Readable(code) => (code.store_id, code.name),
                coven::SavedRestoreEntry::Unreadable { store_id, reason } => {
                    panic!("{store_id}'s saved code is unreadable: {reason:?}")
                }
            })
            .collect()
    }

    fn directory(&self, id: coven::StoreId) -> std::path::PathBuf {
        self.app_dir.registered_library(&id.to_string())
    }

    fn point_at(&self, id: coven::StoreId) {
        std::fs::write(self.app_dir.active_library_pointer(), id.to_string()).unwrap();
    }

    fn pointer(&self) -> Option<String> {
        self.app_dir.active_library_id().unwrap()
    }

    /// What the person is shown when the app next lists its libraries, and
    /// whether it has files to finish deleting.
    async fn unfinished(&self) -> Option<(String, bool)> {
        UnfinishedRemoval::find(&self.app_dir, &self.installation, &Diagnostics::noop())
            .await
            .unwrap()
            .map(|removal| (removal.to_string(), removal.files_remain()))
    }

    fn record(&self, id: coven::StoreId) -> std::path::PathBuf {
        self.app_dir.removal_records().join(id.to_string())
    }

    /// Run a removal and stop it, as the app quitting would, once `delete`
    /// (standing in for coven's delete) has run: everything before it is done,
    /// nothing after it.
    async fn stop_removal_after(
        &self,
        id: coven::StoreId,
        delete: impl std::future::Future<Output = Result<(), coven::StoreDeletionError>>,
    ) {
        let reached = tokio::sync::Notify::new();
        let diagnostics = Diagnostics::noop();
        let removal = super::remove_recorded(
            &self.app_dir,
            &diagnostics,
            id,
            async {
                delete.await?;
                reached.notify_one();
                std::future::pending().await
            },
            &|_| (),
        );
        tokio::select! {
            result = removal => panic!("the removal was to stop partway: {result:?}"),
            () = reached.notified() => {}
        }
    }

    async fn stores(&self) -> usize {
        self.installation.stores().await.unwrap().len()
    }
}

#[cfg(unix)]
fn set_writable(path: &std::path::Path, writable: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if writable { 0o755 } else { 0o555 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[tokio::test]
async fn removing_an_inactive_library_preserves_the_active_pointer() {
    let home = Home::new();
    let active = home.library("Active").await;
    let removed = home.library("Removed").await;
    home.point_at(active.store_id());
    let id = removed.store_id();
    remove_local_library(&home.app_dir, &Diagnostics::noop(), &removed).await.unwrap();
    assert_eq!(home.stores().await, 1);
    assert!(!home.directory(id).exists());
    assert_eq!(home.pointer(), Some(active.store_id().to_string()));
    assert!(home.directory(active.store_id()).exists());
    assert_eq!(home.unfinished().await, None);
}

#[tokio::test]
async fn removing_the_active_library_clears_its_pointer() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap();
    assert_eq!(home.stores().await, 0);
    assert!(!home.directory(id).exists());
    assert_eq!(home.pointer(), None);
    assert_eq!(home.unfinished().await, None);
}

/// Closing the store fails once this device has left the library: the
/// removal fails saying so, with the library's files still on this device,
/// its pointer kept and no removal recorded.
#[tokio::test]
async fn a_close_failing_after_leaving_keeps_the_files_and_says_so() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    let error = super::remove_recorded(
        &home.app_dir,
        &Diagnostics::noop(),
        id,
        async { Err(coven::StoreDeletionError::Closing(coven::SyncError::NoStorage)) },
        &|_| (),
    )
    .await
    .expect_err("the close failed");
    assert!(matches!(error, LibraryRemovalError::Closing(_)), "{error}");
    assert!(error.to_string().contains("files are still on this device"), "{error}");
    assert_eq!(home.stores().await, 1);
    assert!(home.directory(id).join("cache").exists());
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(!home.record(id).exists());
}

/// A removal coven refuses changes nothing of bae's; the same request
/// finishes once storage is back.
#[tokio::test]
async fn offline_removal_keeps_the_store_preferences_and_active_pointer() {
    let home = Home::new();
    let handle = home.library("Library").await;
    home.connect(&handle).await;
    let id = handle.store_id();
    home.point_at(id);
    home.storage.set_online(false);
    let error = remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle)
        .await
        .expect_err("leaving needs reachable storage");
    assert!(matches!(error, LibraryRemovalError::Store(_)), "{error}");
    assert_eq!(
        error.category(),
        bae_diagnostics::UiErrorCategory::Storage(coven::StorageFailure::Network)
    );
    assert_eq!(handle.store_name().await.unwrap(), "Library");
    assert!(home.directory(id).join("cache").exists());
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
    home.storage.set_online(true);
    remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap();
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert_eq!(home.unfinished().await, None);
}

/// The restore code saved for a library is the person's, not this device's:
/// removing the library from this device keeps it, and once the library is
/// gone from here this device offers restoring it again.
#[tokio::test]
async fn removing_a_library_keeps_its_saved_restore_code_to_offer_again() {
    let home = Home::new();
    let handle = home.library("Library").await;
    home.connect(&handle).await;
    let id = handle.store_id();
    assert_eq!(home.offered_codes().await, Vec::new());
    remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap();
    assert_eq!(home.offered_codes().await, vec![(id, "Library".to_string())]);
}

/// Forgetting a saved restore code deletes it from the person's keychain, so
/// no device offers it, and a second forget finds nothing to delete and
/// succeeds.
#[tokio::test]
async fn a_forgotten_restore_code_is_no_longer_offered() {
    let home = Home::new();
    let kept = home.library("Kept").await;
    home.connect(&kept).await;
    let forgotten = home.library("Forgotten").await;
    home.connect(&forgotten).await;
    let (kept_id, forgotten_id) = (kept.store_id(), forgotten.store_id());
    remove_local_library(&home.app_dir, &Diagnostics::noop(), &kept).await.unwrap();
    remove_local_library(&home.app_dir, &Diagnostics::noop(), &forgotten).await.unwrap();
    let mut offered = home.offered_codes().await;
    offered.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        offered,
        vec![(forgotten_id, "Forgotten".to_string()), (kept_id, "Kept".to_string()),]
    );

    home.installation.forget_saved_restore_code(forgotten_id).await.unwrap();
    assert_eq!(home.offered_codes().await, vec![(kept_id, "Kept".to_string())]);
    home.installation.forget_saved_restore_code(forgotten_id).await.unwrap();
    assert_eq!(home.offered_codes().await, vec![(kept_id, "Kept".to_string())]);
}

#[tokio::test]
async fn a_removal_that_cannot_be_recorded_removes_nothing() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    std::fs::write(home.app_dir.removal_records(), b"").unwrap();
    let error =
        remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap_err();
    assert!(matches!(error, LibraryRemovalError::Record(_)), "{error}");
    assert_eq!(handle.store_name().await.unwrap(), "Library");
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(home.directory(id).exists());
}

/// The app stops after recording the removal, before coven deletes the store:
/// the library is still here, the person is told the removal didn't finish,
/// and the record is cleared so it's reported once.
#[tokio::test]
async fn a_removal_stopped_before_the_delete_reports_the_library_still_here() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    home.stop_removal_after(id, async { Ok(()) }).await;
    assert!(home.record(id).exists());

    let (shown, files_remain) = home.unfinished().await.unwrap();
    assert!(shown.contains("still on this device"), "{shown}");
    assert!(shown.contains(&id.to_string()), "{shown}");
    assert!(!files_remain);
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
    finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|_, _| ())
        .await
        .unwrap();
    assert_eq!(home.stores().await, 1);
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(home.directory(id).exists());

    remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap();
    assert_eq!(home.stores().await, 0);
    assert!(!home.directory(id).exists());
}

/// The app stops right after coven deletes the store: the record finds the
/// removal again, and finishing it deletes bae's files and then the record.
#[tokio::test]
async fn a_removal_stopped_after_the_store_was_deleted_is_found_again() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    home.stop_removal_after(id, handle.delete_store(coven::IfUnsent::Refuse)).await;
    drop(handle);

    let (shown, files_remain) = home.unfinished().await.unwrap();
    assert!(shown.contains(&id.to_string()), "{shown}");
    assert!(files_remain);
    assert!(home.record(id).exists());
    let finished = std::sync::Mutex::new(Vec::new());
    finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|done, total| {
        finished.lock().unwrap().push((done, total))
    })
    .await
    .unwrap();
    assert_eq!(
        finished.into_inner().unwrap(),
        [(0, 1), (1, 1)],
        "finishing counts the one removal it finishes"
    );
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
    finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|_, _| ())
        .await
        .unwrap();
}

/// The pointer can't be read: the store is gone, the record keeps the removal
/// listed, and a retry finishes it once the cause is fixed.
#[tokio::test]
async fn a_pointer_that_cannot_be_cleared_leaves_the_removal_to_retry() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    std::fs::create_dir(home.app_dir.active_library_pointer()).unwrap();
    let error =
        remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap_err();
    assert!(matches!(error, LibraryRemovalError::Unfinished(_)), "{error}");
    assert!(error.to_string().contains("store was removed"), "{error}");
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::Internal);
    assert_eq!(home.stores().await, 0);
    assert!(home.directory(id).exists());
    assert!(home.record(id).exists());
    assert!(home.unfinished().await.unwrap().1);

    assert!(finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|_, _| ())
        .await
        .is_err());
    std::fs::remove_dir(home.app_dir.active_library_pointer()).unwrap();
    home.point_at(id);
    finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|_, _| ())
        .await
        .unwrap();
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
}

/// The directory can't be deleted: the pointer is already cleared, the record
/// keeps the removal listed, and a retry deletes the directory and record.
#[cfg(unix)]
#[tokio::test]
async fn a_directory_that_cannot_be_deleted_leaves_the_removal_to_retry() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    set_writable(&home.app_dir.libraries(), false);
    let error =
        remove_local_library(&home.app_dir, &Diagnostics::noop(), &handle).await.unwrap_err();
    assert!(matches!(error, LibraryRemovalError::Unfinished(_)), "{error}");
    assert!(error.to_string().contains(&home.directory(id).display().to_string()), "{error}");
    assert_eq!(home.stores().await, 0);
    assert_eq!(home.pointer(), None);
    assert!(home.directory(id).exists());
    assert!(home.unfinished().await.unwrap().1);

    set_writable(&home.app_dir.libraries(), true);
    finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|_, _| ())
        .await
        .unwrap();
    assert!(!home.directory(id).exists());
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
}

/// bae files for a store coven doesn't list, with no removal record (a store
/// hidden from the list, or files left some other way), are never reported
/// or deleted by removal code.
#[tokio::test]
async fn files_without_a_removal_record_are_left_alone() {
    let home = Home::new();
    let kept = home.library("Kept").await;
    let hidden = coven::StoreId(uuid::Uuid::new_v4());
    std::fs::create_dir_all(home.directory(hidden).join("cache")).unwrap();
    home.point_at(hidden);
    let removed = home.library("Removed").await;
    home.stop_removal_after(removed.store_id(), removed.delete_store(coven::IfUnsent::Refuse))
        .await;

    let (shown, _) = home.unfinished().await.unwrap();
    assert!(shown.contains(&removed.store_id().to_string()), "{shown}");
    assert!(!shown.contains(&hidden.to_string()), "{shown}");
    finish_removals(&home.app_dir, &home.installation, &Diagnostics::noop(), &|_, _| ())
        .await
        .unwrap();
    assert!(!home.directory(removed.store_id()).exists());
    assert!(home.directory(hidden).exists());
    assert_eq!(home.pointer(), Some(hidden.to_string()));
    assert!(home.directory(kept.store_id()).exists());
    assert_eq!(home.unfinished().await, None);
}

/// A lookup started while a removal waits on coven's delete waits for the
/// removal and sees it finished, rather than reading its record mid-way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lookup_during_a_removal_waits_for_it() {
    let home = Arc::new(Home::new());
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    let deleting = Arc::new(tokio::sync::Notify::new());
    let delete_may_finish = Arc::new(tokio::sync::Notify::new());

    let removal = tokio::spawn({
        let home = home.clone();
        let deleting = deleting.clone();
        let delete_may_finish = delete_may_finish.clone();
        async move {
            super::remove_recorded(
                &home.app_dir,
                &Diagnostics::noop(),
                id,
                async {
                    deleting.notify_one();
                    delete_may_finish.notified().await;
                    handle.delete_store(coven::IfUnsent::Refuse).await
                },
                &|_| (),
            )
            .await
        }
    });
    deleting.notified().await;
    assert!(home.record(id).exists());

    let lookup = tokio::spawn({
        let home = home.clone();
        async move { home.unfinished().await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!lookup.is_finished(), "the lookup waits for the removal");

    delete_may_finish.notify_one();
    removal.await.unwrap().unwrap();
    assert_eq!(lookup.await.unwrap(), None);
    assert_eq!(home.stores().await, 0);
    assert!(!home.record(id).exists());
    assert!(!home.directory(id).exists());
}

/// What removing a library that isn't open would lose is read by opening its
/// store briefly; the store is closed again, so it opens normally afterwards.
#[tokio::test]
async fn unsent_work_of_a_closed_library_leaves_it_closed() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    handle.close().await.unwrap();
    drop(handle);

    let work = super::unsent_before_removal(home.installation.builder(), id, |_| ()).await.unwrap();
    assert!(work.is_empty(), "{work:?}");
    let reopened =
        bae_db::configure_store(home.installation.builder()).open(id, |_| ()).await.unwrap();
    assert_eq!(reopened.store_name().await.unwrap(), "Library");
}

/// A library whose settings can't be read doesn't open, so what it hasn't
/// sent can't be counted. It is removed without opening: its store, its
/// directory and the pointer naming it are gone, and the library beside it
/// stays.
#[tokio::test]
async fn a_library_whose_settings_cant_be_read_is_removed_without_opening() {
    let home = Home::new();
    let kept = home.library("Kept").await;
    let handle = home.library("Unreadable").await;
    home.connect(&handle).await;
    let id = handle.store_id();
    handle.close().await.unwrap();
    drop(handle);
    home.point_at(id);
    std::fs::write(
        home._temp.path().join(".bae").join("stores").join(id.to_string()).join("storage.json"),
        "{not settings",
    )
    .unwrap();
    let listed = home.app_dir.discover_libraries(&home.installation).await.unwrap();
    assert!(
        listed.iter().any(|library| library.id == id.to_string()
            && matches!(library.state, bae_app_dir::LibraryState::Unreadable(_))),
        "{listed:?}"
    );
    super::unsent_before_removal(home.installation.builder(), id, |_| ())
        .await
        .expect_err("counting opens the library, and its settings can't be read");

    let steps = std::sync::Mutex::new(Vec::new());
    remove_unopenable_library(
        &home.app_dir,
        &home.installation,
        &Diagnostics::noop(),
        id,
        &|step| steps.lock().unwrap().push(step),
    )
    .await
    .unwrap();
    assert_eq!(
        *steps.lock().unwrap(),
        [crate::ClosingStep::LeavingLibrary, crate::ClosingStep::DeletingFiles]
    );
    assert_eq!(home.stores().await, 1);
    assert!(!home.directory(id).exists());
    assert_eq!(home.pointer(), None);
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
    assert_eq!(kept.store_name().await.unwrap(), "Kept");
    assert!(home.directory(kept.store_id()).exists());
}

/// coven refusing the delete, here because the library is open, removes
/// nothing and leaves no record to report.
#[tokio::test]
async fn an_unopenable_removal_coven_refuses_removes_nothing() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    let error = remove_unopenable_library(
        &home.app_dir,
        &home.installation,
        &Diagnostics::noop(),
        id,
        &|_| (),
    )
    .await
    .expect_err("an open library holds its lock");
    assert!(matches!(error, LibraryRemovalError::Store(_)), "{error}");
    assert_eq!(handle.store_name().await.unwrap(), "Library");
    assert!(home.directory(id).exists());
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
}
