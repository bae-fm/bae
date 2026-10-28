//! The transfer queues' tables: what an entry is, what one move does to it,
//! what opening the library leaves, and how the reads are planned.

use super::super::*;
use super::*;
use crate::library::output_snapshot::{OutputKind, OutputRequest};
use crate::library::release_queue::{ReleaseQueueEntry, ReleaseQueueState};

fn download(release_id: &str) -> ReleaseQueueEntry<()> {
    ReleaseQueueEntry {
        release_id: release_id.to_string(),
        created_at: 7,
        payload: (),
    }
}

/// The download queue's entries as the Downloads pane lists them: each
/// release and its phase, in the order queued.
async fn downloads(db: &Database) -> Vec<(String, ReleaseQueueState<()>)> {
    db.release_queue::<()>()
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
    let (db, _tmp) = temp_db().await;
    assert!(db
        .enqueue_releases(vec![download(RELEASE_A), download(RELEASE_B)])
        .await
        .unwrap());
    assert!(!db
        .enqueue_releases(vec![download(RELEASE_A)])
        .await
        .unwrap());

    let rows: Vec<String> = db
        .call(|sql| {
            Ok(sql.query(
                "SELECT release_id FROM download_queue ORDER BY position",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(rows, vec![RELEASE_A.to_string(), RELEASE_B.to_string()]);
    assert_eq!(
        db.next_queued_release::<()>().await.unwrap(),
        Some(download(RELEASE_A))
    );
}

/// The worker's moves: the first queued entry starts, one at a time; a
/// failure stays listed with its error until retried; a finished entry
/// leaves. A move on an entry no longer there says so.
#[tokio::test]
async fn an_entry_starts_fails_retries_and_finishes() {
    let (db, _tmp) = temp_db().await;
    db.enqueue_releases(vec![download(RELEASE_A), download(RELEASE_B)])
        .await
        .unwrap();

    assert!(db.activate_queued_release::<()>(RELEASE_A).await.unwrap());
    assert!(
        !db.activate_queued_release::<()>(RELEASE_A).await.unwrap(),
        "an active entry does not start again"
    );
    assert_eq!(
        db.next_queued_release::<()>().await.unwrap(),
        Some(download(RELEASE_B))
    );
    assert!(db
        .fail_queued_release::<()>(RELEASE_A, "the cloud refused".to_string())
        .await
        .unwrap());
    assert_eq!(
        downloads(&db).await,
        vec![
            (
                RELEASE_A.to_string(),
                ReleaseQueueState::Failed {
                    error: "the cloud refused".to_string()
                }
            ),
            (RELEASE_B.to_string(), ReleaseQueueState::Queued),
        ]
    );

    assert!(db.retry_failed_releases::<()>().await.unwrap());
    assert!(
        !db.retry_failed_releases::<()>().await.unwrap(),
        "none failed"
    );
    assert!(db.activate_queued_release::<()>(RELEASE_A).await.unwrap());
    assert!(db.finish_queued_release::<()>(RELEASE_A).await.unwrap());
    assert!(
        !db.finish_queued_release::<()>(RELEASE_A).await.unwrap(),
        "a finished entry is gone"
    );
    assert!(
        !db.activate_queued_release::<()>(RELEASE_Z).await.unwrap(),
        "a release never queued does not start"
    );
    assert_eq!(
        downloads(&db).await,
        vec![(RELEASE_B.to_string(), ReleaseQueueState::Queued)]
    );
}

/// The schema holds one active entry at most.
#[tokio::test]
async fn a_queue_holds_one_active_entry_at_most() {
    let (db, _tmp) = temp_db().await;
    db.enqueue_releases(vec![download(RELEASE_A), download(RELEASE_B)])
        .await
        .unwrap();
    db.activate_queued_release::<()>(RELEASE_A).await.unwrap();
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
    let (db, _tmp) = temp_db().await;
    db.enqueue_releases(vec![download(RELEASE_A)])
        .await
        .unwrap();
    assert!(!db.set_release_queue_paused::<()>(true).await.unwrap());
    assert_eq!(db.next_queued_release::<()>().await.unwrap(), None);
    let queue = db.release_queue::<()>().await.unwrap();
    assert!(queue.paused);
    assert_eq!(queue.total.queued, 1);
    assert!(db.set_release_queue_paused::<()>(false).await.unwrap());
    assert_eq!(
        db.next_queued_release::<()>().await.unwrap(),
        Some(download(RELEASE_A))
    );
}

/// Cancelling takes an entry out in any phase.
#[tokio::test]
async fn cancelling_takes_entries_out() {
    let (db, _tmp) = temp_db().await;
    db.enqueue_releases(vec![
        download(RELEASE_A),
        download(RELEASE_B),
        download(RELEASE_Z),
    ])
    .await
    .unwrap();
    db.activate_queued_release::<()>(RELEASE_A).await.unwrap();
    assert!(db.cancel_queued_release::<()>(RELEASE_A).await.unwrap());
    assert!(!db.cancel_queued_release::<()>(RELEASE_A).await.unwrap());
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
    let (db, _tmp) = temp_db().await;
    db.call(|sql| {
        seed_artist_album_release(sql)?;
        Ok(())
    })
    .await
    .unwrap();
    let gone = "0f0b3c1e-5a07-4a8e-9a43-3d2d64d1e7a1";
    db.enqueue_releases(vec![download(RELEASE_A), download(gone)])
        .await
        .unwrap();

    let queue = db.release_queue::<()>().await.unwrap();
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
    let (db, _tmp) = temp_db().await;
    let preset = crate::config::Preferences::default().save_presets.remove(0);
    let save = ReleaseQueueEntry {
        release_id: RELEASE_A.to_string(),
        created_at: 9,
        payload: OutputRequest {
            target_dir: "/Users/ana/Music/Out".into(),
            kind: OutputKind::Save {
                preset: preset.clone(),
            },
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
    db.enqueue_releases(vec![save.clone(), export.clone()])
        .await
        .unwrap();
    assert_eq!(
        db.next_queued_release::<OutputRequest>().await.unwrap(),
        Some(save.clone())
    );
    let queue = db.release_queue::<OutputRequest>().await.unwrap();
    let payloads: Vec<OutputRequest> = queue.rows.into_iter().map(|row| row.op.payload).collect();
    assert_eq!(payloads, vec![save.payload, export.payload]);
    assert!(
        downloads(&db).await.is_empty(),
        "an output is not a download"
    );
}

/// The library opens with both queues empty and running: no transfer
/// outlives the process that ran it.
#[tokio::test]
async fn opening_leaves_the_queues_empty_and_running() {
    let (db, _tmp) = temp_db().await;
    db.enqueue_releases(vec![download(RELEASE_A)])
        .await
        .unwrap();
    db.set_release_queue_paused::<()>(true).await.unwrap();
    db.set_release_queue_paused::<OutputRequest>(true)
        .await
        .unwrap();

    db.clear_release_queues().await.unwrap();

    let queue = db.release_queue::<()>().await.unwrap();
    assert!(queue.rows.is_empty());
    assert!(!queue.paused);
    assert!(!db.release_queue::<OutputRequest>().await.unwrap().paused);
}

/// The worker's lookups use the table's keys, and the pane's read joins
/// each row to its release by key.
#[tokio::test]
async fn the_queue_reads_are_planned_on_their_keys() {
    let (db, _tmp) = temp_db().await;
    let plan = |sql_text: String| {
        let db = db.clone();
        async move {
            db.call(move |sql| {
                Ok(
                    sql.query(&format!("EXPLAIN QUERY PLAN {sql_text}"), [], |row| {
                        row.get::<_, String>(3)
                    })?,
                )
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
    let pane = plan(super::super::release_queues::queue_rows_sql::<()>(
        "ORDER BY q.position LIMIT 100 OFFSET 0",
    ))
    .await;
    assert!(
        pane.contains("SEARCH r USING INDEX sqlite_autoindex_releases_1 (id=?)"),
        "{pane}"
    );
    assert!(!pane.contains("TEMP B-TREE"), "{pane}");
}

/// A pane's window holds the entries it names in the order queued, and the
/// read says how many the queue holds; a window past the end holds none.
#[tokio::test]
async fn a_window_reads_its_entries_and_the_queue_length() {
    let (db, _tmp) = temp_db().await;
    let releases: Vec<String> = (0..5)
        .map(|i| bae_test_support::test_uuid(&format!("window-release-{i}")))
        .collect();
    db.enqueue_releases(releases.iter().map(|id| download(id)).collect())
        .await
        .unwrap();
    let mut live =
        db.subscribe_release_queue_windows::<()>(crate::library::LibraryPageWindows::from([
            crate::library::LibraryPageWindow {
                offset: 1,
                limit: 2,
            },
            crate::library::LibraryPageWindow {
                offset: 9,
                limit: 2,
            },
        ]));
    let read = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(read.total_count, 5);
    let shown: Vec<Vec<String>> = read
        .windows
        .iter()
        .map(|window| {
            window
                .rows
                .iter()
                .map(|row| row.op.release_id.clone())
                .collect()
        })
        .collect();
    assert_eq!(shown, vec![releases[1..3].to_vec(), Vec::<String>::new()]);
}

/// The inspector's read of one release holds its entry in each queue and
/// nothing of another release's, and reads no release the library does not
/// hold.
#[cfg(desktop)]
#[tokio::test]
async fn the_inspectors_read_holds_the_releases_own_entries() {
    let (db, _tmp) = temp_db().await;
    db.call(|sql| {
        seed_artist_album_release(sql)?;
        Ok(())
    })
    .await
    .unwrap();
    db.enqueue_releases(vec![download(RELEASE_A), download(RELEASE_B)])
        .await
        .unwrap();
    db.enqueue_releases(vec![ReleaseQueueEntry {
        release_id: RELEASE_B.to_string(),
        created_at: 3,
        payload: OutputRequest {
            target_dir: "/Users/ana/Music/Copies".into(),
            kind: OutputKind::Export,
        },
    }])
    .await
    .unwrap();

    let mut live = db.subscribe_storage_inspector(Some(RELEASE_B.to_string()));
    let read = live
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(read, None, "the library holds no RELEASE_B");

    live.requests()
        .set(Some(RELEASE_A.to_string()))
        .expect("the query is open");
    let read = loop {
        let event = live.next().await.expect("the query stays open");
        if event.request.as_deref() == Some(RELEASE_A) {
            break event.result.unwrap().expect("the library holds RELEASE_A");
        }
    };
    assert_eq!(
        read.entries
            .download
            .map(|row| row.op.release_id)
            .as_deref(),
        Some(RELEASE_A)
    );
    assert!(read.entries.output.is_none(), "only RELEASE_B is exporting");
}
