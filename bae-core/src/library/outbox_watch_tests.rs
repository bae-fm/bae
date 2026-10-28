use super::*;
use crate::db::DbSendingFile;

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

fn transfers(rates: Vec<(String, u64)>, total: UploadTransfer) -> OutboxTransfers {
    OutboxTransfers {
        rates: rates.into_iter().collect(),
        total,
    }
}

fn sending(file: coven::FileId, release_id: Option<&str>) -> DbSendingFile {
    DbSendingFile {
        file,
        release_id: release_id.map(str::to_string),
        bytes_confirmed: Some(10),
        bytes_total: Some(100),
    }
}

#[test]
fn a_reader_hears_each_rate_that_moved_and_each_that_left() {
    let first = "release-1".to_string();
    let second = "release-2".to_string();
    let told = transfers(vec![(first.clone(), 5), (second.clone(), 5)], total(30, 10));
    let next = transfers(vec![(first.clone(), 8)], total(40, 8));

    let changes = changes_between(&told, &next);

    assert_eq!(changes.len(), 3, "{changes:?}");
    assert!(changes.contains(&OutboxTransferChange::Removed { release_id: second }));
    assert!(changes.contains(&OutboxTransferChange::Updated {
        release_id: first,
        throughput_bps: 8,
    }));
    assert!(changes.contains(&OutboxTransferChange::TotalUpdated(total(40, 8))));
}

#[test]
fn transfers_that_did_not_move_say_nothing() {
    let told = transfers(vec![("release-1".to_string(), 5)], total(10, 5));

    assert!(changes_between(&told, &told.clone()).is_empty());
}

/// A release goes at its files' rates together, and the total at every
/// file's; an image no release owns counts only in the total. The total's bytes are the ones read for the files
/// being sent.
#[test]
fn a_release_goes_at_its_files_rates_together() {
    let (a, b, cover) = (file(1), file(2), file(3));
    let sending = DbSendingUploads {
        files: vec![
            sending(a, Some("release-1")),
            sending(b, Some("release-1")),
            sending(cover, None),
        ],
        bytes_confirmed: 30,
        bytes_total: 300,
        paused: false,
    };
    let rates = [(a, 10), (b, 20), (cover, 5)].into_iter().collect();

    let measured = OutboxTransfers::of(&sending, &rates);

    assert_eq!(
        measured.rates,
        [("release-1".to_string(), 30)].into_iter().collect()
    );
    assert_eq!(
        measured.total,
        UploadTransfer {
            bar: Some(UploadBar {
                bytes_done: 30,
                bytes_total: 300,
            }),
            throughput_bps: 35,
        }
    );
}

/// A file no rate was measured for moves its release at none yet, and with nothing being
/// sent there is no rate and no bar.
#[test]
fn a_file_with_no_measured_rate_goes_at_none() {
    let a = file(1);
    let one = DbSendingUploads {
        files: vec![sending(a, Some("release-1"))],
        bytes_confirmed: 0,
        bytes_total: 0,
        paused: false,
    };
    let measured = OutboxTransfers::of(&one, &HashMap::new());
    assert_eq!(measured.rates["release-1"], 0);
    assert_eq!(
        measured.total.bar,
        Some(UploadBar {
            bytes_done: 0,
            bytes_total: 0,
        }),
        "a file sent in one request is being sent with no bytes to count"
    );

    let none = DbSendingUploads {
        files: Vec::new(),
        bytes_confirmed: 0,
        bytes_total: 0,
        paused: false,
    };
    assert_eq!(
        OutboxTransfers::of(&none, &HashMap::new()),
        transfers(
            Vec::new(),
            UploadTransfer {
                bar: None,
                throughput_bps: 0,
            }
        )
    );
}
