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
use bae_app_dir::{AppDir, AppDirError};
use bae_diagnostics::Diagnostics;
use std::collections::{BTreeSet, HashSet};
use tracing::{debug, warn};

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
    Store(coven::StoreDeletionError),
    /// This device left the library, but closing its store failed and
    /// discarding the closed store then failed too: the library's files are
    /// still on this device.
    #[error(
        "this device left the library, but closing its store failed ({closing}) and discarding it failed, so its files are still on this device: {discard}"
    )]
    Closing {
        closing: coven::SyncError,
        #[source]
        discard: coven::StoreDeletionError,
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
            Self::Store(error) | Self::Closing { discard: error, .. } => {
                bae_diagnostics::deletion_category(error)
            }
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
        let _lock = RemovalLock::acquire(app_dir, diagnostics).await?;
        let records = removal_records(app_dir, diagnostics).await?;
        if records.is_empty() {
            return Ok(None);
        }
        let listed = listed_stores(coven).await?;
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
    let _lock = RemovalLock::acquire(app_dir, diagnostics).await?;
    let records = removal_records(app_dir, diagnostics).await?;
    if records.is_empty() {
        return Ok(());
    }
    let total = records.len() as u64;
    finished(0, total);
    let listed = listed_stores(coven).await?;
    for (done, id) in (1..).zip(records) {
        if !listed.contains(&id) {
            delete_library_files(app_dir, diagnostics, id).await?;
        }
        delete_record(app_dir, diagnostics, id).await?;
        finished(done, total);
    }
    Ok(())
}

/// Leave the store before removing this installation, discarding what this
/// device hasn't uploaded. Offline failures leave the store and bae's files
/// intact; the caller retries the same request. The caller must stop its
/// playback and import services first.
pub async fn remove_local_library(
    app_dir: &AppDir,
    coven: &coven::Coven,
    diagnostics: &Diagnostics,
    handle: &coven::CovenHandle,
) -> Result<(), LibraryRemovalError> {
    remove_recorded(
        app_dir,
        coven,
        diagnostics,
        handle.store_id(),
        handle.delete_store(coven::IfUnsent::Discard),
        &|_| (),
    )
    .await
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

/// What removing a library that isn't open would lose, read by opening its
/// store just long enough to count, as
/// [`crate::LibraryManager::unsent_before_removal`] does for an open
/// one. The open reports its steps to `on_open_progress`, as any open does.
pub async fn unsent_before_removal(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    on_open_progress: impl Fn(coven::Progress<coven::OpenStep>),
) -> Result<coven::UnsentWork, LibraryError> {
    let handle =
        bae_db::configure_store(builder, crate::SCHEMA).open(store, on_open_progress).await?;
    let unsent = bae_db::Database::from_handle(
        handle.clone(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidIds),
    )
    .unsent_before_removal()
    .await;
    let closed = handle.close().await;
    let unsent = unsent?;
    closed?;
    Ok(unsent)
}

/// Record the removal of `store_id`, run `delete_store`, then delete bae's
/// files and the record, telling `steps` as leaving and deleting start. A
/// delete that fails closing the store after this device left it is finished
/// by `coven` discarding the closed store. A refused delete clears the record
/// again.
pub(crate) async fn remove_recorded(
    app_dir: &AppDir,
    coven: &coven::Coven,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
    delete_store: impl std::future::Future<Output = Result<(), coven::StoreDeletionError>>,
    steps: &(dyn Fn(super::ClosingStep) + Send + Sync),
) -> Result<(), LibraryRemovalError> {
    let _lock =
        RemovalLock::acquire(app_dir, diagnostics).await.map_err(LibraryRemovalError::Record)?;
    write_record(app_dir, diagnostics, store_id).await.map_err(LibraryRemovalError::Record)?;
    steps(super::ClosingStep::LeavingLibrary);
    let deleted = match delete_store.await {
        Err(coven::StoreDeletionError::Closing(closing)) => {
            warn!(%store_id, %closing, "closing the store failed after this device left it; discarding the closed store");
            coven
                .discard_store(&coven.store_dir(&store_id))
                .await
                .map_err(|discard| LibraryRemovalError::Closing { closing, discard })
        }
        deleted => deleted.map_err(LibraryRemovalError::Store),
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

/// The removal lock: an exclusive OS lock on one file in the app directory,
/// released when dropped. Separate opens of the file exclude each other, so
/// it orders removals and lookups within this process and across processes.
struct RemovalLock {
    _file: coven::FileLock,
}

impl RemovalLock {
    /// Wait for the lock: take it at once if no one holds it, else block
    /// until its holder lets it go.
    async fn acquire(app_dir: &AppDir, diagnostics: &Diagnostics) -> Result<Self, LibraryError> {
        let path = app_dir.removal_lock();
        file_work(diagnostics, "take the removal lock", move || {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
                .map_err(|error| io_at(&path, error))?;
            let attempt = file.try_clone().map_err(|error| io_at(&path, error))?;
            match coven::FileLock::try_lock(attempt, coven::LockMode::Exclusive) {
                Ok(file) => return Ok(Self { _file: file }),
                Err(std::fs::TryLockError::WouldBlock) => {
                    #[cfg(test)]
                    removal_lock_waits::waiting(&path);
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(io_at(&path, error)),
            }
            let file = coven::FileLock::lock(file, coven::LockMode::Exclusive)
                .map_err(|error| io_at(&path, error))?;
            Ok(Self { _file: file })
        })
        .await
    }
}

/// Where a test hears that a call found the removal lock held and is about
/// to wait for it.
#[cfg(test)]
pub(crate) mod removal_lock_waits {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static WATCHED: Mutex<Vec<(PathBuf, tokio::sync::oneshot::Sender<()>)>> =
        Mutex::new(Vec::new());

    /// Told once a call finds the lock at `path` held.
    pub(crate) fn when_waiting(path: PathBuf) -> tokio::sync::oneshot::Receiver<()> {
        let (told, telling) = tokio::sync::oneshot::channel();
        WATCHED.lock().unwrap().push((path, told));
        telling
    }

    pub(super) fn waiting(path: &Path) {
        let mut watched = WATCHED.lock().unwrap();
        let (found, rest): (Vec<_>, Vec<_>) =
            std::mem::take(&mut *watched).into_iter().partition(|(watched, _)| watched == path);
        *watched = rest;
        for (_, told) in found {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a test that stopped listening has no use for it"
            )]
            let _ = told.send(());
        }
    }
}

async fn listed_stores(coven: &coven::Coven) -> Result<HashSet<coven::StoreId>, LibraryError> {
    Ok(coven.stores().await.map_err(AppDirError::from)?.into_iter().map(|store| store.id).collect())
}

/// Write the record, synced along with the directories naming it.
async fn write_record(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let records = app_dir.removal_records();
    file_work(diagnostics, "write a removal record", move || {
        std::fs::create_dir_all(&records).map_err(|error| io_at(&records, error))?;
        let parent = records.parent().expect("the records directory has a parent");
        bae_util::fs::sync_directory(parent).map_err(|error| io_at(parent, error))?;
        let id = store_id.to_string();
        coven::write_atomic(&records.join(&id), id.as_bytes()).map_err(AppDirError::from)?;
        Ok(())
    })
    .await
}

async fn delete_record(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let record = app_dir.removal_records().join(store_id.to_string());
    file_work(diagnostics, "delete a removal record", move || match std::fs::remove_file(&record) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_at(&record, error)),
    })
    .await
}

/// The ids of the recorded removals. Anything else in the records directory
/// (a temporary file an interrupted write left) is passed over.
async fn removal_records(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
) -> Result<BTreeSet<coven::StoreId>, LibraryError> {
    let root = app_dir.removal_records();
    file_work(diagnostics, "list the removal records", move || {
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeSet::new())
            }
            Err(error) => return Err(io_at(&root, error)),
        };
        let mut records = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|error| io_at(&root, error))?;
            let path = entry.path();
            let is_file = entry.file_type().map_err(|error| io_at(&path, error))?.is_file();
            match entry.file_name().to_str().and_then(parse_library_id) {
                Some(id) if is_file => {
                    records.insert(id);
                }
                _ => debug!(path = %path.display(), "not a removal record"),
            }
        }
        Ok(records)
    })
    .await
}

/// A library id exactly as bae writes it: a canonical UUID.
fn parse_library_id(name: &str) -> Option<coven::StoreId> {
    let id = uuid::Uuid::parse_str(name).ok()?;
    (id.to_string() == name).then_some(coven::StoreId(id))
}

/// Clear the pointer if it names `store_id`, then delete the library's
/// directory.
async fn delete_library_files(
    app_dir: &AppDir,
    diagnostics: &Diagnostics,
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    file_work(diagnostics, "delete a removed library's files", move || {
        let id = store_id.to_string();
        let pointer = app_dir.active_library_pointer();
        match std::fs::read_to_string(&pointer) {
            Ok(current) if current.trim() == id => {
                std::fs::remove_file(&pointer).map_err(|error| io_at(&pointer, error))?
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_at(&pointer, error)),
        }
        let directory = app_dir.registered_library(&id);
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_at(&directory, error)),
        }
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
    bae_util::task_failure::blocking(operation, work)
        .await
        .map_err(|failed| diagnostics.bug(failed))?
}

/// Keep the path in the error: a bare "Permission denied" names no file.
fn io_at(path: &std::path::Path, error: std::io::Error) -> LibraryError {
    LibraryError::Io(bae_util::fs::io_error_at(path, error))
}

#[cfg(test)]
#[path = "local_lifecycle_tests.rs"]
mod tests;
