//! The dates the file system gives for a folder — when it was added to the
//! folder it is in, created, and last modified — read once here, and the two
//! choices made from them:
//!
//! - the import list orders the releases it shows by [`FolderDate`], when the
//!   folder arrived: date added, else created, never modified, since a
//!   modification does not say when a folder arrived;
//! - a pass walks the folders directly under a watched root newest first by
//!   [`FolderTimes::latest`], so a folder the person just added, or one whose
//!   own contents just changed — an album added inside an artist's folder —
//!   shows its releases before the rest of the library is read. A wrong guess
//!   only changes which releases show first, never what the pass finds.

use super::scan::{ScanCancellation, FOLDERS_AT_ONCE};
use super::FolderScanError;
use bae_util::task_failure::{caught_sync, Bug};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

#[cfg(target_os = "macos")]
mod attributes;

/// The dates the file system gives for one folder. Each is `None` where the
/// system keeps no such date for it or it could not be read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FolderTimes {
    /// When it was added to the folder it is in, moves included: kept on
    /// macOS, though not for a folder on an SMB share.
    pub(crate) added: Option<SystemTime>,
    /// When it was created. A copy made with `cp -p`, `ditto` or `rsync -a`
    /// keeps the original's; Windows gives a copy a new one; Linux keeps one
    /// only on file systems that record it.
    pub(crate) created: Option<SystemTime>,
    pub(crate) modified: Option<SystemTime>,
}

impl FolderTimes {
    /// The dates of the folder at `path`: its date added where the system
    /// keeps one, and what its metadata gives. A file system that keeps no
    /// date added or no creation time gives none; any other failure to read
    /// them fails.
    fn read(path: &Path) -> io::Result<Self> {
        #[cfg(target_os = "macos")]
        let added = attributes::added(path)?;
        #[cfg(not(target_os = "macos"))]
        let added = None;
        Ok(Self { added, ..Self::of_metadata(path)? })
    }

    /// What `path`'s metadata gives, which carries no date added.
    fn of_metadata(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        Ok(Self {
            added: None,
            created: creation_time(path, metadata.created())?,
            modified: metadata.modified().ok(),
        })
    }

    /// The latest of the dates the system gives: when the folder was added,
    /// created, or last had an entry directly in it change, whichever came
    /// last; `None` when it gives none.
    pub(crate) fn latest(&self) -> Option<SystemTime> {
        [self.added, self.created, self.modified].into_iter().flatten().max()
    }

    /// When the folder arrived, as far as the system says: its date added,
    /// else its creation time. Never its modification time.
    fn arrival(&self) -> Option<FolderDate> {
        let millis = |at: SystemTime| chrono::DateTime::<chrono::Utc>::from(at).timestamp_millis();
        match (self.added, self.created) {
            (Some(added), _) => Some(FolderDate::AddedToDirectory(millis(added))),
            (None, Some(created)) => Some(FolderDate::Created(millis(created))),
            (None, None) => None,
        }
    }
}

/// `created` as a creation time: `None` where the platform or file system
/// keeps none.
fn creation_time(path: &Path, created: io::Result<SystemTime>) -> io::Result<Option<SystemTime>> {
    match created {
        Ok(at) => Ok(Some(at)),
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            tracing::debug!(path = %path.display(), "folder creation time is unavailable");
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// When a folder arrived, in Unix epoch milliseconds, with the file system
/// fact they are: what the import list orders releases by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderDate {
    AddedToDirectory(i64),
    Created(i64),
}

impl FolderDate {
    /// When the folder at `path` arrived: its date added where the system
    /// keeps one, else its creation time where it keeps that.
    pub fn read(path: &Path) -> Result<Option<Self>, FolderScanError> {
        FolderTimes::read(path)
            .map(|times| times.arrival())
            .map_err(|error| FolderScanError::io(path, error))
    }
}

/// The dates of each of `folders`, directly under `root` and relative to it,
/// in their order. On macOS one `getattrlistbulk` over `root` reads them all;
/// a folder it gives nothing for, and every folder elsewhere, is read on its
/// own, several at once. A folder whose dates can't be read has none; only a
/// cancelled pass fails this.
pub(crate) fn times_of_folders(
    root: &Path,
    folders: &[PathBuf],
    cancellation: &ScanCancellation,
) -> Result<Vec<FolderTimes>, FolderScanError> {
    cancellation.check()?;
    let mut times: Vec<Option<FolderTimes>> = vec![None; folders.len()];
    #[cfg(target_os = "macos")]
    match attributes::read_directory(root) {
        Ok(mut entries) => {
            for (folder, times) in folders.iter().zip(&mut times) {
                *times = folder.file_name().and_then(|name| entries.remove(name));
            }
        }
        Err(error) => {
            tracing::debug!(root = %root.display(), %error, "folder dates are unreadable in one batch");
        }
    }
    cancellation.check()?;
    let unread: Vec<usize> = (0..folders.len()).filter(|index| times[*index].is_none()).collect();
    for (index, read) in unread.iter().zip(read_each_at_once(root, folders, &unread, cancellation)?)
    {
        times[*index] = Some(read);
    }
    Ok(times.into_iter().map(|times| times.expect("every folder's dates are read")).collect())
}

/// The dates of `folders[index]` for each of `indices`, in their order, each
/// read on its own, [`FOLDERS_AT_ONCE`] at a time: on a network share each is
/// a round trip, and several in flight overlap their waits.
fn read_each_at_once(
    root: &Path,
    folders: &[PathBuf],
    indices: &[usize],
    cancellation: &ScanCancellation,
) -> Result<Vec<FolderTimes>, FolderScanError> {
    let next = AtomicUsize::new(0);
    let read: Vec<std::sync::Mutex<FolderTimes>> =
        indices.iter().map(|_| Default::default()).collect();
    // Cancelled with the pass, and by a read that panics, so the others stop
    // without the pass reading as cancelled.
    let stop = cancellation.child();
    let panicked: std::sync::Mutex<Option<Bug>> = std::sync::Mutex::new(None);
    std::thread::scope(|threads| {
        for _ in 0..FOLDERS_AT_ONCE.min(indices.len()) {
            let (next, read, stop, panicked) = (&next, &read, &stop, &panicked);
            threads.spawn(move || {
                let reading = caught_sync("read folders' dates", || {
                    while !stop.is_cancelled() {
                        let place = next.fetch_add(1, Ordering::Relaxed);
                        let Some(index) = indices.get(place) else {
                            break;
                        };
                        let path = root.join(&folders[*index]);
                        let times = FolderTimes::of_metadata(&path).unwrap_or_else(|error| {
                            tracing::debug!(path = %path.display(), %error, "folder dates are unreadable");
                            FolderTimes::default()
                        });
                        *read[place].lock().expect("folder dates mutex poisoned") = times;
                    }
                });
                if let Err(failed) = reading {
                    stop.cancel();
                    panicked
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get_or_insert(failed);
                }
            });
        }
    });
    if let Some(failed) = panicked.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
        return Err(FolderScanError::Panicked(failed));
    }
    cancellation.check()?;
    Ok(read
        .into_iter()
        .map(|times| times.into_inner().expect("folder dates mutex poisoned"))
        .collect())
}

/// The order a pass walks folders whose dates are `times` in, as indices
/// into it: newest [`FolderTimes::latest`] first, the undated after, and
/// folders with the same date, or none, in the order `times` lists them.
pub(crate) fn newest_first(times: &[FolderTimes]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..times.len()).collect();
    // `None` sorts below every date, so reversed it goes last; the sort is
    // stable, so ties keep their order.
    order.sort_by_key(|index| std::cmp::Reverse(times[*index].latest()));
    order
}

#[cfg(test)]
#[path = "folder_times_tests.rs"]
mod tests;
