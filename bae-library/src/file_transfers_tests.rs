use super::*;

fn progress(bytes_transferred: u64, bytes_total: u64) -> coven::TransferProgress {
    coven::TransferProgress { bytes_transferred, bytes_total }
}

/// A file's share is the bytes of its object moved over the object's
/// size, and an object of no size has moved none.
#[test]
fn a_files_share_is_its_objects_bytes_moved() {
    assert_eq!(fraction(progress(25, 100)), 0.25);
    assert_eq!(fraction(progress(100, 100)), 1.0);
    assert_eq!(fraction(progress(0, 0)), 0.0);
}

/// A reader holds every file the reset names, then each one updated, and
/// none removed.
#[test]
fn a_reader_holds_each_file_in_flight() {
    let (a, b) = (coven::FileId(uuid::Uuid::from_u128(1)), coven::FileId(uuid::Uuid::from_u128(2)));
    let mut held = BTreeMap::new();
    apply(
        &mut held,
        vec![coven::TransferChange::Reset([(a, progress(1, 10))].into_iter().collect())],
    );
    apply(
        &mut held,
        vec![
            coven::TransferChange::Updated { file: b, progress: progress(2, 10) },
            coven::TransferChange::Removed { file: a, end: coven::TransferEnd::Finished },
        ],
    );
    assert_eq!(held, [(b, progress(2, 10))].into_iter().collect());
}
