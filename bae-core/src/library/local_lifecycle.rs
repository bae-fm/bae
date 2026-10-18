//! Removing a library from this device. bae first records the removal in its
//! own app directory; coven then deletes the store; bae clears the
//! active-library pointer if it names the library, deletes the library's
//! directory of preferences and caches, and deletes the record last. bae and
//! coven can't commit together, so the record is what says a removal started:
//! a removal that stops partway is found from its record, never from bae
//! files that coven's store list lacks, and finished with
//! [`finish_removals`]. Every step succeeds when already done.

use crate::config::{AppDir, ConfigError};
use crate::library::LibraryError;
use std::collections::{BTreeSet, HashSet};
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
pub enum LibraryRemovalError {
    /// Recording the removal failed, so nothing was removed.
    #[error("couldn't record the removal, so nothing was removed: {0}")]
    Record(#[source] LibraryError),
    /// This device holds writes and files it hasn't uploaded, so nothing was
    /// removed. Removing with `IfUnsent::Discard` loses them.
    #[error("{} writes and {} files wait to upload", .0.writes, .0.files)]
    Unsent(coven::UnsentWork),
    /// coven didn't delete the store; bae's files for it are untouched.
    #[error(transparent)]
    Store(coven::StoreDeletionError),
    /// coven deleted the store, but deleting bae's files for it or its record
    /// failed. The record keeps it an [`UnfinishedRemoval`] until
    /// [`finish_removals`] completes it.
    #[error("the store was removed, but deleting this device's files for it failed: {0}")]
    Unfinished(#[source] LibraryError),
}

impl From<coven::StoreDeletionError> for LibraryRemovalError {
    fn from(error: coven::StoreDeletionError) -> Self {
        match error {
            coven::StoreDeletionError::Unsent(work) => Self::Unsent(work),
            error => Self::Store(error),
        }
    }
}

impl LibraryRemovalError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        match self {
            Self::Unsent(_) => crate::ui::UiErrorCategory::Config,
            Self::Store(error) => super::manager::deletion_category(error),
            Self::Record(source) | Self::Unfinished(source) => source.category(),
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
            ids.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
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
    pub async fn find(app_dir: &AppDir) -> Result<Option<Self>, LibraryError> {
        let records = removal_records(app_dir).await?;
        if records.is_empty() {
            return Ok(None);
        }
        let listed = listed_stores(app_dir).await?;
        let (not_removed, files_remain): (Vec<_>, Vec<_>) =
            records.into_iter().partition(|id| listed.contains(id));
        for &id in &not_removed {
            delete_record(app_dir, id).await?;
        }
        Ok(Some(Self {
            files_remain,
            not_removed,
        }))
    }

    /// Whether [`finish_removals`] has files left to delete.
    pub fn files_remain(&self) -> bool {
        !self.files_remain.is_empty()
    }

    pub fn category(&self) -> crate::ui::UiErrorCategory {
        crate::ui::UiErrorCategory::Config
    }
}

/// Complete every recorded removal: for a store coven no longer lists, delete
/// bae's files for it, then its record; for one coven still lists, only the
/// record, since the delete never happened.
pub async fn finish_removals(app_dir: &AppDir) -> Result<(), LibraryError> {
    let records = removal_records(app_dir).await?;
    if records.is_empty() {
        return Ok(());
    }
    let listed = listed_stores(app_dir).await?;
    for id in records {
        if !listed.contains(&id) {
            delete_library_files(app_dir, id).await?;
        }
        delete_record(app_dir, id).await?;
    }
    Ok(())
}

/// Leave the store before removing this installation. Offline failures leave
/// the store and bae's files intact; the caller retries the same request.
/// The caller must stop its playback and import services first.
pub async fn remove_local_library(
    app_dir: &AppDir,
    handle: &coven::CovenHandle,
    unsent: coven::IfUnsent,
) -> Result<(), LibraryRemovalError> {
    remove_recorded(app_dir, handle.store_id(), handle.delete_store(unsent)).await
}

/// Record the removal of `store_id`, run `delete_store`, then delete bae's
/// files and the record. A refused delete clears the record again.
pub(crate) async fn remove_recorded(
    app_dir: &AppDir,
    store_id: coven::StoreId,
    delete_store: impl std::future::Future<Output = Result<(), coven::StoreDeletionError>>,
) -> Result<(), LibraryRemovalError> {
    write_record(app_dir, store_id)
        .await
        .map_err(LibraryRemovalError::Record)?;
    if let Err(error) = delete_store.await {
        if let Err(record) = delete_record(app_dir, store_id).await {
            warn!(%store_id, %record, "the removal record outlives coven's refusal; finding unfinished removals reports it");
        }
        return Err(error.into());
    }
    delete_library_files(app_dir, store_id)
        .await
        .map_err(LibraryRemovalError::Unfinished)?;
    delete_record(app_dir, store_id)
        .await
        .map_err(LibraryRemovalError::Unfinished)
}

async fn listed_stores(app_dir: &AppDir) -> Result<HashSet<coven::StoreId>, LibraryError> {
    Ok(app_dir
        .store_layout()
        .stores()
        .await
        .map_err(ConfigError::from)?
        .into_iter()
        .map(|store| store.id)
        .collect())
}

/// Write the record, synced along with the directories naming it.
async fn write_record(app_dir: &AppDir, store_id: coven::StoreId) -> Result<(), LibraryError> {
    let records = app_dir.removal_records();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&records).map_err(|error| io_at(&records, error))?;
        #[cfg(unix)]
        {
            let parent = records
                .parent()
                .expect("the records directory has a parent");
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| io_at(parent, error))?;
        }
        let id = store_id.to_string();
        coven::write_atomic(&records.join(&id), id.as_bytes()).map_err(ConfigError::from)?;
        Ok(())
    })
    .await
    .map_err(unfinished_work)?
}

async fn delete_record(app_dir: &AppDir, store_id: coven::StoreId) -> Result<(), LibraryError> {
    let record = app_dir.removal_records().join(store_id.to_string());
    tokio::task::spawn_blocking(move || match std::fs::remove_file(&record) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_at(&record, error)),
    })
    .await
    .map_err(unfinished_work)?
}

/// The ids of the recorded removals. Anything else in the records directory
/// (a temporary file an interrupted write left) is passed over.
async fn removal_records(app_dir: &AppDir) -> Result<BTreeSet<coven::StoreId>, LibraryError> {
    let root = app_dir.removal_records();
    tokio::task::spawn_blocking(move || {
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
            let is_file = entry
                .file_type()
                .map_err(|error| io_at(&path, error))?
                .is_file();
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
    .map_err(unfinished_work)?
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
    store_id: coven::StoreId,
) -> Result<(), LibraryError> {
    let app_dir = app_dir.clone();
    tokio::task::spawn_blocking(move || {
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
    .map_err(unfinished_work)?
}

fn unfinished_work(error: tokio::task::JoinError) -> LibraryError {
    LibraryError::Internal(format!("removal file work did not complete: {error}"))
}

/// Keep the path in the error: a bare "Permission denied" names no file.
fn io_at(path: &std::path::Path, error: std::io::Error) -> LibraryError {
    LibraryError::Io(std::io::Error::new(
        error.kind(),
        format!("{}: {error}", path.display()),
    ))
}
