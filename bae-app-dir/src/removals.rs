//! The files bae keeps in its own directory about removing a library from
//! this device: one record per removal that started and has not finished, the
//! lock that orders removals, and the deletion of a removed library's
//! directory and active-library pointer. Every call blocks on file system
//! work, so an async caller runs it on a blocking thread.

use crate::{AppDir, AppDirError};
use std::collections::BTreeSet;
use std::path::Path;
use tracing::debug;

/// The removal lock: an exclusive OS lock on one file in the app directory,
/// released when dropped. Separate opens of the file exclude each other, so
/// it orders removals and lookups within this process and across processes.
#[derive(Debug)]
pub struct RemovalLock {
    _file: coven::FileLock,
}

impl AppDir {
    /// Wait for the removal lock: take it at once if no one holds it, else
    /// block until its holder lets it go.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn lock_removals(&self) -> Result<RemovalLock, AppDirError> {
        let path = self.removal_lock();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|error| io_at(&path, error))?;
        let attempt = file.try_clone().map_err(|error| io_at(&path, error))?;
        match coven::FileLock::try_lock(attempt, coven::LockMode::Exclusive) {
            Ok(file) => return Ok(RemovalLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => {
                #[cfg(any(test, feature = "test-utils"))]
                removal_lock_waits::waiting(&path);
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(io_at(&path, error)),
        }
        let file = coven::FileLock::lock(file, coven::LockMode::Exclusive)
            .map_err(|error| io_at(&path, error))?;
        Ok(RemovalLock { _file: file })
    }

    /// Record that the removal of `store` has started, synced along with the
    /// directories naming the record.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn record_removal(&self, store: coven::StoreId) -> Result<(), AppDirError> {
        let records = self.removal_records();
        std::fs::create_dir_all(&records).map_err(|error| io_at(&records, error))?;
        let parent = records.parent().expect("the records directory has a parent");
        sync_directory(parent).map_err(|error| io_at(parent, error))?;
        let id = store.to_string();
        coven::write_atomic(&records.join(&id), id.as_bytes())?;
        Ok(())
    }

    /// Delete the record of `store`'s removal; succeeds when there is none.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn clear_removal_record(&self, store: coven::StoreId) -> Result<(), AppDirError> {
        let record = self.removal_records().join(store.to_string());
        match std::fs::remove_file(&record) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_at(&record, error)),
        }
    }

    /// The ids of the recorded removals. Anything else in the records
    /// directory (a temporary file an interrupted write left) is passed over.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn recorded_removals(&self) -> Result<BTreeSet<coven::StoreId>, AppDirError> {
        let root = self.removal_records();
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
    }

    /// Clear the active-library pointer if it names `store`, then delete the
    /// library's directory of preferences and caches. Succeeds when both are
    /// already gone.
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )]
    pub fn delete_library_files(&self, store: coven::StoreId) -> Result<(), AppDirError> {
        let id = store.to_string();
        let pointer = self.active_library_pointer();
        match std::fs::read_to_string(&pointer) {
            Ok(current) if current.trim() == id => {
                std::fs::remove_file(&pointer).map_err(|error| io_at(&pointer, error))?
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_at(&pointer, error)),
        }
        let directory = self.registered_library(&id);
        match std::fs::remove_dir_all(&directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_at(&directory, error)),
        }
    }
}

/// A library id exactly as bae writes it: a canonical UUID.
fn parse_library_id(name: &str) -> Option<coven::StoreId> {
    let id = uuid::Uuid::parse_str(name).ok()?;
    (id.to_string() == name).then_some(coven::StoreId(id))
}

/// Keep the path in the error: a bare "Permission denied" names no file.
fn io_at(path: &Path, error: std::io::Error) -> AppDirError {
    AppDirError::Io(bae_util::fs::io_error_at(path, error))
}

/// Make the entries just created in `directory` durable. Unix makes a new
/// entry durable only once its directory is synced, which std does by opening
/// the directory as a file. std's `File::open` can't open a directory on
/// Windows (access is denied); `OpenOptionsExt::custom_flags` with
/// `FILE_FLAG_BACKUP_SEMANTICS` can, which bae doesn't do, so there the synced
/// file is all bae syncs; coven syncs directories on Unix only as well.
#[cfg_attr(
    unix,
    expect(
        clippy::disallowed_methods,
        reason = "file system: bae's own directory, which coven does not own"
    )
)]
pub(crate) fn sync_directory(directory: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(directory)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        Ok(())
    }
}

/// Where a test hears that a call found the removal lock held and is about
/// to wait for it.
#[cfg(any(test, feature = "test-utils"))]
pub mod removal_lock_waits {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    static WATCHED: Mutex<Vec<(PathBuf, tokio::sync::oneshot::Sender<()>)>> =
        Mutex::new(Vec::new());

    /// Told once a call finds the lock at `path` held.
    pub fn when_waiting(path: PathBuf) -> tokio::sync::oneshot::Receiver<()> {
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
