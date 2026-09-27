//! A removal's work off the coordinator's loop, putting the watches back when
//! the store change does not land.

use super::*;

/// Stop watching `roots`, watching `parent` in their place when given.
pub(super) async fn run_root_removal(
    roots: &[PathBuf],
    parent: Option<&Path>,
    scans: Vec<RootScanTask>,
    backend: &dyn RootRemovalBackend,
    folder_state_commit: crate::import::FolderStateCommit,
) -> RootRemovalResult {
    let described = describe(roots, parent);
    for scan in scans {
        if let Err(error) = scan.task.await {
            return RootRemovalResult::Failed(format!(
                "folder scan task failed while removing {described}: {error}"
            ));
        }
    }
    let mut uninstalled: Vec<(&PathBuf, FolderWatchSnapshot)> = Vec::new();
    for root in roots {
        match backend.uninstall(root).await {
            Ok(snapshot) => uninstalled.push((root, snapshot)),
            Err(error) => {
                let error = format!(
                    "could not remove folder watch for {}: {error}",
                    root.display()
                );
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
        Ok(removed_keys) => RootRemovalResult::Removed {
            commit,
            removed_keys,
        },
        Err(error) => {
            drop(commit);
            let error = format!("could not remove {described}: {error}");
            RootRemovalResult::Failed(restore_watches(backend, &uninstalled, error).await)
        }
    }
}

/// What a removal changes, in words for its errors.
fn describe(roots: &[PathBuf], parent: Option<&Path>) -> String {
    let roots = roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    match parent {
        None => format!("watched folder {roots}"),
        Some(parent) => format!(
            "watched folders {roots} for {} to watch in their place",
            parent.display()
        ),
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
