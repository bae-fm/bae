use super::*;

/// How coven's sync state names a cause: the state it shows.
type Names = fn(coven::SyncCause) -> coven::SyncState;
use bae_transfers::ReleaseQueueCounts;
use bae_types::count_label::CountLabel;

/// An empty, running upload queue.
fn idle_uploads() -> UploadQueueCounts {
    UploadQueueCounts { queued: 0, sending: 0, paused: false }
}

fn status() -> SyncStatusSnapshot {
    SyncStatusSnapshot {
        coven: coven::SyncStatus { state: coven::SyncState::Synced, last_synced: None },
        pending: Vec::new(),
        retry_effect: coven::RetryEffect::Pass,
        storage_setup: None,
        error: None,
        provider: None,
    }
}

/// Nothing moving and nothing to fix: the indicator is absent.
#[test]
fn a_settled_library_is_quiet() {
    let activity = SyncActivity::new(&status(), &idle_uploads(), &ReleaseQueueCounts::default());
    assert!(!activity.moving());
    assert!(!activity.needs_attention());
}

/// A pass that runs while nothing transfers doesn't spin the indicator; the
/// list still says sync is running.
#[test]
fn a_pass_alone_is_not_movement() {
    let mut syncing = status();
    syncing.coven.state = coven::SyncState::Syncing;
    let activity = SyncActivity::new(&syncing, &idle_uploads(), &ReleaseQueueCounts::default());
    assert!(activity.syncing);
    assert!(!activity.moving());
}

/// Files being sent move; a paused queue's waiting files don't move.
#[test]
fn transfers_are_movement() {
    let mut uploads = UploadQueueCounts { queued: 6, sending: 2, paused: false };
    let uploading = SyncActivity::new(&status(), &uploads, &ReleaseQueueCounts::default());
    assert!(uploading.moving());
    assert_eq!(uploading.uploads, Some(UploadsInFlight { sending: 2, waiting: 4 }));
    uploads.paused = true;
    uploads.sending = 0;
    let paused = SyncActivity::new(&status(), &uploads, &ReleaseQueueCounts::default());
    assert_eq!(paused.uploads, None);
}

/// A sync failure or a pending record that needs the person marks the
/// indicator.
#[test]
fn failures_and_pending_records_need_attention() {
    let mut failed = status();
    failed.coven.state = named(
        coven::LastPass::Offline {
            failure: coven::StorageFailure::Network,
            error: "unreachable".into(),
        },
        coven::SyncState::Offline,
    );
    let new = |status: &SyncStatusSnapshot| {
        SyncActivity::new(status, &idle_uploads(), &ReleaseQueueCounts::default())
    };
    assert!(new(&failed).needs_attention());
    let mut waiting = status();
    waiting.pending = vec![PendingItem::before_open(coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::NoStorage,
    ))];
    assert!(new(&waiting).needs_attention());
}

/// The list's lines name what is sending or downloading, then what waits,
/// and drop a zero.
#[test]
fn transfer_lines_name_the_moving_count_first() {
    let label = |key: &str, count| CountLabel { key: key.into(), count };
    assert_eq!(
        UploadsInFlight { sending: 2, waiting: 4 }.summary_parts(),
        vec![label("core.queue.uploading", 2), label("core.queue.queued", 4)]
    );
    assert_eq!(
        DownloadsInFlight { active: 1, queued: 0 }.summary_parts(),
        vec![label("core.queue.downloading", 1)]
    );
}

/// The text coven keeps for an S3 failure the provider answered.
const ANSWERED_S3: &str = "S3 answered HTTP 503";

/// A pass storage refused, as the status keeps it.
fn refused(failure: coven::StorageFailure) -> coven::LastPass {
    coven::LastPass::Failed {
        failure: coven::StatusFailure::Storage(failure),
        error: ANSWERED_S3.into(),
    }
}

/// coven's sync state named by the last pass `pass`, as `state` names it.
fn named(pass: coven::LastPass, state: Names) -> coven::SyncState {
    state(coven::SyncCause::Pass(pass))
}

fn pending(record: coven::PendingRecord) -> Vec<PendingItem> {
    vec![PendingItem::before_open(record)]
}

fn connection(failure: coven::StorageFailure) -> coven::PendingRecord {
    coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            failure,
            answered: true,
        },
    )
}

fn activity(status: &SyncStatusSnapshot) -> SyncActivity {
    SyncActivity::new(status, &idle_uploads(), &ReleaseQueueCounts::default())
}

/// A pass that fails on the connection leaves coven's record of that
/// failure: the list shows the record, which names what failed and what the
/// person can do, and not the same failure again as a second line.
#[test]
fn a_failed_connection_shows_once_as_its_record() {
    let cases: [(coven::LastPass, Names); 2] = [
        (refused(coven::StorageFailure::Protocol), coven::SyncState::Failed),
        (
            coven::LastPass::Offline {
                failure: coven::StorageFailure::Protocol,
                error: ANSWERED_S3.into(),
            },
            coven::SyncState::Offline,
        ),
    ];
    for (failure, state) in cases {
        let mut failed = status();
        failed.coven.state = named(failure, state);
        failed.pending = pending(connection(coven::StorageFailure::Protocol));
        let shown = activity(&failed);
        assert_eq!(shown.failure, None, "{:?}", failed.coven);
        assert!(shown.needs_attention());
        assert_eq!(shown.attention.len(), 1);
    }
}

/// A blocked slot and coven's own failed operation each have their record
/// in the list, which shows the failure the status holds.
#[test]
fn a_blocked_slot_or_failed_operation_shows_once_as_its_record() {
    let reason = coven::PendingReason::Storage {
        provider: coven::CloudProvider::S3,
        failure: coven::StorageFailure::Protocol,
        answered: true,
    };
    let mut blocked = status();
    blocked.coven.state = named(
        coven::LastPass::Blocked {
            slot: coven::Slot::new(3).unwrap(),
            reason: reason.clone(),
            error: Some(ANSWERED_S3.into()),
        },
        coven::SyncState::Failed,
    );
    blocked.pending = pending(coven::PendingRecord::new(
        coven::PendingSubject::Slot(coven::Slot::new(3).unwrap()),
        reason.clone(),
    ));
    assert_eq!(activity(&blocked).failure, None);

    let operation = coven::PendingRecord::new(
        coven::PendingSubject::Operation {
            id: coven::OperationId(5),
            kind: coven::OperationKind::Invite,
        },
        reason,
    );
    let mut failed = status();
    failed.coven.state = named(
        coven::LastPass::Failed {
            failure: coven::StatusFailure::Operation(operation.clone()),
            error: "the invite failed".into(),
        },
        coven::SyncState::Failed,
    );
    failed.pending = pending(operation);
    assert_eq!(activity(&failed).failure, None);
}

/// A failure no record shows is the list's line: another copy of this
/// device, which coven records nothing for, beside an older connection
/// record; or a storage failure another failure's record stands for.
#[test]
fn a_failure_no_record_shows_is_its_own_line() {
    let mut copied = status();
    copied.coven.state = named(
        coven::LastPass::Failed {
            failure: coven::StatusFailure::DeviceIdInUse(coven::DeviceIdEvidence::Replaced),
            error: "another copy uses this device's id".into(),
        },
        coven::SyncState::NeedsReset,
    );
    copied.pending = pending(connection(coven::StorageFailure::Network));
    assert!(activity(&copied).failure.is_some());

    let mut unrecorded = status();
    unrecorded.coven.state =
        named(refused(coven::StorageFailure::Protocol), coven::SyncState::Failed);
    unrecorded.pending = pending(connection(coven::StorageFailure::Network));
    assert!(activity(&unrecorded).failure.is_some());
}

/// The list offers Retry now for a pass a server error failed while coven
/// would run a pass for the retry, and not while a held failure would
/// answer it at once; nor with nothing failing.
#[test]
fn the_list_offers_retry_only_when_a_retry_runs_a_pass_that_can_clear_a_failure() {
    let failed = |retry_effect| {
        let mut failed = status();
        failed.coven.state =
            named(refused(coven::StorageFailure::Network), coven::SyncState::Failed);
        failed.pending = pending(connection(coven::StorageFailure::Network));
        failed.retry_effect = retry_effect;
        activity(&failed).retry_offered
    };
    assert!(failed(coven::RetryEffect::Pass));
    assert!(!failed(coven::RetryEffect::Failed));
    assert!(!activity(&status()).retry_offered);
}

/// What the activity reads as queued, and what counting the queue gives.
async fn queued_counts(store: &bae_store::Store) -> (u32, u32) {
    store
        .read(|sql| {
            Ok::<_, bae_store::DbError>((
                upload_queue_counts_on(&sql)?.queued,
                sql.query_row("SELECT COUNT(*) FROM coven_upload_queue", [], |row| row.get(0))?,
            ))
        })
        .await
        .unwrap()
}

/// An artist image, which stages one file for the cloud.
async fn write_image(records: &bae_records::Records, artist: &str) {
    let image = bae_records::DbLibraryImage {
        id: artist.to_string(),
        image_type: bae_records::LibraryImageType::Artist,
        content_type: bae_util::content_type::ContentType::Jpeg,
    };
    records.write_library_image(&image, artist.as_bytes()).await.unwrap();
}

/// The queued count follows files joining the queue and leaving it, and is
/// what counting the queue gives: two files queue, and replacing one image
/// queues its new file and drops the old.
#[tokio::test]
async fn the_queued_count_follows_the_queue() {
    let (store, _tmp) = bae_store::test_fixtures::empty_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&store);
    let artists = bae_records::test_fixtures::library::RecordsFixture::new()
        .artist("a", "A")
        .artist("b", "B");
    artists.write(&records).await;
    assert_eq!(queued_counts(&store).await, (0, 0));
    write_image(&records, "a").await;
    assert_eq!(queued_counts(&store).await, (1, 1));
    write_image(&records, "b").await;
    assert_eq!(queued_counts(&store).await, (2, 2));
    write_image(&records, "a").await;
    assert_eq!(queued_counts(&store).await, (2, 2));
}
