use super::*;
use std::collections::HashMap;

fn transfer(bytes_done: u64, throughput_bps: u64) -> UploadTransfer {
    UploadTransfer {
        bar: Some(crate::library::UploadBar {
            bytes_done,
            bytes_total: 100,
        }),
        throughput_bps,
    }
}

fn transfers(
    keyed: Vec<(UploadTransferKey, UploadTransfer)>,
    total: UploadTransfer,
) -> OutboxTransfers {
    OutboxTransfers {
        keyed: keyed.into_iter().collect::<HashMap<_, _>>(),
        total,
    }
}

#[test]
fn a_reader_hears_each_transfer_that_moved_and_each_that_left() {
    let first = UploadTransferKey::Release("release-1".into());
    let second = UploadTransferKey::Release("release-2".into());
    let told = transfers(
        vec![
            (first.clone(), transfer(10, 5)),
            (second.clone(), transfer(20, 5)),
        ],
        transfer(30, 10),
    );
    let next = transfers(vec![(first.clone(), transfer(40, 5))], transfer(40, 5));

    let changes = changes_between(&told, &next);

    assert_eq!(changes.len(), 3, "{changes:?}");
    assert!(changes.contains(&OutboxTransferChange::Removed { key: second }));
    assert!(changes.contains(&OutboxTransferChange::Updated {
        key: first,
        transfer: transfer(40, 5),
    }));
    assert!(changes.contains(&OutboxTransferChange::TotalUpdated(transfer(40, 5))));
}

#[test]
fn transfers_that_did_not_move_say_nothing() {
    let key = UploadTransferKey::Release("release-1".into());
    let told = transfers(vec![(key, transfer(10, 5))], transfer(10, 5));

    assert!(changes_between(&told, &told.clone()).is_empty());
}
