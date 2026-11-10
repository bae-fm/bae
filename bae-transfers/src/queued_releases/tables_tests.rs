//! The transfer queues' tables: what an entry is, what one move does to it,
//! what opening the library leaves, and how the reads are planned.

use crate::download_snapshot::DownloadRequest;
use crate::output_snapshot::{OutputKind, OutputRequest};
use crate::queued_releases::tables::{self, QueuedPayload};
use crate::release_queue::{ReleaseQueueEntry, ReleaseQueueState};
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::*;
use bae_store::*;

async fn enqueue_releases<P: QueuedPayload>(
    db: &Store,
    entries: Vec<ReleaseQueueEntry<P>>,
) -> Result<bool, DbError> {
    db.call(move |sql| tables::enqueue_releases_on(sql, &entries)).await
}

async fn next_queued_release<P: QueuedPayload>(
    db: &Store,
) -> Result<Option<ReleaseQueueEntry<P>>, DbError> {
    db.read(|sql| tables::next_queued_release_on::<P>(&sql)).await
}

async fn activate_queued_release<P: QueuedPayload>(
    db: &Store,
    release_id: &str,
) -> Result<bool, DbError> {
    let release_id = release_id.to_string();
    db.call(move |sql| tables::activate_queued_release_on::<P>(sql, &release_id)).await
}

async fn finish_queued_release<P: QueuedPayload>(
    db: &Store,
    release_id: &str,
) -> Result<bool, DbError> {
    let release_id = release_id.to_string();
    db.call(move |sql| tables::finish_queued_release_on::<P>(sql, &release_id)).await
}

async fn fail_queued_release<P: QueuedPayload>(
    db: &Store,
    release_id: &str,
    error: String,
) -> Result<bool, DbError> {
    let release_id = release_id.to_string();
    db.call(move |sql| tables::fail_queued_release_on::<P>(sql, &release_id, &error)).await
}

async fn retry_failed_releases<P: QueuedPayload>(db: &Store) -> Result<bool, DbError> {
    db.call(|sql| tables::retry_failed_releases_on::<P>(sql)).await
}

async fn retry_failed_release<P: QueuedPayload>(
    db: &Store,
    release_id: &str,
) -> Result<bool, DbError> {
    let release_id = release_id.to_string();
    db.call(move |sql| tables::retry_failed_release_on::<P>(sql, &release_id)).await
}

async fn set_release_queue_paused<P: QueuedPayload>(
    db: &Store,
    paused: bool,
) -> Result<bool, DbError> {
    db.call(move |sql| tables::set_release_queue_paused_on::<P>(sql, paused)).await
}

async fn cancel_queued_release<P: QueuedPayload>(
    db: &Store,
    release_id: &str,
) -> Result<bool, DbError> {
    let release_id = release_id.to_string();
    db.call(move |sql| tables::cancel_queued_release_on::<P>(sql, &release_id)).await
}

async fn release_queue<P: QueuedPayload>(
    db: &Store,
) -> Result<crate::release_queue::ReleaseQueueSnapshot<P, ()>, DbError> {
    db.read(|sql| tables::release_queue_on::<P>(&sql)).await
}

async fn clear_transfers(db: &Store) -> Result<(), DbError> {
    db.call(tables::clear_transfers_on).await
}

fn download(release_id: &str) -> ReleaseQueueEntry<DownloadRequest> {
    ReleaseQueueEntry {
        release_id: release_id.to_string(),
        created_at: 7,
        payload: DownloadRequest::Pin,
    }
}

/// The download queue's entries as the Downloads pane lists them: each
/// release and its phase, in the order queued.
async fn downloads(db: &Store) -> Vec<(String, ReleaseQueueState<()>)> {
    release_queue::<DownloadRequest>(db)
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|row| (row.op.release_id, row.op.state))
        .collect()
}

/// An entry is a row of its queue's table, kept in the order queued; a
/// release already queued is not queued again.
#[tokio::test]
async fn a_queued_release_is_a_row_in_the_order_queued() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    assert!(enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B)]).await.unwrap());
    assert!(!enqueue_releases(&db, vec![download(RELEASE_A)]).await.unwrap());

    let rows: Vec<String> = db
        .call(|sql| {
            Ok(sql.query("SELECT release_id FROM download_queue ORDER BY position", [], |row| {
                row.get(0)
            })?)
        })
        .await
        .unwrap();
    assert_eq!(rows, vec![RELEASE_A.to_string(), RELEASE_B.to_string()]);
    assert_eq!(
        next_queued_release::<DownloadRequest>(&db).await.unwrap(),
        Some(download(RELEASE_A))
    );
}

/// Retrying one release queues only its failed entry; the other failed
/// release stays failed, and a release with no failed entry changes nothing.
#[tokio::test]
async fn retrying_one_release_leaves_the_other_failed() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B)]).await.unwrap();
    for release in [RELEASE_A, RELEASE_B] {
        assert!(fail_queued_release::<DownloadRequest>(&db, release, "refused".to_string())
            .await
            .unwrap());
    }

    assert!(retry_failed_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());
    assert!(
        !retry_failed_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap(),
        "a queued entry has not failed"
    );
    assert_eq!(
        downloads(&db).await,
        vec![
            (RELEASE_A.to_string(), ReleaseQueueState::Queued),
            (RELEASE_B.to_string(), ReleaseQueueState::Failed { error: "refused".to_string() }),
        ]
    );
}

/// The worker's moves: the first queued entry starts, and an active one
/// before any queued; a
/// failure stays listed with its error until retried; a finished entry
/// leaves. A move on an entry no longer there says so.
#[tokio::test]
async fn an_entry_starts_fails_retries_and_finishes() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B)]).await.unwrap();

    assert!(activate_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());
    assert!(
        activate_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap(),
        "an active entry starts again, as it does after a restart"
    );
    assert_eq!(
        next_queued_release::<DownloadRequest>(&db).await.unwrap(),
        Some(download(RELEASE_A)),
        "the active entry is taken before the queued one"
    );
    assert!(fail_queued_release::<DownloadRequest>(
        &db,
        RELEASE_A,
        "the cloud refused".to_string()
    )
    .await
    .unwrap());
    assert_eq!(
        downloads(&db).await,
        vec![
            (
                RELEASE_A.to_string(),
                ReleaseQueueState::Failed { error: "the cloud refused".to_string() }
            ),
            (RELEASE_B.to_string(), ReleaseQueueState::Queued),
        ]
    );

    assert!(retry_failed_releases::<DownloadRequest>(&db).await.unwrap());
    assert!(!retry_failed_releases::<DownloadRequest>(&db).await.unwrap(), "none failed");
    assert!(activate_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());
    assert!(finish_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());
    assert!(
        !finish_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap(),
        "a finished entry is gone"
    );
    assert!(
        !activate_queued_release::<DownloadRequest>(&db, RELEASE_Z).await.unwrap(),
        "a release never queued does not start"
    );
    assert_eq!(downloads(&db).await, vec![(RELEASE_B.to_string(), ReleaseQueueState::Queued)]);
}

/// The schema holds one active entry at most.
#[tokio::test]
async fn a_queue_holds_one_active_entry_at_most() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B)]).await.unwrap();
    activate_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap();
    let second = db
        .call(|sql| {
            Ok(sql.execute(
                "UPDATE download_queue SET state = 'active' WHERE release_id = ?",
                [RELEASE_B],
            )?)
        })
        .await;
    assert!(second.is_err(), "a second active entry is refused");
}

/// A paused queue offers nothing to start and keeps its entries; resuming
/// offers the first again.
#[tokio::test]
async fn a_paused_queue_offers_nothing_to_start() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    enqueue_releases(&db, vec![download(RELEASE_A)]).await.unwrap();
    assert!(!set_release_queue_paused::<DownloadRequest>(&db, true).await.unwrap());
    assert_eq!(next_queued_release::<DownloadRequest>(&db).await.unwrap(), None);
    let queue = release_queue::<DownloadRequest>(&db).await.unwrap();
    assert!(queue.paused);
    assert_eq!(queue.total.queued, 1);
    assert!(set_release_queue_paused::<DownloadRequest>(&db, false).await.unwrap());
    assert_eq!(
        next_queued_release::<DownloadRequest>(&db).await.unwrap(),
        Some(download(RELEASE_A))
    );
}

/// Cancelling takes an entry out in any phase.
#[tokio::test]
async fn cancelling_takes_entries_out() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B), download(RELEASE_Z)])
        .await
        .unwrap();
    activate_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap();
    assert!(cancel_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());
    assert!(!cancel_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());
    assert_eq!(
        downloads(&db).await,
        vec![
            (RELEASE_B.to_string(), ReleaseQueueState::Queued),
            (RELEASE_Z.to_string(), ReleaseQueueState::Queued),
        ]
    );
}

/// A row shows its release as the library holds it now; one whose release
/// left the library stays listed with none, to be cancelled.
#[tokio::test]
async fn an_entry_shows_its_release_or_none_once_it_is_gone() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    seed_artist_album_release(&db).await;
    let gone = "0f0b3c1e-5a07-4a8e-9a43-3d2d64d1e7a1";
    enqueue_releases(&db, vec![download(RELEASE_A), download(gone)]).await.unwrap();

    let queue = release_queue::<DownloadRequest>(&db).await.unwrap();
    let release = queue.rows[0].release.as_ref().expect("RELEASE_A is held");
    assert_eq!(release.file_count, 0);
    assert_eq!(queue.rows[1].op.release_id, gone);
    assert_eq!(queue.rows[1].release, None);
    assert_eq!(queue.total.queued, 2);
}

/// An export and a save keep what they were queued with, the save's preset
/// whole.
#[tokio::test]
async fn an_output_keeps_its_target_and_preset() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let preset = bae_preferences::Preferences::default().save_presets.remove(0);
    let save = ReleaseQueueEntry {
        release_id: RELEASE_A.to_string(),
        created_at: 9,
        payload: OutputRequest {
            target_dir: "/Users/ana/Music/Out".into(),
            kind: OutputKind::Save { preset: preset.clone() },
        },
    };
    let export = ReleaseQueueEntry {
        release_id: RELEASE_B.to_string(),
        created_at: 10,
        payload: OutputRequest {
            target_dir: "/Users/ana/Music/Copies".into(),
            kind: OutputKind::Export,
        },
    };
    enqueue_releases(&db, vec![save.clone(), export.clone()]).await.unwrap();
    assert_eq!(next_queued_release::<OutputRequest>(&db).await.unwrap(), Some(save.clone()));
    let queue = release_queue::<OutputRequest>(&db).await.unwrap();
    let payloads: Vec<OutputRequest> = queue.rows.into_iter().map(|row| row.op.payload).collect();
    assert_eq!(payloads, vec![save.payload, export.payload]);
    assert!(downloads(&db).await.is_empty(), "an output is not a download");
}

/// The library opens with the download queue as the process left it, entries
/// and pause, and the export and save queue empty and running: the download
/// that was running starts again after a restart, an export does not.
#[tokio::test]
async fn opening_keeps_the_download_queue_and_its_pause() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let export = ReleaseQueueEntry {
        release_id: RELEASE_B.to_string(),
        created_at: 8,
        payload: OutputRequest { target_dir: "/Users/ana/Music".into(), kind: OutputKind::Export },
    };
    enqueue_releases(&db, vec![download(RELEASE_A)]).await.unwrap();
    enqueue_releases(&db, vec![export]).await.unwrap();
    assert!(activate_queued_release::<DownloadRequest>(&db, RELEASE_A).await.unwrap());

    // A quit while the download ran.
    clear_transfers(&db).await.unwrap();
    assert_eq!(
        downloads(&db).await,
        vec![(RELEASE_A.to_string(), ReleaseQueueState::Active { progress: () })]
    );
    assert_eq!(
        next_queued_release::<DownloadRequest>(&db).await.unwrap(),
        Some(download(RELEASE_A))
    );
    assert!(release_queue::<OutputRequest>(&db).await.unwrap().rows.is_empty());

    // A quit while both queues were paused.
    set_release_queue_paused::<DownloadRequest>(&db, true).await.unwrap();
    set_release_queue_paused::<OutputRequest>(&db, true).await.unwrap();
    clear_transfers(&db).await.unwrap();
    assert!(release_queue::<DownloadRequest>(&db).await.unwrap().paused);
    assert_eq!(downloads(&db).await.len(), 1);
    assert_eq!(next_queued_release::<DownloadRequest>(&db).await.unwrap(), None, "paused");
    assert!(!release_queue::<OutputRequest>(&db).await.unwrap().paused);
    set_release_queue_paused::<DownloadRequest>(&db, false).await.unwrap();
    assert_eq!(
        next_queued_release::<DownloadRequest>(&db).await.unwrap(),
        Some(download(RELEASE_A))
    );
}

/// The worker's lookups use the table's keys, and the pane's read joins
/// each row to its release by key.
#[tokio::test]
async fn the_queue_reads_are_planned_on_their_keys() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let plan = |sql_text: String| {
        let db = db.clone();
        async move {
            db.call(move |sql| {
                Ok(sql.query(&format!("EXPLAIN QUERY PLAN {sql_text}"), [], |row| {
                    row.get::<_, String>(3)
                })?)
            })
            .await
            .unwrap()
            .join("\n")
        }
    };
    let by_release =
        plan("SELECT state FROM download_queue WHERE release_id = 'r'".to_string()).await;
    assert!(
        by_release.contains("USING INDEX sqlite_autoindex_download_queue_1 (release_id=?)"),
        "{by_release}"
    );
    let pane = plan(crate::queued_releases::tables::queue_rows_sql::<DownloadRequest>(
        None,
        "ORDER BY q.position LIMIT 100 OFFSET 0",
    ))
    .await;
    assert!(pane.contains("SEARCH r USING INDEX sqlite_autoindex_releases_1 (id=?)"), "{pane}");
    assert!(!pane.contains("TEMP B-TREE"), "{pane}");
}

/// A pane's window holds the entries it names in the order queued, and the
/// read says how many the queue holds; a window past the end holds none.
#[tokio::test]
async fn a_window_reads_its_entries_and_the_queue_length() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let releases: Vec<String> =
        (0..5).map(|i| bae_util::test_ids::test_uuid(&format!("window-release-{i}"))).collect();
    enqueue_releases(&db, releases.iter().map(|id| download(id)).collect()).await.unwrap();
    let mut live =
        crate::Downloads::new(db.clone()).subscribe_windows(bae_store::LibraryPageWindows::from([
            bae_store::LibraryPageWindow { offset: 1, limit: 2 },
            bae_store::LibraryPageWindow { offset: 9, limit: 2 },
        ]));
    let read = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(read.total_count, 5);
    let shown: Vec<Vec<String>> = read
        .windows
        .iter()
        .map(|window| window.rows.iter().map(|row| row.op.release_id.clone()).collect())
        .collect();
    assert_eq!(shown, vec![releases[1..3].to_vec(), Vec::<String>::new()]);
}

/// The download batch's rows, each a release and its kind, in kind order.
async fn download_batch(db: &Store) -> Vec<(String, String)> {
    db.read(|sql| {
        Ok(sql.query(
            "SELECT kind, release_id FROM download_batch ORDER BY kind, release_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    })
    .await
    .unwrap()
}

fn batched(kind: &str, release_id: &str) -> (String, String) {
    (kind.to_string(), release_id.to_string())
}

/// Each kind's batch holds every release queued as that kind until none of
/// that kind waits or downloads: a finished pin and a cancelled one stay in
/// the pin batch while another pin waits, and it empties once the last one
/// leaves, the make-local's batch staying as it was. A failure ends the batch
/// only when no entry of its kind waits, and a retry starts it again.
#[tokio::test]
async fn the_download_batch_holds_each_kind_until_it_has_none_waiting() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let make_local = ReleaseQueueEntry {
        release_id: RELEASE_Z.to_string(),
        created_at: 8,
        payload: DownloadRequest::MakeLocal { target_dir: "/Users/ana/Music".into() },
    };
    let third = "3c4c2e9a-7f0b-4b8e-8d2a-5d6a1f0e9b21";
    enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B), download(third)])
        .await
        .unwrap();
    enqueue_releases(&db, vec![make_local]).await.unwrap();
    let everything = vec![
        batched("make_local", RELEASE_Z),
        batched("pin", RELEASE_A),
        batched("pin", third),
        batched("pin", RELEASE_B),
    ];
    assert_eq!(download_batch(&db).await, everything);

    db.call(|sql| tables::worker_moves::activate_on::<DownloadRequest>(sql, RELEASE_A))
        .await
        .unwrap();
    assert!(db
        .call(|sql| tables::worker_moves::finish_on::<DownloadRequest>(sql, RELEASE_A))
        .await
        .unwrap());
    assert!(cancel_queued_release::<DownloadRequest>(&db, RELEASE_B).await.unwrap());
    assert_eq!(download_batch(&db).await, everything, "a pin still waits");

    assert!(fail_queued_release::<DownloadRequest>(&db, third, "refused".to_string())
        .await
        .unwrap());
    assert_eq!(
        download_batch(&db).await,
        vec![batched("make_local", RELEASE_Z)],
        "no pin waits or downloads: the pin batch is over"
    );

    assert!(retry_failed_release::<DownloadRequest>(&db, third).await.unwrap());
    assert_eq!(
        download_batch(&db).await,
        vec![batched("make_local", RELEASE_Z), batched("pin", third)],
        "the retried pin starts a new batch"
    );
    assert!(cancel_queued_release::<DownloadRequest>(&db, RELEASE_Z).await.unwrap());
    assert_eq!(download_batch(&db).await, vec![batched("pin", third)]);
}
