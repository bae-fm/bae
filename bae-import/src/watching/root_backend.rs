//! The folder watches and store change a removal of watched roots makes.

use super::*;

#[async_trait::async_trait]
pub(super) trait RootRemovalBackend: Send + Sync {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String>;
    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String>;
    /// Stop watching `roots` in the store — watching `parent` in their place
    /// when given — and return the releases that left the queue and the
    /// folders still watched, as the write left them.
    async fn remove_durable_roots(
        &self,
        roots: &[PathBuf],
        parent: Option<&Path>,
    ) -> Result<crate::tables::WatchedFoldersRemoved, String>;
}

pub(super) struct ServiceRootRemovalBackend {
    folder_watcher: Arc<FolderWatcher>,
    tables: crate::tables::ImportTables,
    diagnostics: bae_diagnostics::Diagnostics,
}

impl ServiceRootRemovalBackend {
    pub(super) fn new(
        folder_watcher: Arc<FolderWatcher>,
        tables: crate::tables::ImportTables,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self { folder_watcher, tables, diagnostics }
    }
}

#[async_trait::async_trait]
impl RootRemovalBackend for ServiceRootRemovalBackend {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        bae_util::task_failure::blocking("remove a folder's watch", move || {
            watcher.uninstall(&path)
        })
        .await
        .map_err(|failed| self.diagnostics.bug(failed).to_string())
    }

    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        let snapshot = snapshot.clone();
        bae_util::task_failure::blocking("restore a folder's watch", move || {
            watcher.reinstall(&path, &snapshot)
        })
        .await
        .map_err(|failed| self.diagnostics.bug(failed).to_string())?
        .map_err(|error| error.to_string())
    }

    async fn remove_durable_roots(
        &self,
        roots: &[PathBuf],
        parent: Option<&Path>,
    ) -> Result<crate::tables::WatchedFoldersRemoved, String> {
        self.tables
            .remove_watched_import_folders(roots.to_vec(), parent.map(Path::to_path_buf))
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| match parent {
                None => format!("{roots:?} is not watched"),
                Some(parent) => {
                    format!("{} already watches the folders inside it", parent.display())
                }
            })
    }
}
