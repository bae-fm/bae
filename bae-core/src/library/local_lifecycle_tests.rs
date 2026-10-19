use super::local_lifecycle::{
    finish_removals, remove_local_library, LibraryRemovalError, UnfinishedRemoval,
};
use crate::config::{AppDir, Config};
use coven::Storage;
use std::sync::Arc;
use tempfile::TempDir;

struct Home {
    _temp: TempDir,
    app_dir: AppDir,
    installation: Arc<coven::TestCoven>,
    storage: Arc<coven::MemoryStorage>,
}

impl Home {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let app_dir = AppDir::under_home(temp.path());
        Self {
            _temp: temp,
            app_dir,
            installation: Arc::new(coven::TestCoven::new()),
            storage: Arc::new(
                coven::MemoryStorage::builder()
                    .clock(Arc::new(coven::SystemClock))
                    .build()
                    .unwrap(),
            ),
        }
    }

    /// A library with its preferences written, as one this device has opened.
    async fn library(&self, name: &str) -> coven::CovenHandle {
        let handle = super::configure_store(self.installation.builder(self.app_dir.store_layout()))
            .storage_connector(self.storage.clone())
            .create_store(name)
            .await
            .unwrap();
        Config::with_defaults(handle.store_id(), self.directory(handle.store_id()))
            .save_preferences()
            .unwrap();
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
    }

    /// The libraries whose saved restore codes this device offers.
    async fn offered_codes(&self) -> Vec<coven::StoreId> {
        super::Keychain::InMemory(self.installation.clone())
            .saved_restore_codes(&self.app_dir)
            .await
            .unwrap()
            .into_iter()
            .map(|code| code.store_id)
            .collect()
    }

    fn directory(&self, id: coven::StoreId) -> std::path::PathBuf {
        self.app_dir.registered_library(&id.to_string())
    }

    fn point_at(&self, id: coven::StoreId) {
        std::fs::write(self.app_dir.active_library_pointer(), id.to_string()).unwrap();
    }

    fn pointer(&self) -> Option<String> {
        Config::active_library_id(&self.app_dir).unwrap()
    }

    /// What the person is shown when the app next lists its libraries, and
    /// whether it has files to finish deleting.
    async fn unfinished(&self) -> Option<(String, bool)> {
        UnfinishedRemoval::find(&self.app_dir)
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
        let removal = super::local_lifecycle::remove_recorded(&self.app_dir, id, async {
            delete.await?;
            reached.notify_one();
            std::future::pending().await
        });
        tokio::select! {
            result = removal => panic!("the removal was to stop partway: {result:?}"),
            () = reached.notified() => {}
        }
    }

    async fn stores(&self) -> usize {
        self.app_dir.store_layout().stores().await.unwrap().len()
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
    remove_local_library(&home.app_dir, &removed).await.unwrap();
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
    remove_local_library(&home.app_dir, &handle).await.unwrap();
    assert_eq!(home.stores().await, 0);
    assert!(!home.directory(id).exists());
    assert_eq!(home.pointer(), None);
    assert_eq!(home.unfinished().await, None);
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
    let error = remove_local_library(&home.app_dir, &handle)
        .await
        .expect_err("leaving needs reachable storage");
    assert!(matches!(error, LibraryRemovalError::Store(_)), "{error}");
    assert_eq!(error.category(), crate::ui::UiErrorCategory::Network);
    assert_eq!(handle.store_name().await.unwrap(), "Library");
    assert!(home
        .directory(id)
        .join(crate::config::PREFERENCES_FILENAME)
        .exists());
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
    home.storage.set_online(true);
    remove_local_library(&home.app_dir, &handle).await.unwrap();
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
    remove_local_library(&home.app_dir, &handle).await.unwrap();
    assert_eq!(home.offered_codes().await, vec![id]);
}

#[tokio::test]
async fn a_removal_that_cannot_be_recorded_removes_nothing() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    std::fs::write(home.app_dir.removal_records(), b"").unwrap();
    let error = remove_local_library(&home.app_dir, &handle)
        .await
        .unwrap_err();
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
    finish_removals(&home.app_dir).await.unwrap();
    assert_eq!(home.stores().await, 1);
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(home.directory(id).exists());

    remove_local_library(&home.app_dir, &handle).await.unwrap();
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
    home.stop_removal_after(id, handle.delete_store(coven::IfUnsent::Refuse))
        .await;
    drop(handle);

    let (shown, files_remain) = home.unfinished().await.unwrap();
    assert!(shown.contains(&id.to_string()), "{shown}");
    assert!(files_remain);
    assert!(home.record(id).exists());
    finish_removals(&home.app_dir).await.unwrap();
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert!(!home.record(id).exists());
    assert_eq!(home.unfinished().await, None);
    finish_removals(&home.app_dir).await.unwrap();
}

/// The pointer can't be read: the store is gone, the record keeps the removal
/// listed, and a retry finishes it once the cause is fixed.
#[tokio::test]
async fn a_pointer_that_cannot_be_cleared_leaves_the_removal_to_retry() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    std::fs::create_dir(home.app_dir.active_library_pointer()).unwrap();
    let error = remove_local_library(&home.app_dir, &handle)
        .await
        .unwrap_err();
    assert!(
        matches!(error, LibraryRemovalError::Unfinished(_)),
        "{error}"
    );
    assert!(error.to_string().contains("store was removed"), "{error}");
    assert_eq!(error.category(), crate::ui::UiErrorCategory::Internal);
    assert_eq!(home.stores().await, 0);
    assert!(home.directory(id).exists());
    assert!(home.record(id).exists());
    assert!(home.unfinished().await.unwrap().1);

    assert!(finish_removals(&home.app_dir).await.is_err());
    std::fs::remove_dir(home.app_dir.active_library_pointer()).unwrap();
    home.point_at(id);
    finish_removals(&home.app_dir).await.unwrap();
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
    let error = remove_local_library(&home.app_dir, &handle)
        .await
        .unwrap_err();
    assert!(
        matches!(error, LibraryRemovalError::Unfinished(_)),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains(&home.directory(id).display().to_string()),
        "{error}"
    );
    assert_eq!(home.stores().await, 0);
    assert_eq!(home.pointer(), None);
    assert!(home.directory(id).exists());
    assert!(home.unfinished().await.unwrap().1);

    set_writable(&home.app_dir.libraries(), true);
    finish_removals(&home.app_dir).await.unwrap();
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
    Config::with_defaults(hidden, home.directory(hidden))
        .save_preferences()
        .unwrap();
    home.point_at(hidden);
    let removed = home.library("Removed").await;
    home.stop_removal_after(
        removed.store_id(),
        removed.delete_store(coven::IfUnsent::Refuse),
    )
    .await;

    let (shown, _) = home.unfinished().await.unwrap();
    assert!(shown.contains(&removed.store_id().to_string()), "{shown}");
    assert!(!shown.contains(&hidden.to_string()), "{shown}");
    finish_removals(&home.app_dir).await.unwrap();
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
            super::local_lifecycle::remove_recorded(&home.app_dir, id, async {
                deleting.notify_one();
                delete_may_finish.notified().await;
                handle.delete_store(coven::IfUnsent::Refuse).await
            })
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

    let work =
        super::unsent_before_removal(home.installation.builder(home.app_dir.store_layout()), id)
            .await
            .unwrap();
    assert!(work.is_empty(), "{work:?}");
    let reopened = super::configure_store(home.installation.builder(home.app_dir.store_layout()))
        .open(id)
        .await
        .unwrap();
    assert_eq!(reopened.store_name().await.unwrap(), "Library");
}
