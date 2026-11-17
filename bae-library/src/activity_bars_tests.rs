use super::*;
use bae_transfers::{DownloadTransferProgress, MoveToThisDeviceTransferProgress};
use bae_views::{BatchBar, MovingShare};

fn file(n: u128) -> coven::FileId {
    coven::FileId(uuid::Uuid::from_u128(n))
}

/// A line's bar with nothing done and nothing transferring.
fn empty<Key>() -> BatchBar<Key> {
    BatchBar { done: 0.0, moving: Vec::new() }
}

/// Rows of three lines with nothing done and nothing transferring.
fn no_rows() -> ActivityBarRows {
    ActivityBarRows { uploads: empty(), pins: empty(), moving_to_this_device: empty() }
}

/// Rows whose Pinning line has `done` of its batch done and `moving`
/// downloading, each release with its share of the batch.
fn pin_rows(done: f64, moving: &[(&str, f64)]) -> ActivityBarRows {
    ActivityBarRows {
        pins: BatchBar {
            done,
            moving: moving
                .iter()
                .map(|(release_id, share)| MovingShare {
                    key: release_id.to_string(),
                    share: *share,
                })
                .collect(),
        },
        ..no_rows()
    }
}

/// `release`'s pin has downloaded `bytes_done` of its `bytes_total`.
fn pinning(release: &str, bytes_done: u64, bytes_total: u64) -> (String, DownloadTransfer) {
    (
        release.to_string(),
        DownloadTransfer::Pin(
            DownloadTransferProgress::new(release, bytes_done, bytes_total).unwrap(),
        ),
    )
}

/// A pin batch's bar is the share its rows count done plus, for the release
/// downloading, its share of the batch times how far its pin has got: a
/// quarter done and a release of half the batch half downloaded is half.
#[test]
fn a_pin_bar_adds_the_downloading_release_to_the_share_done() {
    let rows = pin_rows(0.25, &[("second", 0.5)]);
    let downloading = BTreeMap::from([pinning("second", 20, 40)]);

    let bars = ActivityBars::of(&rows, &BTreeMap::new(), &downloading);

    assert_eq!(bars.pins, 0.5);
}

/// A release the rows list as downloading, whose transfer has told nothing
/// yet, has downloaded none of its share; a transfer of the other kind
/// counts on the other line only.
#[test]
fn a_release_with_no_transfer_of_its_kind_adds_nothing() {
    let rows = pin_rows(0.25, &[("second", 0.5)]);
    assert_eq!(ActivityBars::of(&rows, &BTreeMap::new(), &BTreeMap::new()).pins, 0.25);

    let moving = BTreeMap::from([(
        "second".to_string(),
        DownloadTransfer::MoveToThisDevice(MoveToThisDeviceTransferProgress::begin(40)),
    )]);
    assert_eq!(ActivityBars::of(&rows, &BTreeMap::new(), &moving).pins, 0.25);
}

/// A transfer whose release the rows no longer list as downloading adds
/// nothing: the rows count a stored release in their share done.
#[test]
fn a_transfer_the_rows_do_not_list_adds_nothing() {
    let rows = pin_rows(1.0, &[]);
    let downloading = BTreeMap::from([pinning("second", 40, 40)]);

    assert_eq!(ActivityBars::of(&rows, &BTreeMap::new(), &downloading).pins, 1.0);
}

/// The Uploading line's bar adds, for each file bae is sending, the file's
/// share of the batch times how much of its object coven has sent.
#[test]
fn an_upload_bar_adds_each_file_being_sent() {
    let (a, b) = (file(1), file(2));
    let rows = ActivityBarRows {
        uploads: BatchBar {
            done: 0.5,
            moving: vec![MovingShare { key: a, share: 0.25 }, MovingShare { key: b, share: 0.25 }],
        },
        ..no_rows()
    };
    let sent =
        BTreeMap::from([(a, coven::TransferProgress { bytes_transferred: 50, bytes_total: 100 })]);

    let bars = ActivityBars::of(&rows, &sent, &BTreeMap::new());

    assert_eq!(bars.uploads, 0.5 + 0.25 * 0.5);
    assert_eq!(bars.pins, 0.0);
    assert_eq!(bars.moving_to_this_device, 0.0);
}
