//! Removing watched roots off the coordinator's loop: the folder watches
//! taken down and the store change made, and the watches put back when the
//! store change does not land.

use super::*;

/// Stop watching `roots`, watching `parent` in their place when given.
pub(super) async fn run_root_removal(
    roots: &[PathBuf],
    parent: Option<&Path>,
    scans: Vec<RootScanTask>,
    backend: &dyn RootRemovalBackend,
    folder_state_commit: crate::FolderStateCommit,
) -> RootRemovalResult {
    let described = describe(roots, parent);
    // Each pass is waited out, however it ended: one that panicked was
    // reported where it ran, and installs no more watches.
    for scan in scans {
        scan.ended().await;
    }
    let mut uninstalled: Vec<(&PathBuf, FolderWatchSnapshot)> = Vec::new();
    for root in roots {
        match backend.uninstall(root).await {
            Ok(snapshot) => uninstalled.push((root, snapshot)),
            Err(error) => {
                let error =
                    format!("could not remove folder watch for {}: {error}", root.display());
                return RootRemovalResult::Failed(
                    restore_watches(backend, &uninstalled, error).await,
                );
            }
        }
    }
    let commit = folder_state_commit
        .lock(match parent {
            None => "remove a watched folder",
            Some(_) => "watch a folder in place of the watched folders inside it",
        })
        .await;
    match backend.remove_durable_roots(roots, parent).await {
        Ok(removed) => RootRemovalResult::Removed { commit, removed },
        Err(error) => {
            drop(commit);
            let error = format!("could not remove {described}: {error}");
            RootRemovalResult::Failed(restore_watches(backend, &uninstalled, error).await)
        }
    }
}

/// What a removal changes, in words for its errors.
fn describe(roots: &[PathBuf], parent: Option<&Path>) -> String {
    let roots = roots.iter().map(|root| root.display().to_string()).collect::<Vec<_>>().join(", ");
    match parent {
        None => format!("watched folder {roots}"),
        Some(parent) => {
            format!("watched folders {roots} for {} to watch in their place", parent.display())
        }
    }
}

/// Put back the watches a removal took down, adding any that would not go
/// back to `error`.
async fn restore_watches(
    backend: &dyn RootRemovalBackend,
    uninstalled: &[(&PathBuf, FolderWatchSnapshot)],
    error: String,
) -> String {
    let mut detail = error;
    for (root, snapshot) in uninstalled {
        if let Err(rollback) = backend.reinstall(root, snapshot).await {
            detail.push_str(&format!(
                "; restoring the folder watch for {} also failed: {rollback}",
                root.display()
            ));
        }
    }
    detail
}

pub(super) enum RootRemovalResult {
    Removed { commit: crate::FolderStateCommitGuard, removed: crate::tables::WatchedFoldersRemoved },
    Failed(String),
}

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
        self.diagnostics
            .blocking("remove a folder's watch", move || watcher.uninstall(&path))
            .await
            .map_err(|failed| failed.to_string())
    }

    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        let snapshot = snapshot.clone();
        self.diagnostics
            .blocking("restore a folder's watch", move || watcher.reinstall(&path, &snapshot))
            .await
            .map_err(|failed| failed.to_string())?
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
