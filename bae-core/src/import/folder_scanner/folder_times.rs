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
        Ok(Self {
            added,
            ..Self::of_metadata(path)?
        })
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
        [self.added, self.created, self.modified]
            .into_iter()
            .flatten()
            .max()
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

    pub(crate) fn columns(self) -> (i64, &'static str) {
        match self {
            Self::AddedToDirectory(at) => (at, "added_to_directory"),
            Self::Created(at) => (at, "created"),
        }
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
    let unread: Vec<usize> = (0..folders.len())
        .filter(|index| times[*index].is_none())
        .collect();
    for (index, read) in unread
        .iter()
        .zip(read_each_at_once(root, folders, &unread, cancellation)?)
    {
        times[*index] = Some(read);
    }
    Ok(times
        .into_iter()
        .map(|times| times.expect("every folder's dates are read"))
        .collect())
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
    std::thread::scope(|threads| {
        for _ in 0..FOLDERS_AT_ONCE.min(indices.len()) {
            let (next, read) = (&next, &read);
            threads.spawn(move || {
                while !cancellation.is_cancelled() {
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
        }
    });
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
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn at(seconds: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn a_folder_is_as_new_as_the_latest_date_the_system_gives() {
        // Just added: a copy that kept its old created and modified dates.
        let added_last = FolderTimes {
            added: at(30),
            created: at(20),
            modified: at(10),
        };
        assert_eq!(added_last.latest(), at(30));
        // Something just changed directly in it, long after it was added.
        let modified_last = FolderTimes {
            added: at(5),
            created: at(5),
            modified: at(60),
        };
        assert_eq!(modified_last.latest(), at(60));
        // No date added kept, as on an SMB share.
        let created_last = FolderTimes {
            added: None,
            created: at(50),
            modified: at(40),
        };
        assert_eq!(created_last.latest(), at(50));
        let only_modified = FolderTimes {
            modified: at(10),
            ..Default::default()
        };
        assert_eq!(only_modified.latest(), at(10));
        assert_eq!(FolderTimes::default().latest(), None);
    }

    #[test]
    fn a_folder_arrived_when_it_was_added_else_created_never_modified() {
        let all = FolderTimes {
            added: at(30),
            created: at(20),
            modified: at(60),
        };
        assert_eq!(all.arrival(), Some(FolderDate::AddedToDirectory(30_000)));
        let no_added = FolderTimes { added: None, ..all };
        assert_eq!(no_added.arrival(), Some(FolderDate::Created(20_000)));
        let only_modified = FolderTimes {
            modified: at(60),
            ..Default::default()
        };
        assert_eq!(only_modified.arrival(), None);
    }

    #[test]
    fn unavailable_creation_time_is_absent_but_io_errors_propagate() {
        let path = Path::new("folder");
        assert_eq!(
            creation_time(path, Err(io::ErrorKind::Unsupported.into())).unwrap(),
            None
        );
        assert_eq!(
            creation_time(path, Err(io::ErrorKind::PermissionDenied.into()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            creation_time(path, Ok(UNIX_EPOCH)).unwrap(),
            Some(UNIX_EPOCH)
        );
    }

    #[test]
    fn reads_an_existing_folder_and_reports_a_missing_one() {
        let directory = tempfile::tempdir().unwrap();
        let date = FolderDate::read(directory.path()).unwrap();
        #[cfg(target_os = "macos")]
        assert!(
            matches!(date, Some(FolderDate::AddedToDirectory(_))),
            "{date:?}"
        );
        #[cfg(not(target_os = "macos"))]
        let _ = date;
        assert!(FolderDate::read(&directory.path().join("missing")).is_err());
    }

    /// A folder's own read and the root's batch say the same of it.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_folder_read_alone_has_the_dates_its_batch_gives() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("One")).unwrap();
        let alone = FolderTimes::read(&root.path().join("One")).unwrap();
        let batch = attributes::read_directory(root.path()).unwrap();
        assert_eq!(batch[std::ffi::OsStr::new("One")], alone);
    }

    #[test]
    fn newest_first_puts_the_undated_last_and_keeps_ties_in_order() {
        let modified = |seconds| FolderTimes {
            modified: at(seconds),
            ..Default::default()
        };
        let times = [
            FolderTimes::default(),
            modified(10),
            FolderTimes {
                added: at(40),
                ..Default::default()
            },
            modified(20),
            FolderTimes::default(),
            // Added long ago, and its latest date ties the folder before it.
            FolderTimes {
                added: at(1),
                created: at(1),
                modified: at(20),
            },
            FolderTimes {
                created: at(30),
                ..Default::default()
            },
        ];
        assert_eq!(newest_first(&times), vec![2, 6, 3, 5, 1, 0, 4]);
    }

    #[test]
    fn folders_listed_with_no_dates_keep_their_order() {
        assert_eq!(newest_first(&[FolderTimes::default(); 3]), vec![0, 1, 2]);
    }

    #[test]
    fn reads_every_folder_and_leaves_a_missing_one_undated() {
        let root = tempfile::tempdir().unwrap();
        for name in ["One", "Two"] {
            std::fs::create_dir(root.path().join(name)).unwrap();
        }
        let folders = [
            PathBuf::from("One"),
            PathBuf::from("Missing"),
            PathBuf::from("Two"),
        ];
        let times = times_of_folders(root.path(), &folders, &ScanCancellation::new()).unwrap();
        assert_eq!(times.len(), 3);
        assert!(times[0].modified.is_some() && times[2].modified.is_some());
        assert_eq!(times[1], FolderTimes::default());
        #[cfg(target_os = "macos")]
        assert!(times[0].added.is_some() && times[0].created.is_some());
    }

    #[test]
    fn a_cancelled_pass_reads_no_dates() {
        let root = tempfile::tempdir().unwrap();
        let cancellation = ScanCancellation::new();
        cancellation.cancel();
        assert!(matches!(
            times_of_folders(root.path(), &[PathBuf::from("One")], &cancellation),
            Err(FolderScanError::Cancelled)
        ));
    }

    /// A folder moved in later is newer than one made earlier with a newer
    /// modified date than the moved folder kept: its date added is its
    /// latest, and it is walked first.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_folder_moved_in_later_is_walked_before_one_modified_later() {
        let place = tempfile::tempdir().unwrap();
        let root = place.path().join("Library");
        std::fs::create_dir(&root).unwrap();
        // Made outside the root first, with an old modified date.
        let elsewhere = place.path().join("Arrives Later");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::File::open(&elsewhere)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000_000))
            .unwrap();
        // Made in the root, with a modified date newer than the moved folder's
        // but before either was made.
        let already = root.join("Already There");
        std::fs::create_dir(&already).unwrap();
        std::fs::File::open(&already)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(3_600))
            .unwrap();
        std::fs::rename(&elsewhere, root.join("Arrives Later")).unwrap();

        let folders = [
            PathBuf::from("Already There"),
            PathBuf::from("Arrives Later"),
        ];
        let times = times_of_folders(&root, &folders, &ScanCancellation::new()).unwrap();
        assert!(times[1].added > times[0].added, "{times:?}");
        assert!(times[0].modified > times[1].modified, "{times:?}");
        assert_eq!(newest_first(&times), vec![1, 0]);
    }
}
