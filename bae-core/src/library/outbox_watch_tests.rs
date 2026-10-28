use super::*;
use crate::db::{DbSendingFile, DbSendingRelease};

fn file(n: u128) -> coven::FileId {
    coven::FileId(uuid::Uuid::from_u128(n))
}

fn total(bytes_done: u64, throughput_bps: u64) -> UploadTransfer {
    UploadTransfer {
        bar: Some(UploadBar {
            bytes_done,
            bytes_total: 100,
        }),
        throughput_bps,
    }
}

fn release(bytes_done: u64, throughput_bps: u64) -> ReleaseUploadTransfer {
    ReleaseUploadTransfer {
        bar: UploadBar {
            bytes_done,
            bytes_total: 1_000,
        },
        throughput_bps,
    }
}

fn transfers(
    releases: Vec<(String, ReleaseUploadTransfer)>,
    total: UploadTransfer,
) -> OutboxTransfers {
    OutboxTransfers {
        releases: releases.into_iter().collect(),
        total,
    }
}

/// A file being sent: a release's of `file_size` bytes, or one no release
/// owns.
fn sending(file: coven::FileId, release: Option<(&str, i64)>) -> DbSendingFile {
    DbSendingFile {
        file,
        release_id: release.map(|(id, _)| id.to_string()),
        file_size: release.map(|(_, size)| size),
    }
}

fn moved(bytes_transferred: u64, bytes_total: u64) -> coven::TransferProgress {
    coven::TransferProgress {
        bytes_transferred,
        bytes_total,
    }
}

#[test]
fn a_reader_hears_each_release_that_moved_and_each_that_left() {
    let first = "release-1".to_string();
    let second = "release-2".to_string();
    let told = transfers(
        vec![
            (first.clone(), release(100, 5)),
            (second.clone(), release(100, 5)),
        ],
        total(30, 10),
    );
    let next = transfers(vec![(first.clone(), release(200, 8))], total(40, 8));

    let changes = changes_between(&told, &next);

    assert_eq!(changes.len(), 3, "{changes:?}");
    assert!(changes.contains(&OutboxTransferChange::Removed { release_id: second }));
    assert!(changes.contains(&OutboxTransferChange::Updated {
        release_id: first,
        transfer: release(200, 8),
    }));
    assert!(changes.contains(&OutboxTransferChange::TotalUpdated(total(40, 8))));
}

#[test]
fn transfers_that_did_not_move_say_nothing() {
    let told = transfers(
        vec![("release-1".to_string(), release(100, 5))],
        total(10, 5),
    );

    assert!(changes_between(&told, &told.clone()).is_empty());
}

/// A release's bar holds its stored bytes and, of each file being sent, the
/// share of its size its object has moved; it goes at its files' rates
/// together. An image no release owns counts only in the total, which holds
/// the bytes every file's object has moved, at every file's rate.
#[test]
fn a_release_holds_its_stored_bytes_and_its_files_shares() {
    let (a, b, cover) = (file(1), file(2), file(3));
    let sending = DbSendingUploads {
        files: vec![
            sending(a, Some(("release-1", 200))),
            sending(b, Some(("release-1", 100))),
            sending(cover, None),
        ],
        releases: vec![DbSendingRelease {
            release_id: "release-1".into(),
            bytes_stored: 400,
            bytes_total: 1_000,
        }],
        paused: false,
    };
    let rates = [(a, 10), (b, 20), (cover, 5)].into_iter().collect();
    let progress = [
        (a, moved(55, 220)),
        (b, moved(110, 110)),
        (cover, moved(5, 50)),
    ]
    .into_iter()
    .collect();

    let measured = OutboxTransfers::of(&sending, &rates, &progress);

    assert_eq!(
        measured.releases,
        [("release-1".to_string(), release(400 + 50 + 100, 30))]
            .into_iter()
            .collect()
    );
    assert_eq!(
        measured.total,
        UploadTransfer {
            bar: Some(UploadBar {
                bytes_done: 55 + 110 + 5,
                bytes_total: 220 + 110 + 50,
            }),
            throughput_bps: 35,
        }
    );
}

/// A file the watch and the rates don't name yet has moved nothing: its
/// release holds only what is stored, and the total is a bar of nothing.
/// With nothing being sent there is no release and no bar.
#[test]
fn a_file_not_yet_heard_has_moved_nothing() {
    let a = file(1);
    let one = DbSendingUploads {
        files: vec![sending(a, Some(("release-1", 200)))],
        releases: vec![DbSendingRelease {
            release_id: "release-1".into(),
            bytes_stored: 400,
            bytes_total: 1_000,
        }],
        paused: false,
    };
    let measured = OutboxTransfers::of(&one, &HashMap::new(), &BTreeMap::new());
    assert_eq!(measured.releases["release-1"], release(400, 0));
    assert_eq!(
        measured.total.bar,
        Some(UploadBar {
            bytes_done: 0,
            bytes_total: 0,
        }),
    );

    let none = DbSendingUploads {
        files: Vec::new(),
        releases: Vec::new(),
        paused: false,
    };
    assert_eq!(
        OutboxTransfers::of(&none, &HashMap::new(), &BTreeMap::new()),
        transfers(
            Vec::new(),
            UploadTransfer {
                bar: None,
                throughput_bps: 0,
            }
        )
    );
}
