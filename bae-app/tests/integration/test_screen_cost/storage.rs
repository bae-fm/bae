//! The Storage Manager's plain reads: its window under each filter and sort,
//! its count and total, its search, a selection's actions and the inspector of
//! one release. Each is the plain query over the domain tables: a release's
//! storage worked out from its files and coven's records of them each time,
//! no `release_listing` and no `release_storage`.

use super::seed::{sizes, SEEDS};
use bae_records::test_fixtures::shaped_library;
use bae_store::test_fixtures::screen_cost::{list_costs, plain_cost, size_name, ListShape};
use bae_store::Store;
use rusqlite::types::Value;

const LANGUAGE: &str = "(SELECT language FROM name_sort)";

/// Each release with what the manager sorts and filters it by, worked out
/// from its files, coven's records of them and the download queue.
const STORAGE: &str = "\
    (SELECT r.id AS release_id, r.album_id, r.created_at, \
        (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r.id) AS file_count, \
        (SELECT COALESCE(SUM(coven.size), 0) FROM release_files f \
            JOIN coven_files coven ON coven.id = f.file WHERE f.release_id = r.id) AS total_size, \
        (EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id) \
         AND NOT EXISTS (SELECT 1 FROM release_files f \
            LEFT JOIN coven_files coven ON coven.id = f.file \
            WHERE f.release_id = r.id AND coven.hash IS NULL)) AS stored, \
        (SELECT COUNT(*) FROM release_files f \
            JOIN coven_pins pin ON pin.file = f.file WHERE f.release_id = r.id) AS pinned_count, \
        (SELECT COUNT(*) FROM release_files f \
            JOIN coven_upload_queue queued ON queued.file = f.file \
            WHERE f.release_id = r.id) AS queued_count, \
        EXISTS (SELECT 1 FROM release_files f JOIN coven_moves moves ON moves.file = f.file \
            WHERE f.release_id = r.id AND moves.state = 'failed') AS move_failed, \
        EXISTS (SELECT 1 FROM download_queue dq \
            WHERE dq.release_id = r.id AND dq.state <> 'failed') AS download_queued, \
        EXISTS (SELECT 1 FROM download_queue dq \
            WHERE dq.release_id = r.id AND dq.state = 'failed') AS download_failed \
     FROM releases r) st";

/// Where a release sorts by storage: failed first, uploading, pinning,
/// local, pinned, in the cloud only.
const STORAGE_ORDER: &str = "CASE \
    WHEN st.download_failed = 1 OR st.move_failed = 1 THEN 0 \
    WHEN st.queued_count > 0 THEN 1 \
    WHEN st.download_queued = 1 THEN 2 \
    WHEN st.stored = 0 THEN 3 \
    WHEN st.file_count > 0 AND st.pinned_count = st.file_count THEN 4 \
    ELSE 5 END";

fn source(filter: &str, search: &str) -> String {
    format!("FROM {STORAGE} JOIN albums a ON a.id = st.album_id WHERE {filter}{search}")
}

/// The window, count and total of each filter and sort.
async fn windows(db: &Store, size: u64) {
    let title = format!("hex(bae_name_key(a.title, {LANGUAGE}))");
    let tie = format!("{title} || st.created_at || st.release_id");
    let title_key = format!("bae_name_key(a.title, {LANGUAGE})");
    let columns = "st.release_id, a.title, st.file_count, st.total_size, st.stored, \
                   st.queued_count, st.pinned_count";
    for (filter, condition) in [("all", "1"), ("local", "st.stored = 0")] {
        let source = source(condition, "");
        for (sort, key, descending) in [
            ("title", title_key.as_str(), false),
            ("size", "st.total_size", true),
            ("storage", STORAGE_ORDER, false),
            ("added", "st.created_at", true),
        ] {
            // The order ends with the title and the release, as the list's
            // does, whatever it sorts by first.
            list_costs(
                db,
                size,
                &ListShape {
                    metric: &format!("storage/{filter}/{sort}"),
                    select: columns,
                    source: &source,
                    key,
                    tie: &tie,
                    descending,
                },
            )
            .await;
        }
    }
    let at = size_name(size);
    let totals = format!(
        "SELECT st.stored, COUNT(*), COALESCE(SUM(st.total_size), 0) FROM {STORAGE} GROUP BY st.stored"
    );
    plain_cost(db, &format!("storage/totals@{at}"), Some(50.0), &totals, vec![]).await;
}

/// The search over the releases' albums, broad (every title) and rare (one
/// album's whole title).
async fn search(db: &Store, size: u64) {
    let at = size_name(size);
    let title_key = format!("bae_name_key(a.title, {LANGUAGE})");
    let tie = format!("hex({title_key}) || st.created_at || st.release_id");
    let one_title: String = db
        .read(|read| {
            read.query_row("SELECT title FROM albums ORDER BY id LIMIT 1 OFFSET 7", [], |row| {
                row.get(0)
            })
            .map_err(Into::into)
        })
        .await
        .unwrap();
    for (kind, text) in [("broad", "album".to_string()), ("rare", one_title)] {
        let source = source("1", " AND bae_fold_case(a.title) GLOB bae_search_pattern(?1)");
        let params = vec![Value::Text(text)];
        let count = plain_cost(
            db,
            &format!("storage/search-{kind}/count@{at}"),
            Some(50.0),
            &format!("SELECT COUNT(*), COALESCE(SUM(st.total_size), 0) {source}"),
            params.clone(),
        )
        .await;
        assert_eq!(count, 1);
        let rows = plain_cost(
            db,
            &format!("storage/search-{kind}/window@{at}"),
            Some(50.0),
            &format!(
                "SELECT st.release_id, a.title, st.file_count, st.total_size {source} \
                 ORDER BY {title_key}, {tie} LIMIT 50"
            ),
            params,
        )
        .await;
        assert!(rows > 0, "the {kind} search finds releases");
    }
}

/// The ids of `count` releases of the library, from its start.
async fn release_ids(db: &Store, count: usize) -> String {
    let ids: Vec<String> = db
        .read(move |read| {
            Ok(read.query(
                "SELECT id FROM releases ORDER BY id LIMIT ?1",
                [count as i64],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    serde_json::to_string(&ids).unwrap()
}

/// What a selection of releases offers: how many are selected and what holds
/// of every one of them, from their storage worked out plainly.
async fn selection(db: &Store, size: u64) {
    let at = size_name(size);
    for count in [1, 50, 5_000] {
        let ids = release_ids(db, count).await;
        let sql = format!(
            "SELECT COUNT(*), MIN(st.stored = 0), MIN(st.stored = 1), \
                    MAX(st.download_queued OR st.move_failed), MIN(st.pinned_count = st.file_count) \
             FROM {STORAGE} WHERE st.release_id IN (SELECT value FROM json_each(?1))"
        );
        let rows = plain_cost(
            db,
            &format!("storage/selection-{count}@{at}"),
            Some(50.0),
            &sql,
            vec![Value::Text(ids)],
        )
        .await;
        assert_eq!(rows, 1);
    }
}

/// The inspector of a release: each file with its pin, its upload and its
/// move, for a release of 14 files and one of 602.
async fn inspector(db: &Store, size: u64) {
    let at = size_name(size);
    for (name, album) in [("12-files", 7), ("600-files", 250)] {
        let sql = "SELECT f.id, f.original_filename, f.content_type, coven.size, \
                          pin.file IS NOT NULL AS pinned, queued.position, moves.state \
                   FROM release_files f \
                   JOIN coven_files coven ON coven.id = f.file \
                   LEFT JOIN coven_pins pin ON pin.file = f.file \
                   LEFT JOIN coven_upload_queue queued ON queued.file = f.file \
                   LEFT JOIN coven_moves moves ON moves.file = f.file \
                   WHERE f.release_id = ?1 ORDER BY f.original_filename COLLATE natural_order, f.id";
        let rows = plain_cost(
            db,
            &format!("storage/inspector-{name}@{at}"),
            Some(50.0),
            sql,
            vec![Value::Text(shaped_library::release_id(album, 0))],
        )
        .await;
        assert!(rows >= 12, "the inspector's release has its files");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn storage_plain_reads() {
    for size in sizes() {
        let (db, _copy) = SEEDS.seeded_copy(size).await;
        windows(&db, size).await;
        search(&db, size).await;
        selection(&db, size).await;
        inspector(&db, size).await;
    }
}
