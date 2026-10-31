//! What a library-sized play costs the playback queue tables: filling,
//! replacing and emptying a context lane of `TRACKS` tracks through bae's
//! own writes, at bae's page cache size, then the same statements in plain
//! SQLite on the same file: the fill in play order, as bae fills, and in
//! track order, with SQLite's default page cache and larger ones, and the
//! emptying, with and without a pre-update hook.
//! Ignored by default; run with
//! `scripts/cargo.sh test -p bae-core --features test-utils --lib -- playback_queue_bench --ignored --nocapture`.
//! Each line it prints is one measurement over `RUNS` runs: the fastest,
//! which other work on the machine slows least, the median and every run.

use super::super::*;
use super::*;
use crate::db::ContextFill;
use std::time::Instant;

/// The library's tracks, all on one release.
const TRACKS: i64 = 1_000_000;
const RUNS: usize = 5;

/// The rows a library play (`ContextFill::LibraryShuffled`) fills the lane
/// with: each track, its place in play order and in the source's order.
const LIBRARY_ROWS: &str = "SELECT track_id, unshuffled, \
            row_number() OVER (ORDER BY random()) AS position \
     FROM (SELECT t.id AS track_id, \
                  row_number() OVER (ORDER BY t.release_id, t.position, t.id) AS unshuffled \
           FROM tracks t)";

/// The statement a library play fills the lane with, into `{table}`: the
/// entries numbered and inserted in play order, so the table and its position
/// index grow at their end and only the track index takes its rows out of
/// order.
const LIBRARY_FILL: &str = "INSERT INTO {table} (entry, position, track_id, unshuffled) \
     SELECT position, position, track_id, unshuffled FROM ({rows}) ORDER BY position";

/// The same rows numbered and inserted in track order instead, so the table
/// and its track index grow at their end and the position index takes its
/// rows out of order.
const LIBRARY_FILL_IN_TRACK_ORDER: &str =
    "INSERT INTO {table} (entry, position, track_id, unshuffled) \
     SELECT row_number() OVER (ORDER BY track_id), position, track_id, unshuffled \
     FROM ({rows}) ORDER BY track_id";

/// `fill` with its rows and table named.
fn fill_into(fill: &str, table: &str) -> String {
    fill.replace("{rows}", LIBRARY_ROWS)
        .replace("{table}", table)
}

/// `playback_context`'s columns without its `track_id` index.
const UNINDEXED_LANE: &str = "CREATE TABLE bench_unindexed_lane \
     (entry INTEGER PRIMARY KEY, position INTEGER NOT NULL UNIQUE, track_id TEXT NOT NULL, \
      unshuffled INTEGER NOT NULL) STRICT;";

fn report(label: &str, mut times: Vec<f64>) {
    times.sort_by(f64::total_cmp);
    eprintln!(
        "{label}: fastest {:.3} s, median {:.3} s (runs {times:.3?})",
        times[0],
        times[times.len() / 2]
    );
}

fn seconds_since(start: Instant) -> f64 {
    start.elapsed().as_secs_f64()
}

/// The SQLite file holding the store's tables, below `dir`.
fn database_file(dir: &std::path::Path) -> std::path::PathBuf {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let mut head = [0u8; 16];
            let is_sqlite = std::fs::File::open(&path)
                .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut head))
                .is_ok()
                && &head == b"SQLite format 3\0";
            if !is_sqlite {
                continue;
            }
            let holds_queue: bool = rusqlite::Connection::open(&path)
                .unwrap()
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'playback_context')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            if holds_queue {
                found.push(path);
            }
        }
    }
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn plain_connection(path: &std::path::Path) -> rusqlite::Connection {
    let plain = rusqlite::Connection::open(path).unwrap();
    plain
        .busy_timeout(std::time::Duration::from_secs(60))
        .unwrap();
    plain
}

/// `RELEASE_A` holding `TRACKS` tracks with random ids, as a library's are.
/// The tracks go in through a plain connection with triggers off: through
/// coven every synced row's write is captured whole, which would take
/// minutes, and the queue tables read nothing of `tracks` but its rows.
async fn library_db() -> (Database, tempfile::TempDir, std::path::PathBuf) {
    let (db, tmp) = super::temp_db().await;
    db.call(move |conn| {
        super::seed_artist_album_release(conn)?;
        conn.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?1, ?2, 1, 'cd', 1, '2026-01-01T00:00:00Z')",
            params![crate::db::identity::release_medium_id(RELEASE_A, 1), RELEASE_A],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let path = database_file(tmp.path());
    let plain = plain_connection(&path);
    plain
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
            false,
        )
        .unwrap();
    plain
        .execute(
            &format!(
                "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < {TRACKS} - 1) \
                 INSERT INTO tracks \
                     (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
                 SELECT i, \
                        lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' \
                              || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(2)) \
                              || '-' || hex(randomblob(6))), \
                        ?1, 'Track Title ' || i, ?2, 1, i + 1, 1000, '2026-01-01T00:00:00Z' \
                 FROM n"
            ),
            params![RELEASE_A, crate::db::identity::release_medium_id(RELEASE_A, 1)],
        )
        .unwrap();
    (db, tmp, path)
}

/// A library play through bae's writes: into an empty lane, replacing a full
/// one, and the two writes that empty it.
async fn bae_writes(db: &Database, label: &str) {
    let tables = db.playback_queue_tables();
    let mut runs: [Vec<f64>; 4] = Default::default();
    for _ in 0..RUNS {
        let start = Instant::now();
        tables.fill(ContextFill::LibraryShuffled).await.unwrap();
        runs[0].push(seconds_since(start));
        let start = Instant::now();
        tables.fill(ContextFill::LibraryShuffled).await.unwrap();
        runs[1].push(seconds_since(start));
        let start = Instant::now();
        assert!(tables.clear_context().await.unwrap());
        runs[2].push(seconds_since(start));
        tables.fill(ContextFill::LibraryShuffled).await.unwrap();
        let start = Instant::now();
        db.settle_playback_queue(false).await.unwrap();
        runs[3].push(seconds_since(start));
    }
    let names = [
        "play into an empty lane",
        "play replacing a full lane",
        "clear playing from",
        "open without resuming",
    ];
    for (name, times) in names.into_iter().zip(runs) {
        report(&format!("bae, {label}: {name}"), times);
    }
}

/// Time `statement` on `plain`, each run in a transaction rolled back after
/// `setup`, with a pre-update hook that does nothing when `hooked`: the hook
/// alone makes SQLite delete a whole table row by row, as coven's does.
fn plain_statement(
    plain: &rusqlite::Connection,
    label: &str,
    setup: &str,
    statement: &str,
    hooked: bool,
) {
    let mut times = Vec::new();
    for _ in 0..RUNS {
        plain.execute_batch("BEGIN").unwrap();
        plain.execute_batch(setup).unwrap();
        if hooked {
            plain
                .preupdate_hook(Some(
                    |_: rusqlite::hooks::Action,
                     _: &str,
                     _: &str,
                     _: &rusqlite::hooks::PreUpdateCase| {},
                ))
                .unwrap();
        }
        let start = Instant::now();
        plain.execute(statement, []).unwrap();
        times.push(seconds_since(start));
        if hooked {
            plain
                .preupdate_hook(
                    None::<
                        fn(rusqlite::hooks::Action, &str, &str, &rusqlite::hooks::PreUpdateCase),
                    >,
                )
                .unwrap();
        }
        plain.execute_batch("ROLLBACK").unwrap();
    }
    let hook = if hooked {
        "with a pre-update hook"
    } else {
        "no hook"
    };
    report(&format!("plain SQLite, {label}, {hook}"), times);
}

/// Time `fill` into the empty lane on `plain` and its commit, as a play's
/// write commits, after `pragmas`; the lane is emptied again after each run
/// and the pragmas set back to SQLite's defaults.
fn plain_fill(plain: &rusqlite::Connection, label: &str, pragmas: &str, fill: &str) {
    plain.execute_batch(pragmas).unwrap();
    let mut times = Vec::new();
    for _ in 0..RUNS {
        let start = Instant::now();
        plain.execute_batch("BEGIN").unwrap();
        plain.execute(fill, []).unwrap();
        plain.execute_batch("COMMIT").unwrap();
        times.push(seconds_since(start));
        plain.execute_batch("DELETE FROM playback_context").unwrap();
    }
    plain
        .execute_batch("PRAGMA cache_size = -2000; PRAGMA cache_spill = ON")
        .unwrap();
    report(&format!("plain SQLite, {label}, committed"), times);
}

/// Time reading every row `rows` names, as `plain_statement` times a write.
fn plain_query(plain: &rusqlite::Connection, label: &str, rows: &str) {
    let count = format!("SELECT count(*) FROM ({rows})");
    let mut times = Vec::new();
    for _ in 0..RUNS {
        let start = Instant::now();
        let counted: i64 = plain.query_row(&count, [], |row| row.get(0)).unwrap();
        times.push(seconds_since(start));
        assert_eq!(counted, TRACKS);
    }
    report(&format!("plain SQLite, {label}"), times);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: fills a 1,000,000-track library"]
async fn playback_queue_bench_library_play() {
    let (db, _tmp, path) = library_db().await;

    bae_writes(&db, "no live queries").await;

    // The queue screens open, each following the queue as it changes.
    let mut value = db.subscribe_queue_value();
    let mut upcoming = db.subscribe_queue_upcoming(crate::library::LibraryPageWindows::from([
        crate::library::LibraryPageWindow {
            offset: 0,
            limit: 100,
        },
    ]));
    value.next().await.unwrap().unwrap();
    upcoming.next().await.unwrap();
    let screens = [
        tokio::spawn(async move { while value.next().await.is_some() {} }),
        tokio::spawn(async move { while upcoming.next().await.is_some() {} }),
    ];
    bae_writes(&db, "queue screens open").await;
    for screen in screens {
        screen.abort();
    }

    // The same statements in plain SQLite, the lane empty and then full.
    let plain = plain_connection(&path);
    let into_lane = fill_into(LIBRARY_FILL, "playback_context");
    let into_unindexed = fill_into(LIBRARY_FILL, "bench_unindexed_lane");
    plain_query(
        &plain,
        "the fill's rows alone, read and counted",
        LIBRARY_ROWS,
    );
    plain_statement(&plain, "fill", "", &into_lane, false);
    plain_statement(&plain, "fill", "", &into_lane, true);
    let in_track_order = fill_into(LIBRARY_FILL_IN_TRACK_ORDER, "playback_context");
    plain_fill(&plain, "fill", "", &into_lane);
    plain_fill(&plain, "fill in track order", "", &in_track_order);
    // Changed pages past the cache are written out mid-statement, then read
    // back and written again as the fill keeps changing them.
    for (label, fill) in [
        ("fill", &into_lane),
        ("fill in track order", &in_track_order),
    ] {
        plain_fill(
            &plain,
            &format!("{label}, changed pages kept in memory"),
            "PRAGMA cache_spill = OFF",
            fill,
        );
    }
    // SQLite's default page cache is 2 MiB; a larger one holds more of the
    // indexes a fill writes out of order.
    for mebibytes in [16, 64, 256] {
        let pragma = format!("PRAGMA cache_size = -{}", mebibytes * 1024);
        plain_fill(
            &plain,
            &format!("fill, {mebibytes} MiB page cache"),
            &pragma,
            &into_lane,
        );
        plain_fill(
            &plain,
            &format!("fill in track order, {mebibytes} MiB page cache"),
            &pragma,
            &in_track_order,
        );
    }
    plain_statement(
        &plain,
        "fill without the track_id index",
        UNINDEXED_LANE,
        &into_unindexed,
        false,
    );
    db.playback_queue_tables()
        .fill(ContextFill::LibraryShuffled)
        .await
        .unwrap();
    let copy_lane = format!(
        "{UNINDEXED_LANE} INSERT INTO bench_unindexed_lane SELECT entry, position, track_id, unshuffled FROM playback_context;"
    );
    let empty_lane = "DELETE FROM playback_context";
    let empty_unindexed = "DELETE FROM bench_unindexed_lane";
    plain_statement(&plain, "empty", "", empty_lane, false);
    plain_statement(&plain, "empty", "", empty_lane, true);
    plain_statement(
        &plain,
        "empty without the track_id index",
        &copy_lane,
        empty_unindexed,
        false,
    );
    plain_statement(
        &plain,
        "empty without the track_id index",
        &copy_lane,
        empty_unindexed,
        true,
    );
}
