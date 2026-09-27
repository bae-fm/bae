//! The folder watches and store change a removal of watched roots makes.

use super::*;

#[async_trait::async_trait]
pub(super) trait RootRemovalBackend: Send + Sync {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String>;
    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String>;
    /// Stop watching `roots` in the store — watching `parent` in their place
    /// when given — and return the keys of the releases that left the queue.
    async fn remove_durable_roots(
        &self,
        roots: &[PathBuf],
        parent: Option<&Path>,
    ) -> Result<Vec<String>, String>;
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

    async fn remove_durable_roots(
        &self,
        roots: &[PathBuf],
        parent: Option<&Path>,
    ) -> Result<Vec<String>, String> {
        self.library_manager
            .remove_watched_import_folders(
                roots
                    .iter()
                    .map(|root| root.to_string_lossy().into_owned())
                    .collect(),
                parent.map(|parent| parent.to_string_lossy().into_owned()),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| match parent {
                None => format!("{roots:?} is not watched"),
                Some(parent) => format!(
                    "{} already watches the folders inside it",
                    parent.display()
                ),
            })
    }
}
