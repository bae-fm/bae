//! Removing a library from this device. bae first records the removal in its
//! own app directory; coven then deletes the store; bae clears the
//! active-library pointer if it names the library, deletes the library's
//! directory of preferences and caches, and deletes the record last. bae and
//! coven can't commit together, so the record is what says a removal started:
//! a removal that stops partway is found from its record, never from bae
//! files that coven's store list lacks, and finished with
//! [`finish_removals`]. Every step succeeds when already done. A removal,
//! a lookup and finishing each hold the removal lock for their whole run, so
//! a lookup never sees a removal that is still waiting on coven.

use crate::LibraryError;
use bae_app_dir::{AppDir, AppDirError, RemovalLock};
use bae_diagnostics::Diagnostics;
use std::collections::{BTreeSet, HashSet};
use tracing::warn;

#[derive(Debug, thiserror::Error)]
pub enum LibraryRemovalError {
    /// The library's services did not stop, so nothing was removed.
    #[error("the library's services didn't stop, so nothing was removed: {0}")]
    Stop(#[source] LibraryError),
    /// Recording the removal failed, so nothing was removed.
    #[error("couldn't record the removal, so nothing was removed: {0}")]
    Record(#[source] LibraryError),
    /// coven didn't delete the store; bae's files for it are untouched.
    #[error(transparent)]
    Store(Box<coven::StoreDeletionError>),
    /// coven didn't delete the store because files in its directory are open,
    /// which Windows refuses to move; bae's files for it are untouched.
    #[error(
        "coven couldn't delete the store because these files are open: {}: {source}",
        display_paths(held)
    )]
    StoreFilesHeld {
        #[source]
        source: Box<coven::StoreDeletionError>,
        held: Vec<std::path::PathBuf>,
    },
    /// This device left the library, but closing its store failed and
    /// discarding the closed store then failed too: the library's files are
    /// still on this device.
    #[error(
        "this device left the library, but closing its store failed ({closing}) and discarding it failed, so its files are still on this device: {discard}"
    )]
    Closing {
        closing: Box<coven::SyncError>,
        #[source]
        discard: Box<coven::StoreDeletionError>,
    },
    /// coven deleted the store, but deleting bae's files for it or its record
    /// failed. The record keeps it an [`UnfinishedRemoval`] until
    /// [`finish_removals`] completes it.
    #[error("the store was removed, but deleting this device's files for it failed: {0}")]
    Unfinished(#[source] LibraryError),
}

impl LibraryRemovalError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Store(error)
            | Self::StoreFilesHeld { source: error, .. }
            | Self::Closing { discard: error, .. } => bae_diagnostics::deletion_category(error),
            Self::Stop(source) | Self::Record(source) | Self::Unfinished(source) => {
                source.category()
            }
        }
    }
}

/// Removals recorded on this device that never finished, as their records
/// and coven's store list show them.
#[derive(Debug)]
pub struct UnfinishedRemoval {
    /// coven deleted these stores; bae's files for them may remain, and
    /// [`finish_removals`] deletes them.
    files_remain: Vec<coven::StoreId>,
    /// These removals stopped before coven deleted the store: the libraries
    /// are still on this device, and their records are already cleared.
    not_removed: Vec<coven::StoreId>,
}

impl std::fmt::Display for UnfinishedRemoval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids = |ids: &[coven::StoreId]| {
            ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
        };
        let mut parts = Vec::new();
        if !self.files_remain.is_empty() {
            parts.push(format!(
                "this device still has files for removed libraries: {}",
                ids(&self.files_remain)
            ));
        }
        if !self.not_removed.is_empty() {
            parts.push(format!(
                "removing these libraries stopped before they were deleted, so they are still on this device: {}",
                ids(&self.not_removed)
            ));
        }
        f.write_str(&parts.join("; "))
    }
}

impl UnfinishedRemoval {
    /// The recorded removals that never finished. A record whose store coven
    /// still lists is a removal that stopped before the delete: it is cleared
    /// here and reported, and the library can be removed again.
    pub async fn find(
        app_dir: &AppDir,
        coven: &coven::Coven,
        diagnostics: &Diagnostics,
    ) -> Result<Option<Self>, LibraryError> {
        let _lock = lock_removals(app_dir, diagnostics).await?;
        let records = removal_records(app_dir, diagnostics).await?;
        if records.is_empty() {
            return Ok(None);
        }
        let listed = listed_stores(coven, diagnostics).await?;
        let (not_removed, files_remain): (Vec<_>, Vec<_>) =
            records.into_iter().partition(|id| listed.contains(id));
        for &id in &not_removed {
            delete_record(app_dir, diagnostics, id).await?;
        }
        Ok(Some(Self { files_remain, not_removed }))
    }

    /// Whether [`finish_removals`] has files left to delete.
    pub fn files_remain(&self) -> bool {
        !self.files_remain.is_empty()
    }

    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        bae_diagnostics::UiErrorCategory::Config
    }
}

/// Complete every recorded removal: for a store coven no longer lists, delete
/// bae's files for it, then its record; for one coven still lists, only the
/// record, since the delete never happened. `finished` hears how many of the
/// recorded removals are done of how many there are, when there are any: none
/// of them once they are read, then each as it finishes.
pub async fn finish_removals(
    app_dir: &AppDir,
    coven: &coven::Coven,
    diagnostics: &Diagnostics,
    finished: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<(), LibraryError> {
    let _lock = lock_removals(app_dir, diagnostics).await?;
    let records = removal_records(app_dir, diagnostics).await?;
    if records.is_empty() {
        return Ok(());
    }
    let total = records.len() as u64;
    finished(0, total);
    let listed = listed_stores(coven, diagnostics).await?;
    for (done, id) in (1..).zip(records) {
        if !listed.contains(&id) {
            delete_library_files(app_dir, diagnostics, id).await?;
        }
        delete_record(app_dir, diagnostics, id).await?;
        finished(done, total);
    }
    Ok(())
}

/// Remove a library that can't open, its settings unreadable or a reset or
/// restore of its copy unfinished, without opening it: coven deletes this
/// device's keychain entries for it and its directory, and bae's files and
/// the record follow as in any removal. Whatever this device hadn't sent is
/// lost uncounted, since counting needs the library open, and this device
/// stays among the library's members, since leaving needs it open too.
/// For a reset or restore that stopped after its swap, coven deletes both
/// copies, the one it loaded and the one it replaced. Leaving and deleting
/// are told to `steps` as each starts.
pub async fn remove_unopenable_library(
    app_dir: &AppDir,
    coven: &coven::Coven,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
    steps: &(dyn Fn(super::ClosingStep) + Send + Sync),
) -> Result<(), LibraryRemovalError> {
    remove_recorded(
        app_dir,
        coven,
        diagnostics,
        store_id,
        coven.discard_store(&coven.store_dir(&store_id)),
        steps,
    )
    .await
}

/// Finish removing a library whose removal stopped partway, which coven
/// lists as being deleted and refuses to open: coven opens it and goes on as
/// the removal would, publishing this device's leaving if it hadn't, and
/// bae's files and the record follow as in any removal. `builder` carries
/// the sign-in a provider asks for. Leaving and deleting are told to `steps`
/// as each starts.
pub async fn finish_deletion(
    app_dir: &AppDir,
    coven: &coven::Coven,
    builder: coven::CovenBuilder,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
    steps: &(dyn Fn(super::ClosingStep) + Send + Sync),
) -> Result<(), LibraryRemovalError> {
    remove_recorded(
        app_dir,
        coven,
        diagnostics,
        store_id,
        bae_store::configure_store(builder, crate::SCHEMA).resume_deletion(store_id),
        steps,
    )
    .await
}

/// What removing a library that isn't open would lose, read by opening its
/// store just long enough to count, as
/// [`crate::LibraryManager::unsent_before_removal`] does for an open
/// one. The open reports its steps to `on_open_progress`, as any open does.
///
/// The open, and the store's calls, report a bug they carry back to
/// `diagnostics`.
pub async fn unsent_before_removal(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    on_open_progress: impl Fn(coven::Progress<coven::OpenStep>),
    clock: coven::ClockRef,
    diagnostics: &Diagnostics,
) -> Result<coven::UnsentWork, LibraryError> {
    let handle = bae_store::configure_store(builder, crate::SCHEMA)
        .open(store, on_open_progress)
        .await
        .map_err(|error| bae_store::DbError::from(coven::task::reported(diagnostics, error)))?;
    let store = bae_store::Store::new(handle, clock, diagnostics.bug_sink());
    let unsent = store.unsent_before_removal().await;
    let closed = store.close().await;
    let unsent = unsent?;
    closed?;
    Ok(unsent)
}

/// Record the removal of `store_id`, run `delete_store`, then delete bae's
/// files and the record, telling `steps` as leaving and deleting start. A
/// delete that fails closing the store after this device left it is finished
/// by `coven` discarding the closed store. A refused delete clears the record
/// again. A bug coven's calls carry back is reported to `diagnostics`.
pub(crate) async fn remove_recorded(
    app_dir: &AppDir,
    coven: &coven::Coven,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
    delete_store: impl std::future::Future<Output = Result<(), coven::StoreDeletionError>>,
    steps: &(dyn Fn(super::ClosingStep) + Send + Sync),
) -> Result<(), LibraryRemovalError> {
    let _lock = lock_removals(app_dir, diagnostics).await.map_err(LibraryRemovalError::Record)?;
    write_record(app_dir, diagnostics, store_id).await.map_err(LibraryRemovalError::Record)?;
    steps(super::ClosingStep::LeavingLibrary);
    let deleted = match delete_store
        .await
        .map_err(|error| coven::task::reported(diagnostics, error))
    {
        Err(coven::StoreDeletionError::Closing(closing)) => {
            warn!(%store_id, %closing, "closing the store failed after this device left it; discarding the closed store");
            coven.discard_store(&coven.store_dir(&store_id)).await.map_err(|discard| {
                LibraryRemovalError::Closing {
                    closing: Box::new(closing),
                    discard: Box::new(coven::task::reported(diagnostics, discard)),
                }
            })
        }
        Err(error) => {
            // The path of a failed move or removal is the store's directory.
            let held = match &error {
                coven::StoreDeletionError::File(coven::FileError::Io { path, .. }) => {
                    files_held_open(diagnostics, path.clone()).await
                }
                _ => Vec::new(),
            };
            if held.is_empty() {
                Err(LibraryRemovalError::Store(Box::new(error)))
            } else {
                Err(LibraryRemovalError::StoreFilesHeld { source: Box::new(error), held })
            }
        }
        Ok(()) => Ok(()),
    };
    if let Err(error) = deleted {
        if let Err(record) = delete_record(app_dir, diagnostics, store_id).await {
            warn!(%store_id, %record, "the removal record outlives coven's refusal; finding unfinished removals reports it");
        }
        return Err(error);
    }
    steps(super::ClosingStep::DeletingFiles);
    delete_library_files(app_dir, diagnostics, store_id)
        .await
        .map_err(LibraryRemovalError::Unfinished)?;
    delete_record(app_dir, diagnostics, store_id).await.map_err(LibraryRemovalError::Unfinished)
}

fn display_paths(paths: &[std::path::PathBuf]) -> String {
    paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
}

/// The files under `directory` that refuse an exclusive open, which are the
/// ones Windows keeps a directory from moving for. Empty on other platforms,
/// where an open file does not stop a rename, and when the walk itself fails.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: bae's own directory, which coven does not own"
)]
#[cfg(windows)]
async fn files_held_open(
    diagnostics: &Diagnostics,
    directory: std::path::PathBuf,
) -> Vec<std::path::PathBuf> {
    use std::os::windows::fs::OpenOptionsExt;
    fn walk(directory: &std::path::Path, held: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(directory) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, held);
            } else if std::fs::OpenOptions::new().read(true).share_mode(0).open(&path).is_err() {
                held.push(path);
            }
        }
    }
    diagnostics
        .blocking("list the store's open files", move || {
            let mut held = Vec::new();
            walk(&directory, &mut held);
            held
        })
        .await
        .unwrap_or_default()
}

#[cfg(not(windows))]
async fn files_held_open(_: &Diagnostics, _: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    Vec::new()
}

async fn listed_stores(
    coven: &coven::Coven,
    diagnostics: &Diagnostics,
) -> Result<HashSet<coven::StoreId>, LibraryError> {
    Ok(coven
        .stores()
        .await
        .map_err(|error| AppDirError::from(coven::task::reported(diagnostics, error)))?
        .into_iter()
        .map(|store| store.id())
        .collect())
}

/// Take the removal lock, held until the guard drops.
async fn lock_removals(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
) -> Result<RemovalLock, LibraryError> {
    let app_dir = app_dir.clone();
    file_work(diagnostics, "take the removal lock", move || Ok(app_dir.lock_removals()?)).await
}

async fn write_record(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    file_work(diagnostics, "write a removal record", move || Ok(app_dir.record_removal(store_id)?))
        .await
}

async fn delete_record(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    file_work(diagnostics, "delete a removal record", move || {
        Ok(app_dir.clear_removal_record(store_id)?)
    })
    .await
}

async fn removal_records(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
) -> Result<BTreeSet<coven::StoreId>, LibraryError> {
    let app_dir = app_dir.clone();
    file_work(diagnostics, "list the removal records", move || Ok(app_dir.recorded_removals()?))
        .await
}

async fn delete_library_files(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    file_work(diagnostics, "delete a removed library's files", move || {
        Ok(app_dir.delete_library_files(store_id)?)
    })
    .await
}

/// Run a removal's file work on a blocking thread. A panic in it is reported
/// as a bug and fails the step, which the removal's record lets run again.
async fn file_work<T: Send + 'static>(
    diagnostics: &Diagnostics,
    operation: &'static str,
    work: impl FnOnce() -> Result<T, LibraryError> + Send + 'static,
) -> Result<T, LibraryError> {
    diagnostics.blocking(operation, work).await?
}

#[cfg(test)]
#[path = "local_lifecycle_tests.rs"]
mod tests;
