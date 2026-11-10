use super::*;

/// A release transfer's summary keeps its order and leaves out every
/// count that is zero, so a release with nothing being sent reads only
/// what waits and what is done.
#[test]
fn a_transfer_summary_leaves_out_zero_counts() {
    let summary = transfer_summary([
        ("core.queue.uploading", 0, TransferStage::Moving),
        ("core.queue.queued_to_upload", 4, TransferStage::Waiting),
        ("core.queue.stored", 105, TransferStage::Done),
    ]);
    assert_eq!(
        summary
            .iter()
            .map(|count| (count.label.key.as_str(), count.label.count, count.stage))
            .collect::<Vec<_>>(),
        vec![
            ("core.queue.queued_to_upload", 4, TransferStage::Waiting),
            ("core.queue.stored", 105, TransferStage::Done),
        ]
    );
}

/// The summary parts read active-then-failed-then-queued, drop any zero, and
/// take the active verb the caller names — so every queue line shares this
/// one decision and differs only in that verb.
#[test]
fn summary_parts_order_drop_and_active_verb() {
    let progress = ReleaseQueueProgress { queued: 3, active: 2, failed: 0 };
    assert_eq!(
        progress.pin_summary_parts(),
        vec![
            CountLabel { key: "core.queue.pinning".to_string(), count: 2 },
            CountLabel { key: "core.queue.queued_to_pin".to_string(), count: 3 },
        ],
        "failed is zero and drops; the active verb is the caller's"
    );
    assert!(
        ReleaseQueueProgress::default()
            .summary_parts("core.queue.uploading", "core.queue.queued_to_upload")
            .is_empty(),
        "an idle queue has no parts"
    );
}

#[test]
fn a_pause_comes_before_the_network() {
    assert_eq!(QueueWait::of(true, Some(false)), Some(QueueWait::Paused));
    assert_eq!(QueueWait::of(false, Some(false)), Some(QueueWait::Offline));
    assert_eq!(QueueWait::of(false, Some(true)), None);
    assert_eq!(QueueWait::of(false, None), None, "a platform that cannot tell holds nothing back");
}

fn row(release_id: &str, state: ReleaseQueueState<()>) -> ReleaseQueueRow<(), ()> {
    ReleaseQueueRow {
        op: ReleaseQueueOp {
            release_id: release_id.to_string(),
            created_at: 0,
            payload: (),
            state,
        },
        release: None,
    }
}

/// The active row takes its progress from the map; the others keep their
/// phase. A map without the active release's progress gives nothing.
#[test]
fn the_active_row_takes_its_progress_from_the_map() {
    let snapshot = ReleaseQueueSnapshot {
        rows: vec![
            row("rel-a", ReleaseQueueState::Active { progress: () }),
            row("rel-b", ReleaseQueueState::Queued),
        ],
        total: ReleaseQueueProgress { queued: 1, active: 1, failed: 0 },
        paused: false,
    };
    let with =
        snapshot.clone().with_progress(&BTreeMap::from([("rel-a".to_string(), 42u8)])).unwrap();
    assert_eq!(with.rows[0].op.state, ReleaseQueueState::Active { progress: 42 });
    assert_eq!(with.rows[1].op.state, ReleaseQueueState::Queued);
    assert!(snapshot.with_progress::<u8>(&BTreeMap::new()).is_none());
}
