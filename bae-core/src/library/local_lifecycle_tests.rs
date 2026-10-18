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
    installation: coven::TestCoven,
    storage: Arc<coven::MemoryStorage>,
}

impl Home {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let app_dir = AppDir::under_home(temp.path());
        Self {
            _temp: temp,
            app_dir,
            installation: coven::TestCoven::new(),
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

    fn directory(&self, id: coven::StoreId) -> std::path::PathBuf {
        self.app_dir.registered_library(&id.to_string())
    }

    /// The directory holding the active-library pointer.
    fn app_root(&self) -> std::path::PathBuf {
        self.app_dir
            .active_library_pointer()
            .parent()
            .unwrap()
            .to_owned()
    }

    fn point_at(&self, id: coven::StoreId) {
        std::fs::write(self.app_dir.active_library_pointer(), id.to_string()).unwrap();
    }

    fn pointer(&self) -> Option<String> {
        Config::active_library_id(&self.app_dir).unwrap()
    }

    /// What the person is shown when the app next lists its libraries.
    async fn unfinished(&self) -> Option<String> {
        UnfinishedRemoval::find(&self.app_dir)
            .await
            .unwrap()
            .map(|removal| removal.to_string())
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
    remove_local_library(&home.app_dir, &removed, coven::IfUnsent::Refuse)
        .await
        .unwrap();
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
    remove_local_library(&home.app_dir, &handle, coven::IfUnsent::Refuse)
        .await
        .unwrap();
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
    handle
        .setup_s3_storage(
            home.storage.config(),
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
    home.point_at(id);
    home.storage.set_online(false);
    let error = remove_local_library(&home.app_dir, &handle, coven::IfUnsent::Discard)
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
    assert_eq!(home.unfinished().await, None);
    home.storage.set_online(true);
    remove_local_library(&home.app_dir, &handle, coven::IfUnsent::Discard)
        .await
        .unwrap();
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert_eq!(home.unfinished().await, None);
}

/// The pointer can't be cleared: the removal says the store is gone, the
/// pointer and directory stay listed as unfinished, and a retry deletes both
/// once the cause is fixed.
#[cfg(unix)]
#[tokio::test]
async fn a_pointer_that_cannot_be_cleared_leaves_the_removal_to_retry() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    set_writable(&home.app_root(), false);
    let error = remove_local_library(&home.app_dir, &handle, coven::IfUnsent::Refuse)
        .await
        .unwrap_err();
    assert!(
        matches!(error, LibraryRemovalError::Unfinished(_)),
        "{error}"
    );
    assert!(error.to_string().contains("store was removed"), "{error}");
    assert_eq!(error.category(), crate::ui::UiErrorCategory::Internal);
    assert_eq!(home.stores().await, 0);
    assert_eq!(home.pointer(), Some(id.to_string()));
    assert!(home.directory(id).exists());
    assert!(home.unfinished().await.unwrap().contains(&id.to_string()));

    assert!(finish_removals(&home.app_dir).await.is_err());
    set_writable(&home.app_root(), true);
    finish_removals(&home.app_dir).await.unwrap();
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert_eq!(home.unfinished().await, None);
    finish_removals(&home.app_dir).await.unwrap();
}

/// The directory can't be deleted: the pointer is already cleared, the
/// directory stays listed as unfinished, and a retry deletes it.
#[cfg(unix)]
#[tokio::test]
async fn a_directory_that_cannot_be_deleted_leaves_the_removal_to_retry() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    set_writable(&home.app_dir.libraries(), false);
    let error = remove_local_library(&home.app_dir, &handle, coven::IfUnsent::Refuse)
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
    assert!(home.unfinished().await.unwrap().contains(&id.to_string()));

    set_writable(&home.app_dir.libraries(), true);
    finish_removals(&home.app_dir).await.unwrap();
    assert!(!home.directory(id).exists());
    assert_eq!(home.unfinished().await, None);
}

/// The app stops right after coven deletes the store: when it starts again
/// the removal is read from disk, and finishing it deletes bae's files.
#[tokio::test]
async fn a_removal_stopped_after_the_store_was_deleted_is_found_again() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    handle.delete_store(coven::IfUnsent::Refuse).await.unwrap();
    drop(handle);

    assert!(home.unfinished().await.unwrap().contains(&id.to_string()));
    finish_removals(&home.app_dir).await.unwrap();
    assert_eq!(home.pointer(), None);
    assert!(!home.directory(id).exists());
    assert_eq!(home.unfinished().await, None);
}

/// A pointer naming a store coven no longer holds is listed as unfinished
/// even with no directory left, and never marks a library active.
#[tokio::test]
async fn a_pointer_left_without_its_store_is_listed_and_cleared() {
    let home = Home::new();
    let handle = home.library("Library").await;
    let id = handle.store_id();
    home.point_at(id);
    handle.delete_store(coven::IfUnsent::Refuse).await.unwrap();
    std::fs::remove_dir_all(home.directory(id)).unwrap();

    assert!(Config::discover_libraries(&home.app_dir)
        .await
        .unwrap()
        .is_empty());
    assert!(home.unfinished().await.unwrap().contains(&id.to_string()));
    finish_removals(&home.app_dir).await.unwrap();
    assert_eq!(home.pointer(), None);
    assert_eq!(home.unfinished().await, None);
}

/// Finishing removals never touches a library coven still holds, nor
/// anything under the libraries directory that isn't a library's.
#[tokio::test]
async fn finishing_removals_leaves_libraries_on_this_device_alone() {
    let home = Home::new();
    let kept = home.library("Kept").await;
    let removed = home.library("Removed").await;
    home.point_at(kept.store_id());
    removed.delete_store(coven::IfUnsent::Refuse).await.unwrap();
    std::fs::write(home.app_dir.libraries().join(".DS_Store"), b"").unwrap();
    std::fs::create_dir(home.app_dir.libraries().join("not-a-library")).unwrap();

    let unfinished = home.unfinished().await.unwrap();
    assert!(unfinished.contains(&removed.store_id().to_string()));
    assert!(!unfinished.contains(&kept.store_id().to_string()));
    finish_removals(&home.app_dir).await.unwrap();
    assert!(!home.directory(removed.store_id()).exists());
    assert!(home.directory(kept.store_id()).exists());
    assert_eq!(home.pointer(), Some(kept.store_id().to_string()));
    assert!(home.app_dir.libraries().join(".DS_Store").exists());
    assert!(home.app_dir.libraries().join("not-a-library").exists());
    assert_eq!(home.unfinished().await, None);
}
