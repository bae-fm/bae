//! The downloads list's reads: what a window, the length, a position and a
//! jump return, which writes rerun them, and how SQLite plans them.

use super::*;
use crate::download_snapshot::DownloadRequest;
use crate::queued_releases::tables::{self, worker_moves};
use crate::release_queue::{ReleaseQueueEntry, ReleaseQueueState};
use bae_store::test_fixtures::*;
use bae_store::Store;

/// `count` releases the library doesn't hold, queued to pin in order.
async fn queue_pins(db: &Store, count: usize) -> Vec<String> {
    let releases: Vec<String> =
        (0..count).map(|i| bae_util::test_ids::test_uuid(&format!("window-release-{i}"))).collect();
    queue(db, &releases).await;
    releases
}

async fn queue(db: &Store, releases: &[String]) {
    let entries: Vec<_> = releases
        .iter()
        .map(|release_id| ReleaseQueueEntry {
            release_id: release_id.clone(),
            created_at: 7,
            payload: DownloadRequest::Pin,
        })
        .collect();
    db.call(move |sql| tables::enqueue_releases_on(sql, &entries)).await.unwrap();
}

/// A window holds its limit of entries from its key on, in the order the
/// person queued them, each with its key; a separate query counts the list's
/// entries, `key_at` returns the key of the entry at a position, or of the
/// last one past the end, and a key's position is the number of entries
/// before it.
#[tokio::test]
async fn a_window_reads_its_entries_and_the_list_length_its_own() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let releases = queue_pins(&db, 5).await;
    let downloads = crate::Downloads::new(db.clone());
    let second = downloads.key_at(1).await.unwrap().expect("a row at 1");
    let fourth = downloads.key_at(3).await.unwrap().expect("a row at 3");
    let mut live = downloads.subscribe_window(ListWindow { from: Some(second.clone()), limit: 2 });
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    let shown: Vec<String> =
        read.rows.iter().map(|(row, _key)| row.op.release_id.clone()).collect();
    assert_eq!(shown, releases[1..3].to_vec());
    assert_eq!(read.rows[0].1, second);
    assert_ne!(read.rows[1].1, fourth, "the window ends before the fourth entry");
    let count = downloads.subscribe_row_count().next_inline().await.expect("open").unwrap();
    assert_eq!(count, 5);
    let before =
        downloads.subscribe_position(second).next_inline().await.expect("open").result.unwrap();
    assert_eq!(before, 1);
    let past_the_end = downloads.key_at(99).await.unwrap();
    assert_eq!(past_the_end, downloads.key_at(4).await.unwrap());
}

/// A list with no entries has no key at any position.
#[tokio::test]
async fn an_empty_list_has_no_key() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let downloads = crate::Downloads::new(db.clone());
    assert_eq!(downloads.key_at(0).await.unwrap(), None);
    assert_eq!(downloads.key_at(3).await.unwrap(), None);
}

/// An entry queued after a window's last row reruns the length and a window
/// that reads to the list's end, and not a window its limit cut short. An
/// entry past a full window that starts downloading reruns neither that
/// window nor the length.
#[tokio::test]
async fn a_write_after_a_full_window_does_not_rerun_it() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let releases = queue_pins(&db, 4).await;
    let downloads = crate::Downloads::new(db.clone());
    let second = downloads.key_at(1).await.unwrap().expect("a row at 1");
    let mut windows = [ListWindow::top(2), ListWindow { from: Some(second), limit: 50 }]
        .map(|window| downloads.subscribe_window(window));
    let mut length = downloads.subscribe_row_count();
    for window in &mut windows {
        window.next_inline().await.expect("the query stays open").result.unwrap();
    }
    length.next_inline().await.expect("open").unwrap();

    let later = bae_util::test_ids::test_uuid("window-release-later");
    queue(&db, std::slice::from_ref(&later)).await;
    assert!(!windows[0].is_marked_for_rerun());
    assert!(windows[1].is_marked_for_rerun());
    assert!(length.is_marked_for_rerun());
    windows[1].next_inline().await.expect("the query stays open").result.unwrap();
    length.next_inline().await.expect("open").unwrap();

    let started = releases[3].clone();
    db.call(move |sql| worker_moves::activate_on::<DownloadRequest>(sql, &started)).await.unwrap();
    assert!(!windows[0].is_marked_for_rerun());
    assert!(windows[1].is_marked_for_rerun());
    assert!(!length.is_marked_for_rerun());
}

/// The controls' read says whether the person paused the queue, beside the
/// phases of its entries.
#[tokio::test]
async fn the_counts_follow_the_pause() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let releases = queue_pins(&db, 2).await;
    let downloads = crate::Downloads::new(db.clone());
    let started = releases[0].clone();
    db.call(move |sql| worker_moves::activate_on::<DownloadRequest>(sql, &started)).await.unwrap();
    let running =
        db.read(|sql| crate::queued_releases::rows::download_counts_on(&sql)).await.unwrap();
    assert_eq!((running.total.queued, running.total.active), (1, 1));
    assert!(!running.paused());

    downloads.set_paused(true).await.unwrap();
    let paused =
        db.read(|sql| crate::queued_releases::rows::download_counts_on(&sql)).await.unwrap();
    assert_eq!((paused.total.queued, paused.total.active), (2, 0));
    assert!(paused.paused());
}

/// The window and the position search the queue's own key, the window joins
/// each entry to its release and its album by key, and neither they nor the
/// jump sort anything or build a table first.
#[tokio::test]
async fn the_list_reads_are_planned_on_the_queue_order() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let plan = |statement: &'static str, names: &'static [&'static str]| {
        let db = db.clone();
        async move {
            db.call(move |sql| {
                let value = 3_i64;
                let params: Vec<(&str, &dyn rusqlite::ToSql)> =
                    names.iter().map(|name| (*name, &value as &dyn rusqlite::ToSql)).collect();
                Ok(sql.query(
                    &format!("EXPLAIN QUERY PLAN {statement}"),
                    params.as_slice(),
                    |row| row.get::<_, String>(3),
                )?)
            })
            .await
            .unwrap()
            .join("\n")
        }
    };
    let window = plan(DownloadWindow::SQL, &[":from", ":limit"]).await;
    assert!(window.contains("USING INTEGER PRIMARY KEY (rowid>?)"), "{window}");
    assert!(window.contains("USING INDEX sqlite_autoindex_releases_1 (id=?)"), "{window}");
    assert!(window.contains("USING INDEX sqlite_autoindex_albums_1 (id=?)"), "{window}");
    let before = plan(DownloadsBefore::SQL, &[":position"]).await;
    assert!(before.contains("USING INTEGER PRIMARY KEY (rowid<?)"), "{before}");
    let jump = plan(DownloadPlaceAt::SQL, &[":index"]).await;
    for steps in [window, before, jump] {
        assert!(!steps.contains("TEMP B-TREE"), "{steps}");
        assert!(!steps.contains("MATERIALIZE"), "{steps}");
    }
}

/// A window reads the rows at or after its key whatever phase they are in.
#[tokio::test]
async fn a_window_shows_each_entry_in_its_phase() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let releases = queue_pins(&db, 2).await;
    let failed = releases[1].clone();
    db.call(move |sql| worker_moves::fail_on::<DownloadRequest>(sql, &failed, "refused"))
        .await
        .unwrap();
    let read = db.read(|sql| download_window_on(&sql, &ListWindow::top(10))).await.unwrap();
    let phases: Vec<_> = read.rows.into_iter().map(|(row, _key)| row.op.state).collect();
    assert_eq!(
        phases,
        vec![ReleaseQueueState::Queued, ReleaseQueueState::Failed { error: "refused".to_string() },]
    );
}
