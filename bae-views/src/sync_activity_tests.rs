use super::*;

/// How coven's sync state names a cause: the state it shows.
type Names = fn(coven::SyncCause) -> coven::SyncState;
use bae_transfers::{QueueWait, ReleaseQueueProgress};

fn batch(done: u32, total: u32) -> TransferBatch {
    TransferBatch { done, total }
}

/// An empty, running upload queue.
fn idle_uploads() -> UploadQueueCounts {
    UploadQueueCounts { batch: batch(0, 0), sending: 0, wait: None }
}

/// An empty, running download queue.
fn idle_downloads() -> DownloadQueueCounts {
    DownloadQueueCounts {
        pins: batch(0, 0),
        moving_to_this_device: batch(0, 0),
        queue: ReleaseQueueCounts::default(),
    }
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

/// Nothing moving, no line and nothing to fix: the indicator is absent.
#[test]
fn a_settled_library_is_quiet() {
    let activity = SyncActivity::new(&status(), &idle_uploads(), &idle_downloads());
    assert!(!activity.moving);
    assert!(!activity.held());
    assert!(!activity.needs_attention());
    assert_eq!(
        (activity.uploads, activity.pins, activity.moving_to_this_device),
        (None, None, None)
    );
}

/// A pass that runs while nothing transfers doesn't spin the indicator; the
/// panel still says sync is running.
#[test]
fn a_pass_alone_is_not_movement() {
    let mut syncing = status();
    syncing.coven.state = coven::SyncState::Syncing(coven::SyncActivity::Receiving);
    let activity = SyncActivity::new(&syncing, &idle_uploads(), &idle_downloads());
    assert!(activity.syncing);
    assert!(!activity.moving);
}

/// A file being sent moves bytes whether or not it belongs to a release: a
/// cover being sent spins the indicator with no Uploading line, and a
/// release's file does with its line.
#[test]
fn a_file_being_sent_moves_with_or_without_a_line() {
    let cover = UploadQueueCounts { batch: batch(0, 0), sending: 1, wait: None };
    let sending_cover = SyncActivity::new(&status(), &cover, &idle_downloads());
    assert!(sending_cover.moving);
    assert_eq!(sending_cover.uploads, None);

    let release = UploadQueueCounts { batch: batch(1, 2), sending: 1, wait: None };
    let sending_release = SyncActivity::new(&status(), &release, &idle_downloads());
    assert!(sending_release.moving);
    assert_eq!(
        sending_release.uploads,
        Some(UploadQueueActivity { batch: batch(1, 2), wait: None })
    );
}

/// Only an entry downloading moves bytes; a pin batch that only waits shows
/// its line and leaves the indicator still.
#[test]
fn a_download_moves_only_while_an_entry_downloads() {
    let mut downloads = DownloadQueueCounts { pins: batch(0, 2), ..idle_downloads() };
    let waiting = SyncActivity::new(&status(), &idle_uploads(), &downloads);
    assert!(!waiting.moving);
    assert_eq!(waiting.pins, Some(DownloadQueueActivity { batch: batch(0, 2), wait: None }));
    downloads.queue.total = ReleaseQueueProgress { queued: 1, active: 1, failed: 0 };
    assert!(SyncActivity::new(&status(), &idle_uploads(), &downloads).moving);
}

/// Pins and moves-to-this-device share the download queue but are lines of their
/// own, each from its own batch; a kind whose batch holds nothing has no
/// line.
#[test]
fn pins_and_moves_to_this_device_are_lines_of_their_own() {
    let downloads = DownloadQueueCounts {
        pins: batch(0, 0),
        moving_to_this_device: batch(2, 3),
        ..idle_downloads()
    };
    let activity = SyncActivity::new(&status(), &idle_uploads(), &downloads);
    assert_eq!(activity.pins, None);
    assert_eq!(
        activity.moving_to_this_device,
        Some(DownloadQueueActivity { batch: batch(2, 3), wait: None })
    );
}

/// A held queue keeps its lines and their counts, and each line carries what
/// holds it: a paused upload queue, and an offline download queue's hold on
/// both its lines.
#[test]
fn a_held_line_keeps_its_count() {
    let uploads = UploadQueueCounts {
        batch: batch(1, 3),
        sending: 0,
        wait: Some(bae_records::UploadQueueWait::Paused),
    };
    let downloads = DownloadQueueCounts {
        pins: batch(0, 2),
        moving_to_this_device: batch(1, 1),
        queue: ReleaseQueueCounts {
            total: ReleaseQueueProgress { queued: 2, active: 0, failed: 0 },
            wait: Some(QueueWait::Offline),
        },
    };
    let activity = SyncActivity::new(&status(), &uploads, &downloads);
    assert!(!activity.moving);
    assert_eq!(
        activity.uploads,
        Some(UploadQueueActivity {
            batch: batch(1, 3),
            wait: Some(bae_records::UploadQueueWait::Paused)
        })
    );
    assert_eq!(
        (activity.pins, activity.moving_to_this_device),
        (
            Some(DownloadQueueActivity { batch: batch(0, 2), wait: Some(QueueWait::Offline) }),
            Some(DownloadQueueActivity { batch: batch(1, 1), wait: Some(QueueWait::Offline) })
        )
    );
}

/// A line ends in Resume only while the person paused its queue: not while
/// the network, a metered network or a retry's time holds it, nor while
/// nothing does. Pins and moves-to-this-device share one queue, so both lines say so.
#[test]
fn a_line_is_paused_only_by_the_person() {
    use bae_records::UploadQueueWait;
    let uploads = |wait| {
        let uploads = UploadQueueCounts { batch: batch(1, 3), sending: 0, wait };
        SyncActivity::new(&status(), &uploads, &idle_downloads()).uploads.unwrap().paused()
    };
    assert!(uploads(Some(UploadQueueWait::Paused)));
    for wait in [
        None,
        Some(UploadQueueWait::Offline),
        Some(UploadQueueWait::Metered),
        Some(UploadQueueWait::Retrying { at_ms: 1_700_000_000_000 }),
    ] {
        assert!(!uploads(wait.clone()), "{wait:?}");
    }

    let downloads = |wait| {
        let downloads = DownloadQueueCounts {
            pins: batch(0, 2),
            moving_to_this_device: batch(0, 1),
            queue: ReleaseQueueCounts {
                total: ReleaseQueueProgress { queued: 2, active: 0, failed: 0 },
                wait,
            },
        };
        let activity = SyncActivity::new(&status(), &idle_uploads(), &downloads);
        (activity.pins.unwrap().paused(), activity.moving_to_this_device.unwrap().paused())
    };
    assert_eq!(downloads(Some(QueueWait::Paused)), (true, true));
    assert_eq!(downloads(Some(QueueWait::Offline)), (false, false));
    assert_eq!(downloads(None), (false, false));
}

/// The indicator shows the pause symbol while a line shows what holds its
/// queue, whatever holds it, and no bytes move: a paused, offline, metered or
/// retrying upload queue, and a paused or offline download queue on either of
/// its lines.
#[test]
fn a_held_line_with_nothing_moving_holds_the_indicator() {
    use bae_records::UploadQueueWait;
    let upload_waits = [
        UploadQueueWait::Paused,
        UploadQueueWait::Offline,
        UploadQueueWait::Metered,
        UploadQueueWait::Retrying { at_ms: 1_700_000_000_000 },
    ];
    for wait in upload_waits {
        let uploads = UploadQueueCounts { batch: batch(1, 3), sending: 0, wait: Some(wait) };
        let activity = SyncActivity::new(&status(), &uploads, &idle_downloads());
        assert!(activity.held(), "{:?}", activity.uploads);
    }
    for wait in [QueueWait::Paused, QueueWait::Offline] {
        let held_queue = ReleaseQueueCounts {
            total: ReleaseQueueProgress { queued: 1, active: 0, failed: 0 },
            wait: Some(wait),
        };
        let pins = DownloadQueueCounts {
            pins: batch(0, 1),
            queue: held_queue.clone(),
            ..idle_downloads()
        };
        assert!(SyncActivity::new(&status(), &idle_uploads(), &pins).held(), "pins {wait:?}");
        let moving_to_this_device = DownloadQueueCounts {
            moving_to_this_device: batch(0, 1),
            queue: held_queue,
            ..idle_downloads()
        };
        assert!(
            SyncActivity::new(&status(), &idle_uploads(), &moving_to_this_device).held(),
            "moving to this device {wait:?}"
        );
    }
}

/// Bytes moving anywhere show the spinner in the pause symbol's place: a
/// cover sent while release files are paused, or a pin downloading while
/// uploads are paused.
#[test]
fn bytes_moving_are_not_held() {
    let paused = Some(bae_records::UploadQueueWait::Paused);
    let sending_cover = UploadQueueCounts { batch: batch(0, 2), sending: 1, wait: paused.clone() };
    let activity = SyncActivity::new(&status(), &sending_cover, &idle_downloads());
    assert!(activity.moving);
    assert!(!activity.held());

    let uploads = UploadQueueCounts { batch: batch(0, 2), sending: 0, wait: paused };
    let downloading = DownloadQueueCounts {
        pins: batch(0, 1),
        queue: ReleaseQueueCounts {
            total: ReleaseQueueProgress { queued: 0, active: 1, failed: 0 },
            wait: None,
        },
        ..idle_downloads()
    };
    let activity = SyncActivity::new(&status(), &uploads, &downloading);
    assert!(activity.moving);
    assert!(!activity.held());
}

/// A queue holds nothing back with no line to say so: a paused queue whose
/// batch holds no release, or a line whose queue waits on nothing, leaves
/// the indicator absent.
#[test]
fn a_hold_without_its_line_or_a_line_without_a_hold_is_not_held() {
    let paused_empty = UploadQueueCounts {
        batch: batch(0, 0),
        sending: 0,
        wait: Some(bae_records::UploadQueueWait::Paused),
    };
    let downloads_paused_empty = DownloadQueueCounts {
        queue: ReleaseQueueCounts {
            wait: Some(QueueWait::Paused),
            ..ReleaseQueueCounts::default()
        },
        ..idle_downloads()
    };
    assert!(!SyncActivity::new(&status(), &paused_empty, &downloads_paused_empty).held());

    let waiting = UploadQueueCounts { batch: batch(0, 2), sending: 0, wait: None };
    let pins_waiting = DownloadQueueCounts { pins: batch(0, 1), ..idle_downloads() };
    assert!(!SyncActivity::new(&status(), &waiting, &pins_waiting).held());
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
    let new = |status: &SyncStatusSnapshot| activity(status);
    assert!(new(&failed).needs_attention());
    let mut waiting = status();
    waiting.pending = vec![PendingItem::before_open(coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::NoStorage,
    ))];
    assert!(new(&waiting).needs_attention());
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
    SyncActivity::new(status, &idle_uploads(), &idle_downloads())
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
    copied.coven.state = coven::SyncState::NeedsReset {
        reason: coven::ResetReason::Replaced,
        unsent: coven::UnsentWork { writes: 0, files: 0 },
    };
    copied.pending = pending(connection(coven::StorageFailure::Network));
    assert!(activity(&copied).failure.is_some());

    let mut unrecorded = status();
    unrecorded.coven.state =
        named(refused(coven::StorageFailure::Protocol), coven::SyncState::Failed);
    unrecorded.pending = pending(connection(coven::StorageFailure::Network));
    assert!(activity(&unrecorded).failure.is_some());
}

/// The activity as one read of `store` finds it.
async fn activity_now(store: &bae_store::Store) -> SyncActivity {
    let mut activity = crate::LibraryViews::new(store.clone()).subscribe_sync_activity();
    activity.next().await.expect("the query stays open").unwrap()
}

/// Run `test` on a library whose storage is set up and whose uploads are
/// paused, with a folder for the originals its files are staged from. The
/// store is closed before its runtime ends, as a store still syncing must be.
pub(crate) fn run_on_library_with_storage(
    test: impl AsyncFnOnce(&bae_store::Store, &std::path::Path) + Send + 'static,
) {
    bae_store::test_devices::on_device_runtime(async move || {
        let dir = tempfile::TempDir::new().unwrap();
        let sources = bae_store::TestSources::system();
        let storage = std::sync::Arc::new(
            coven::MemoryStorage::builder().clock(sources.clock().clone()).build().unwrap(),
        );
        let installation = coven::Coven::in_memory(dir.path().to_path_buf());
        let handle = bae_store::test_devices::create_first_store(
            crate::TEST_SCHEMA,
            &installation,
            &sources,
            storage,
        )
        .await;
        let store = sources.store(handle);
        store.set_uploads_paused(true).await.unwrap();
        let originals = tempfile::TempDir::new().unwrap();
        test(&store, originals.path()).await;
        store.close().await.expect("close the store");
    });
}

pub(crate) const FILE_EARLY: &str = "5b0c3f0e-2d7a-4c61-9a4e-1f2b3c4d5e61";
pub(crate) const FILE_LATE: &str = "6c1d4a1f-3e8b-4d72-8b5f-2a3c4d5e6f72";

/// The two releases of `storage_listing`, each with one file staged on this
/// device only.
pub(crate) async fn write_releases_with_files(
    store: &bae_store::Store,
    originals: &std::path::Path,
) {
    use bae_records::test_fixtures::storage_listing::*;
    write_library(&bae_records::test_fixtures::test_records(store)).await;
    insert_file(store, originals, (FILE_EARLY, RELEASE_EARLY, "early.flac", 3)).await;
    insert_file(store, originals, (FILE_LATE, RELEASE_LATE, "late.flac", 5)).await;
}

/// Move `release` to the cloud, queueing its files to upload.
pub(crate) async fn queue_upload(store: &bae_store::Store, release: &str) {
    let moved = bae_transfers::storage_moves::move_releases_to_cloud(store, vec![release.into()])
        .await
        .unwrap();
    assert_eq!(moved.moved, vec![(release.to_string(), 1)]);
}

/// The upload batch takes in each release as its first file is queued and
/// keeps every one until none has a file queued: a release whose upload was
/// cancelled, or that left the library, stays in the count while another
/// still has files queued, never counting done, and the batch empties once
/// the last release with files queued has none, cancelled or removed.
#[test]
fn the_upload_batch_holds_every_release_queued_until_none_is() {
    use bae_records::test_fixtures::storage_listing::{RELEASE_EARLY, RELEASE_LATE};
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        assert_eq!(activity_now(store).await.uploads, None, "nothing is queued to upload");

        let batch_now = async || activity_now(store).await.uploads.map(|uploads| uploads.batch);
        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;
        assert_eq!(batch_now().await, Some(batch(0, 2)));

        records.cancel_release_uploads(vec![RELEASE_EARLY.into()]).await.unwrap();
        assert_eq!(
            batch_now().await,
            Some(batch(0, 2)),
            "the cancelled release stays in the count while another has files queued"
        );
        records.cancel_release_uploads(vec![RELEASE_LATE.into()]).await.unwrap();
        assert_eq!(batch_now().await, None, "no release has files queued");

        queue_upload(store, RELEASE_EARLY).await;
        assert_eq!(batch_now().await, Some(batch(0, 1)), "a new batch starts from zero");
        queue_upload(store, RELEASE_LATE).await;
        bae_records::test_fixtures::delete_release(&records, RELEASE_EARLY).await;
        assert_eq!(
            batch_now().await,
            Some(batch(0, 2)),
            "the removed release stays in the count while another has files queued"
        );
        bae_records::test_fixtures::delete_release(&records, RELEASE_LATE).await;
        assert_eq!(batch_now().await, None, "the last release with files queued was removed");
    });
}

/// The Uploading line counts releases, a release done once its files are
/// stored: of two releases queued, the one whose file uploaded is done
/// while the other's waits for its missing original. A picture queued
/// alone belongs to no release and shows no line.
#[test]
fn uploading_counts_releases_and_no_cover() {
    use bae_records::test_fixtures::storage_listing::{RELEASE_EARLY, RELEASE_LATE};
    run_on_library_with_storage(async |store, originals| {
        let records = bae_records::test_fixtures::test_records(store);
        write_releases_with_files(store, originals).await;
        queue_upload(store, RELEASE_EARLY).await;
        queue_upload(store, RELEASE_LATE).await;
        std::fs::remove_file(originals.join(FILE_LATE)).unwrap();

        store.set_uploads_paused(false).await.unwrap();
        store.retry_uploads_now().await.unwrap();
        store.set_uploads_paused(true).await.unwrap();
        assert_eq!(
            activity_now(store).await.uploads.map(|uploads| uploads.batch),
            Some(batch(1, 2))
        );

        records.cancel_release_uploads(vec![RELEASE_LATE.into()]).await.unwrap();
        assert_eq!(activity_now(store).await.uploads, None);

        // Sync stopped, the picture's upload stays in the queue.
        bae_store::test_devices::stop_sync(store).await;
        bae_records::test_fixtures::library::RecordsFixture::new()
            .artist(bae_records::test_fixtures::ARTIST_A, "Artist Name A")
            .write(&records)
            .await;
        write_image(&records, bae_records::test_fixtures::ARTIST_A).await;
        let queued: i64 = store
            .read(|sql| {
                Ok::<_, bae_store::DbError>(sql.query_row(
                    "SELECT COUNT(*) FROM coven_upload_queue",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(queued, 1, "the picture waits in the queue");
        assert_eq!(activity_now(store).await.uploads, None);
    });
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
