//! The work a removal or an adoption runs off the coordinator's loop: waiting
//! out the pass it stopped, taking the watches down, and the durable change —
//! with the watches put back when that change does not land.
//!
//! An adoption is a folder taking over the watched folders inside it, which
//! choosing such a folder asks for. Like a removal, it stops whatever reads
//! them and takes their watches down first, so nothing reads or watches them
//! while their rows change hands; unlike one, the durable change keeps
//! everything decided about their candidates, and the folder that takes over
//! is read whole straight after.

use super::*;

pub(super) fn answer(waiter: RefreshCompletion, result: Result<(), String>) {
    if waiter.send(result).is_err() {
        debug!("folder adoption caller dropped before it was answered");
    }
}

pub(super) async fn run_root_adoption(
    parent: &Path,
    inner: &[PathBuf],
    scans: Vec<RootScanTask>,
    backend: &dyn RootRemovalBackend,
    folder_state_commit: crate::import::FolderStateCommit,
) -> Result<crate::import::FolderStateCommitGuard, String> {
    for scan in scans {
        scan.task.await.map_err(|error| {
            format!(
                "folder scan task failed while {} took over the folders inside it: {error}",
                parent.display()
            )
        })?;
    }
    let mut uninstalled: Vec<(&PathBuf, FolderWatchSnapshot)> = Vec::new();
    for root in inner {
        match backend.uninstall(root).await {
            Ok(snapshot) => uninstalled.push((root, snapshot)),
            Err(error) => {
                let error = format!(
                    "could not remove folder watch for {}: {error}",
                    root.display()
                );
                return Err(restore_watches(backend, &uninstalled, error).await);
            }
        }
    }
    let commit = folder_state_commit
        .lock("take over the watched folders inside a folder")
        .await;
    if let Err(error) = backend.adopt_durable_roots(parent, inner).await {
        drop(commit);
        let error = format!(
            "could not watch {} in place of the folders inside it: {error}",
            parent.display()
        );
        return Err(restore_watches(backend, &uninstalled, error).await);
    }
    Ok(commit)
}

/// Put back the watches an adoption took down, and say what went wrong —
/// including a watch that would not go back.
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

pub(super) async fn run_root_removal(
    path: &Path,
    scan: Option<RootScanTask>,
    backend: &dyn RootRemovalBackend,
    folder_state_commit: crate::import::FolderStateCommit,
) -> RootRemovalResult {
    if let Some(scan) = scan {
        if let Err(error) = scan.task.await {
            return RootRemovalResult::Failed(format!(
                "folder scan task failed while removing {}: {error}",
                path.display()
            ));
        }
    }
    let watch_snapshot = match backend.uninstall(path).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return RootRemovalResult::Failed(format!(
                "could not remove folder watch for {}: {error}",
                path.display()
            ));
        }
    };
    let commit = folder_state_commit.lock("remove a watched folder").await;
    let removed_keys = match backend.remove_durable_root(path).await {
        Ok(removed_keys) => removed_keys,
        Err(error) => {
            drop(commit);
            let rollback = backend.reinstall(path, &watch_snapshot).await;
            let detail = match rollback {
                Ok(()) => format!(
                    "could not remove watched folder {}: {error}",
                    path.display()
                ),
                Err(rollback_error) => format!(
                    "could not remove watched folder {}: {error}; restoring its folder watch also \
                 failed: {rollback_error}",
                    path.display()
                ),
            };
            return RootRemovalResult::Failed(detail);
        }
    };
    RootRemovalResult::Removed {
        commit,
        removed_keys,
    }
}
