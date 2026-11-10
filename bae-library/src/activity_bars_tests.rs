use super::*;
use bae_transfers::DownloadTransferProgress;

fn file(n: u128) -> coven::FileId {
    coven::FileId(uuid::Uuid::from_u128(n))
}

fn bar(bytes_done: u64, bytes_total: u64) -> Option<BatchBar> {
    Some(BatchBar { bytes_done, bytes_total })
}

/// A read of a pin batch numbered `number`, each release with its size and
/// whether it is done, or `None` once it left the library.
fn pin_batch(number: u64, releases: &[(&str, Option<(u64, bool)>)]) -> ActivityBytes {
    ActivityBytes {
        uploads: None,
        pins: Some(BatchReleases {
            number,
            releases: releases
                .iter()
                .map(|(release_id, release)| {
                    let release =
                        release.map(|(bytes_total, done)| DownloadRelease { bytes_total, done });
                    (release_id.to_string(), release)
                })
                .collect(),
        }),
        making_local: None,
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

fn pins_bar(count: &mut BarsCount) -> Option<BatchBar> {
    count.bars().expect("the tables were read").pins
}

/// A pin batch's bar counts each done release's whole size and, of the
/// release downloading, its transfer's share of its size: two pins of 100
/// and 300 bytes, the first done and the second half down, are 250 of 400.
#[test]
fn a_pin_batch_bar_counts_done_releases_and_the_active_share() {
    let mut count = BarsCount::default();
    assert_eq!(count.bars(), None, "nothing is known before the tables are read");

    count.read(pin_batch(1, &[("first", Some((100, true))), ("second", Some((300, false)))]));
    assert_eq!(pins_bar(&mut count), bar(100, 400));
    count.downloads_heard(pinning("second", 20, 40));
    assert_eq!(pins_bar(&mut count), bar(250, 400));
}

/// The read and the transfers wake in any order. The release's transfer
/// ending before the read counts it done keeps the bytes it had reached,
/// and the read counting it done counts its size once, whether its
/// transfer still runs or not.
#[test]
fn the_bytes_done_never_go_back_as_a_release_finishes() {
    let mut count = BarsCount::default();
    let downloading = [("first", Some((100, true))), ("second", Some((300, false)))];
    let done = [("first", Some((100, true))), ("second", Some((300, true)))];
    count.read(pin_batch(1, &downloading));
    count.downloads_heard(pinning("second", 20, 40));
    assert_eq!(pins_bar(&mut count), bar(250, 400));

    count.downloads_heard(ended("second"));
    assert_eq!(pins_bar(&mut count), bar(250, 400), "the transfer ended first: the bytes hold");
    count.read(pin_batch(1, &done));
    assert_eq!(pins_bar(&mut count), bar(400, 400));

    let mut count = BarsCount::default();
    count.read(pin_batch(1, &downloading));
    count.downloads_heard(pinning("second", 39, 40));
    count.read(pin_batch(1, &done));
    assert_eq!(pins_bar(&mut count), bar(400, 400), "the read came first: counted once");
    count.downloads_heard(ended("second"));
    assert_eq!(pins_bar(&mut count), bar(400, 400));
}

/// A release that leaves the library while its batch lasts keeps the bytes
/// it had counted, done and total, so neither goes back; one gone before
/// any read counted it adds nothing.
#[test]
fn a_release_removed_from_the_library_keeps_what_it_counted() {
    let mut count = BarsCount::default();
    count.read(pin_batch(1, &[("first", Some((100, true))), ("second", Some((300, false)))]));
    assert_eq!(pins_bar(&mut count), bar(100, 400));

    count.read(pin_batch(1, &[("first", None), ("second", Some((300, false))), ("third", None)]));
    assert_eq!(pins_bar(&mut count), bar(100, 400));
}

/// An upload's bar counts each release's stored files and, of each file
/// being sent, the share of its size its object has moved; a file the
/// watch hasn't named yet has moved nothing. A file whose transfer ends
/// before the read counts it stored keeps the bytes it had reached.
#[test]
fn an_upload_bar_counts_stored_files_and_in_flight_shares() {
    let (a, b, c) = (file(1), file(2), file(3));
    let uploads = |bytes_stored: u64, sending: &[(coven::FileId, u64)]| ActivityBytes {
        uploads: Some(BatchReleases {
            number: 1,
            releases: [
                (
                    "first".to_string(),
                    Some(UploadRelease {
                        bytes_total: 1_000,
                        bytes_stored,
                        sending: sending.iter().copied().collect(),
                    }),
                ),
                (
                    "second".to_string(),
                    Some(UploadRelease {
                        bytes_total: 500,
                        bytes_stored: 0,
                        sending: [(c, 500)].into(),
                    }),
                ),
            ]
            .into(),
        }),
        pins: None,
        making_local: None,
    };
    let moved =
        |bytes_transferred, bytes_total| coven::TransferProgress { bytes_transferred, bytes_total };
    let mut count = BarsCount::default();
    count.read(uploads(400, &[(a, 200), (b, 100)]));
    count.uploads_heard(vec![coven::TransferChange::Reset(
        [(a, moved(55, 220)), (b, moved(110, 110))].into(),
    )]);
    assert_eq!(count.bars().unwrap().uploads, bar(400 + 50 + 100, 1_500));

    count.uploads_heard(vec![coven::TransferChange::Removed {
        file: b,
        end: coven::TransferEnd::Finished,
    }]);
    assert_eq!(count.bars().unwrap().uploads, bar(550, 1_500), "the bytes hold");
    count.read(uploads(500, &[(a, 200)]));
    assert_eq!(count.bars().unwrap().uploads, bar(500 + 50, 1_500));
}

/// A batch numbered anew starts from nothing, whether or not the read saw
/// the line empty in between: a release cancelled half down and queued
/// again in the next batch has downloaded none of it there.
#[test]
fn a_new_batch_starts_from_zero() {
    let mut count = BarsCount::default();
    count.read(pin_batch(1, &[("first", Some((100, false)))]));
    count.downloads_heard(pinning("first", 1, 2));
    assert_eq!(pins_bar(&mut count), bar(50, 100));
    count.downloads_heard(ended("first"));
    assert_eq!(pins_bar(&mut count), bar(50, 100), "cancelled: the batch keeps its bytes");

    count.read(pin_batch(2, &[("first", Some((100, false)))]));
    assert_eq!(pins_bar(&mut count), bar(0, 100));

    count.read(ActivityBytes { uploads: None, pins: None, making_local: None });
    assert_eq!(pins_bar(&mut count), None, "no batch, no bar");
}
