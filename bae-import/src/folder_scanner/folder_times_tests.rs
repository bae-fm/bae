use super::*;
use std::time::{Duration, UNIX_EPOCH};

fn at(seconds: u64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

#[test]
fn a_folder_is_as_new_as_the_latest_date_the_system_gives() {
    // Just added: a copy that kept its old created and modified dates.
    let added_last = FolderTimes { added: at(30), created: at(20), modified: at(10) };
    assert_eq!(added_last.latest(), at(30));
    // Something just changed directly in it, long after it was added.
    let modified_last = FolderTimes { added: at(5), created: at(5), modified: at(60) };
    assert_eq!(modified_last.latest(), at(60));
    // No date added kept, as on an SMB share.
    let created_last = FolderTimes { added: None, created: at(50), modified: at(40) };
    assert_eq!(created_last.latest(), at(50));
    let only_modified = FolderTimes { modified: at(10), ..Default::default() };
    assert_eq!(only_modified.latest(), at(10));
    assert_eq!(FolderTimes::default().latest(), None);
}

#[test]
fn a_folder_arrived_when_it_was_added_else_created_never_modified() {
    let all = FolderTimes { added: at(30), created: at(20), modified: at(60) };
    assert_eq!(all.arrival(), Some(FolderDate::AddedToDirectory(30_000)));
    let no_added = FolderTimes { added: None, ..all };
    assert_eq!(no_added.arrival(), Some(FolderDate::Created(20_000)));
    let only_modified = FolderTimes { modified: at(60), ..Default::default() };
    assert_eq!(only_modified.arrival(), None);
}

#[test]
fn unavailable_creation_time_is_absent_but_io_errors_propagate() {
    let path = Path::new("folder");
    assert_eq!(creation_time(path, Err(io::ErrorKind::Unsupported.into())).unwrap(), None);
    assert_eq!(
        creation_time(path, Err(io::ErrorKind::PermissionDenied.into())).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(creation_time(path, Ok(UNIX_EPOCH)).unwrap(), Some(UNIX_EPOCH));
}

#[test]
fn reads_an_existing_folder_and_reports_a_missing_one() {
    let directory = tempfile::tempdir().unwrap();
    let date = FolderDate::read(directory.path()).unwrap();
    #[cfg(target_os = "macos")]
    assert!(matches!(date, Some(FolderDate::AddedToDirectory(_))), "{date:?}");
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
    let modified = |seconds| FolderTimes { modified: at(seconds), ..Default::default() };
    let times = [
        FolderTimes::default(),
        modified(10),
        FolderTimes { added: at(40), ..Default::default() },
        modified(20),
        FolderTimes::default(),
        // Added long ago, and its latest date ties the folder before it.
        FolderTimes { added: at(1), created: at(1), modified: at(20) },
        FolderTimes { created: at(30), ..Default::default() },
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
    let folders = [PathBuf::from("One"), PathBuf::from("Missing"), PathBuf::from("Two")];
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

    let folders = [PathBuf::from("Already There"), PathBuf::from("Arrives Later")];
    let times = times_of_folders(&root, &folders, &ScanCancellation::new()).unwrap();
    assert!(times[1].added > times[0].added, "{times:?}");
    assert!(times[0].modified > times[1].modified, "{times:?}");
    assert_eq!(newest_first(&times), vec![1, 0]);
}
