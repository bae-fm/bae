//! What a removal or a takeover of a watched root does outside the
//! coordinator: the folder watch it takes down or puts back, and the durable
//! rows it deletes or moves.

use super::*;

#[async_trait::async_trait]
pub(super) trait RootRemovalBackend: Send + Sync {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String>;
    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String>;
    /// Delete the root's rows and return the scan entry keys that went with
    /// them.
    async fn remove_durable_root(&self, path: &Path) -> Result<Vec<String>, String>;
    /// Watch `parent` in place of the watched folders `inner` inside it, in
    /// one write that keeps what was decided about their candidates.
    async fn adopt_durable_roots(&self, parent: &Path, inner: &[PathBuf]) -> Result<(), String>;
}

pub(super) struct ServiceRootRemovalBackend {
    folder_watcher: Arc<FolderWatcher>,
    library_manager: LibraryManager,
}

impl ServiceRootRemovalBackend {
    pub(super) fn new(folder_watcher: Arc<FolderWatcher>, library_manager: LibraryManager) -> Self {
        Self {
            folder_watcher,
            library_manager,
        }
    }
}

#[async_trait::async_trait]
impl RootRemovalBackend for ServiceRootRemovalBackend {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || watcher.uninstall(&path))
            .await
            .map_err(|error| format!("folder watch removal task panicked: {error}"))?
            .map_err(|error| error.to_string())
    }

    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || watcher.reinstall(&path, &snapshot))
            .await
            .map_err(|error| format!("folder watch restore task panicked: {error}"))?
            .map_err(|error| error.to_string())
    }

    async fn remove_durable_root(&self, path: &Path) -> Result<Vec<String>, String> {
        self.library_manager
            .remove_watched_import_folder(&path.to_string_lossy())
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("{} is not a watched folder", path.display()))
    }

    async fn adopt_durable_roots(&self, parent: &Path, inner: &[PathBuf]) -> Result<(), String> {
        self.library_manager
            .adopt_watched_import_folders(
                &parent.to_string_lossy(),
                inner
                    .iter()
                    .map(|root| root.to_string_lossy().into_owned())
                    .collect(),
            )
            .await
            .map_err(|error| error.to_string())
    }
}
