use super::*;
use bae_transfers::DownloadTransferProgress;
use bae_views::SendingFile;

fn file(n: u128) -> coven::FileId {
    coven::FileId(uuid::Uuid::from_u128(n))
}

fn bar(bytes_done: u64, bytes_total: u64) -> Option<BatchBar> {
    Some(BatchBar { bytes_done, bytes_total })
}

/// A read of a pin batch: its size, the bytes of its done releases, and the
/// releases not done, each with its size.
fn pin_batch(bytes_total: u64, bytes_done: u64, waiting: &[(&str, u64)]) -> ActivityBytes {
    ActivityBytes {
        uploads: None,
        pins: Some(DownloadBytes {
            bytes_total,
            bytes_done,
            waiting: waiting
                .iter()
                .map(|(release_id, bytes_total)| bae_views::WaitingRelease {
                    release_id: release_id.to_string(),
                    bytes_total: *bytes_total,
                })
                .collect(),
        }),
        moving_to_this_device: None,
    }
}

/// `release`'s pin has downloaded `bytes_done` of its `bytes_total`.
fn pinning(
    release: &str,
    bytes_done: u64,
    bytes_total: u64,
) -> Vec<ReleaseTransferChange<DownloadTransfer>> {
    vec![ReleaseTransferChange::Updated {
        release_id: release.to_string(),
        progress: DownloadTransfer::Pin(
            DownloadTransferProgress::new(release, bytes_done, bytes_total).unwrap(),
        ),
    }]
}

/// `release`'s transfer is no longer under way.
fn ended(release: &str) -> Vec<ReleaseTransferChange<DownloadTransfer>> {
    vec![ReleaseTransferChange::Removed { release_id: release.to_string() }]
}

fn pins_bar(count: &BarsCount) -> Option<BatchBar> {
    count.bars().expect("the tables were read").pins
}

/// A pin batch's bar counts each done release's whole size and, of the
/// release downloading, its transfer's share of its size: two pins of 100
/// and 300 bytes, the first done and the second half down, are 250 of 400.
#[test]
fn a_pin_batch_bar_counts_done_releases_and_the_active_share() {
    let mut count = BarsCount::default();
    assert_eq!(count.bars(), None, "nothing is known before the tables are read");

    count.read(pin_batch(400, 100, &[("second", 300)]));
    assert_eq!(pins_bar(&count), bar(100, 400));
    count.downloads_heard(pinning("second", 20, 40));
    assert_eq!(pins_bar(&count), bar(250, 400));
}

/// A release counted done is counted once: its size is in the tables' bytes
/// done, and no transfer adds its share.
#[test]
fn a_done_release_counts_its_size_once() {
    let mut count = BarsCount::default();
    count.read(pin_batch(400, 100, &[("second", 300)]));
    count.downloads_heard(pinning("second", 39, 40));
    count.read(pin_batch(400, 400, &[]));
    assert_eq!(pins_bar(&count), bar(400, 400));
    count.downloads_heard(ended("second"));
    assert_eq!(pins_bar(&count), bar(400, 400));
}

/// An upload's bar counts the bytes of the stored files and, of each file
/// being sent, the share of its size its object has moved; a file the watch
/// hasn't named yet has moved nothing.
#[test]
fn an_upload_bar_counts_stored_files_and_in_flight_shares() {
    let (a, b, c) = (file(1), file(2), file(3));
    let uploads = |bytes_done: u64, sending: &[(coven::FileId, u64)]| ActivityBytes {
        uploads: Some(UploadBytes {
            bytes_total: 1_500,
            bytes_done,
            sending: sending
                .iter()
                .map(|(file, size)| SendingFile { file: *file, size: *size })
                .collect(),
        }),
        pins: None,
        moving_to_this_device: None,
    };
    let moved =
        |bytes_transferred, bytes_total| coven::TransferProgress { bytes_transferred, bytes_total };
    let mut count = BarsCount::default();
    count.read(uploads(400, &[(a, 200), (b, 100), (c, 500)]));
    count.uploads_heard(vec![coven::TransferChange::Reset(
        [(a, moved(55, 220)), (b, moved(110, 110))].into(),
    )]);
    assert_eq!(count.bars().unwrap().uploads, bar(400 + 50 + 100, 1_500));

    count.read(uploads(500, &[(a, 200), (c, 500)]));
    assert_eq!(count.bars().unwrap().uploads, bar(500 + 50, 1_500));
}

/// With no batch, a line has no bar.
#[test]
fn no_batch_has_no_bar() {
    let mut count = BarsCount::default();
    count.read(pin_batch(100, 0, &[("first", 100)]));
    assert!(pins_bar(&count).is_some());
    count.read(ActivityBytes { uploads: None, pins: None, moving_to_this_device: None });
    assert_eq!(pins_bar(&count), None);
}

/// A file whose upload finished keeps its last bytes in the bar until a read
/// that no longer lists it as sending arrives: coven ends the file's progress
/// after the commit that stores it, but the read can deliver that commit
/// later, and the bar must not drop in between.
#[test]
fn a_finished_upload_keeps_its_bytes_until_the_read_that_stores_it() {
    let (a, b) = (file(1), file(2));
    let uploads = |bytes_done: u64, sending: &[(coven::FileId, u64)]| ActivityBytes {
        uploads: Some(UploadBytes {
            bytes_total: 300,
            bytes_done,
            sending: sending
                .iter()
                .map(|(file, size)| SendingFile { file: *file, size: *size })
                .collect(),
        }),
        pins: None,
        moving_to_this_device: None,
    };
    let moved =
        |bytes_transferred, bytes_total| coven::TransferProgress { bytes_transferred, bytes_total };
    let mut count = BarsCount::default();
    count.read(uploads(0, &[(a, 200), (b, 100)]));
    count.uploads_heard(vec![coven::TransferChange::Reset(
        [(a, moved(110, 110)), (b, moved(0, 110))].into(),
    )]);
    assert_eq!(count.bars().unwrap().uploads, bar(200, 300));

    count.uploads_heard(vec![coven::TransferChange::Removed {
        file: a,
        end: coven::TransferEnd::Finished,
    }]);
    assert_eq!(
        count.bars().unwrap().uploads,
        bar(200, 300),
        "the read that stores the file has not arrived"
    );

    count.read(uploads(200, &[(b, 100)]));
    assert_eq!(count.bars().unwrap().uploads, bar(200, 300));
    count.uploads_heard(vec![coven::TransferChange::Updated { file: b, progress: moved(55, 110) }]);
    assert_eq!(count.bars().unwrap().uploads, bar(250, 300));
}

/// A file whose upload failed or was held keeps nothing: it is still sending
/// and starts from its next progress.
#[test]
fn an_upload_that_did_not_finish_keeps_no_bytes() {
    let a = file(1);
    let moved =
        |bytes_transferred, bytes_total| coven::TransferProgress { bytes_transferred, bytes_total };
    let mut count = BarsCount::default();
    count.read(ActivityBytes {
        uploads: Some(UploadBytes {
            bytes_total: 200,
            bytes_done: 0,
            sending: vec![SendingFile { file: a, size: 200 }],
        }),
        pins: None,
        moving_to_this_device: None,
    });
    count.uploads_heard(vec![coven::TransferChange::Updated { file: a, progress: moved(50, 100) }]);
    count.uploads_heard(vec![coven::TransferChange::Removed {
        file: a,
        end: coven::TransferEnd::Failed,
    }]);
    assert_eq!(count.bars().unwrap().uploads, bar(0, 200));
}
