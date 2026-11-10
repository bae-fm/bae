//! The playback queue tables' own rules: what the schema refuses, what a
//! deleted track takes with it, what opening keeps, and how the reads are
//! planned.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::tables::tests::*;
/// Play `RELEASE_A` from `start`, returning the entry playing.
async fn play(db: &Store, start: ReleaseStart) -> crate::QueueEntry {
    match PlaybackQueueTables::new(db.clone())
        .fill(ContextFill::Release { release_id: RELEASE_A.to_string(), start })
        .await
        .unwrap()
    {
        ContextFilled::Playing { current, .. } => current,
        other => panic!("the release plays nothing: {other:?}"),
    }
}

/// Play `tracks` in order, from the one at `start`, returning the entry
/// playing.
async fn play_tracks(db: &Store, tracks: &[String], start: usize) -> crate::QueueEntry {
    match PlaybackQueueTables::new(db.clone())
        .fill(ContextFill::Tracks { track_ids: tracks.to_vec(), start })
        .await
        .unwrap()
    {
        ContextFilled::Playing { current, .. } => current,
        other => panic!("the tracks play nothing: {other:?}"),
    }
}

async fn upcoming(db: &Store) -> Vec<String> {
    let manual = queue_value(db).await.unwrap().manual;
    manual.into_iter().chain(queue_upcoming(db).await.unwrap()).map(|item| item.track_id).collect()
}

async fn rejects(db: &Store, sql: &str) -> bool {
    let sql = sql.to_string();
    db.call(move |conn| conn.execute_batch(&sql).map_err(DbError::from)).await.is_err()
}

/// What the context lane plays from, whether it is shuffled and where it
/// stands are NULL together; a release is named exactly when the lane plays
/// one; a position is never negative.
#[tokio::test]
async fn the_schema_refuses_a_half_written_context_and_a_negative_position() {
    let (db, _tmp, _tracks) = release_db(1).await;
    assert!(rejects(&db, "UPDATE playback_queue SET source_kind = 'library'").await);
    assert!(rejects(&db, "UPDATE playback_queue SET source_kind = 'library', shuffled = 1").await);
    assert!(
        rejects(&db, "UPDATE playback_queue SET source_kind = 'release', shuffled = 0, cursor = 1")
            .await
    );
    assert!(
        rejects(
            &db,
            "UPDATE playback_queue \
             SET source_kind = 'releases', source_release_id = 'release', shuffled = 0, cursor = 1"
        )
        .await
    );
    assert!(rejects(&db, "UPDATE playback_queue SET source_release_id = 'release'").await);
    assert!(
        rejects(
            &db,
            "INSERT INTO playback_state (id, current_entry, current_track_id, position_ms) \
             VALUES ('current', NULL, NULL, -1)"
        )
        .await
    );
}

/// The count and the number of the addition it belongs to are set together.
#[tokio::test]
async fn the_schema_refuses_a_count_without_an_addition() {
    let (db, _tmp, _tracks) = release_db(1).await;
    for sql in [
        "UPDATE playback_queue SET added_tracks = 2",
        "UPDATE playback_queue SET additions = 1",
        "UPDATE playback_queue SET additions = 1, added_tracks = 0",
    ] {
        assert!(rejects(&db, sql).await, "{sql}");
    }
}

/// The same track queued twice shows twice, in order, each with its own
/// entry id.
#[tokio::test]
async fn duplicate_entries_show_in_order_with_distinct_ids() {
    let (db, _tmp, tracks) = release_db(2).await;
    let items = queue_items(&db, &[&tracks[0], &tracks[0], &tracks[1]]).await;
    let track_ids: Vec<&str> = items.iter().map(|i| i.track_id.as_str()).collect();
    assert_eq!(track_ids, vec![tracks[0].as_str(), tracks[0].as_str(), tracks[1].as_str()]);
    let mut entry_ids: Vec<&str> = items.iter().map(|i| i.entry_id.as_str()).collect();
    entry_ids.dedup();
    assert_eq!(entry_ids.len(), 3);
}

/// A deleted track leaves both lanes.
#[tokio::test]
async fn deleting_a_queued_track_takes_its_rows() {
    let (db, _tmp, tracks) = single_track_releases_db(3).await;
    play_tracks(&db, &tracks, 0).await;
    queue_items(&db, &[&tracks[1], &tracks[2]]).await;

    delete_release_of(&db, &tracks[1]).await;

    assert_eq!(upcoming(&db).await, vec![tracks[2].clone(), tracks[2].clone()]);
}

/// A track queued twice in each lane leaves all four places when it is
/// deleted, through each lane's plain index on its track.
#[tokio::test]
async fn deleting_a_track_queued_twice_takes_every_entry() {
    let (db, _tmp, tracks) = single_track_releases_db(2).await;
    let filled = PlaybackQueueTables::new(db.clone())
        .fill(ContextFill::Tracks { track_ids: [&tracks[..], &tracks[..]].concat(), start: 0 })
        .await
        .unwrap();
    assert!(matches!(filled, ContextFilled::Playing { track_count: 4, .. }));
    queue_items(&db, &[&tracks[1], &tracks[1]]).await;
    assert_eq!(
        upcoming(&db).await,
        vec![
            tracks[1].clone(),
            tracks[1].clone(),
            tracks[1].clone(),
            tracks[0].clone(),
            tracks[1].clone(),
        ]
    );

    delete_release_of(&db, &tracks[1]).await;

    assert_eq!(upcoming(&db).await, vec![tracks[0].clone()]);
}

/// Deleting the playing track keeps where the lane stands: the track after it
/// still plays next and the one before it is still behind.
#[tokio::test]
async fn deleting_the_playing_track_keeps_the_lane_s_place() {
    let (db, _tmp, tracks) = single_track_releases_db(3).await;
    play_tracks(&db, &tracks, 1).await;

    delete_release_of(&db, &tracks[1]).await;

    assert_eq!(upcoming(&db).await, vec![tracks[2].clone()]);
    assert!(queue_value(&db).await.unwrap().has_previous);
    let back = PlaybackQueueTables::new(db.clone()).step_back(None).await.unwrap();
    assert_eq!(back.map(|entry| entry.track_id), Some(tracks[0].clone()));
}

/// What the `playback_queue` row says the context lane plays from: its
/// source's kind and release, whether it is shuffled, and its cursor.
async fn context_head(db: &Store) -> (Option<String>, Option<String>, Option<bool>, Option<i64>) {
    db.read(|sql| {
        Ok(sql.query_row(
            "SELECT source_kind, source_release_id, shuffled, cursor FROM playback_queue",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?)
    })
    .await
    .unwrap()
}

/// The context lane's last row leaving takes what it played from with it.
#[tokio::test]
async fn the_lane_s_last_track_deleted_clears_the_context() {
    let (db, _tmp, tracks) = single_track_releases_db(1).await;
    play_tracks(&db, &tracks, 0).await;
    assert!(queue_value(&db).await.unwrap().context.is_some());

    delete_release_of(&db, &tracks[0]).await;

    assert!(queue_value(&db).await.unwrap().context.is_none());
    assert_eq!(context_head(&db).await, (None, None, None, None));
}

/// A play of several releases stores that it plays several, not which: its
/// tracks say which. One that finds a single release with tracks plays that
/// release, named.
#[tokio::test]
async fn a_play_of_releases_stores_its_kind_and_a_lone_release_its_id() {
    let (db, _tmp, _tracks) = release_db(1).await;
    let fill = |release_ids: Vec<String>| {
        let db = db.clone();
        async move {
            PlaybackQueueTables::new(db.clone())
                .fill(ContextFill::Releases(release_ids))
                .await
                .unwrap()
        }
    };

    fill(vec![RELEASE_A.to_string(), RELEASE_A.to_string()]).await;
    assert_eq!(context_head(&db).await, (Some("releases".to_string()), None, Some(false), Some(1)));
    let context = queue_value(&db).await.unwrap().context.unwrap();
    assert_eq!(context.source, crate::ContextSource::Releases);
    assert_eq!(context.source_title, None);

    fill(vec![RELEASE_B.to_string(), RELEASE_A.to_string()]).await;
    assert_eq!(
        context_head(&db).await,
        (Some("release".to_string()), Some(RELEASE_A.to_string()), Some(false), Some(1))
    );
    assert_eq!(
        queue_value(&db).await.unwrap().context.unwrap().source,
        crate::ContextSource::Release(RELEASE_A.to_string())
    );
}

/// Deleting every track of the lane in one write takes what it played from
/// with the last of them.
#[tokio::test]
async fn the_lane_s_tracks_deleted_together_clear_the_context() {
    let (db, _tmp, _tracks) = release_db(3).await;
    play(&db, ReleaseStart::First).await;

    delete_release(&test_records(&db), RELEASE_A).await;

    assert_eq!(context_head(&db).await, (None, None, None, None));
}

/// Removing the lane's last row takes what it played from with it; removing
/// any other row leaves it.
#[tokio::test]
async fn removing_the_lane_s_last_row_clears_the_context() {
    let (db, _tmp, _tracks) = release_db(2).await;
    let playing = play(&db, ReleaseStart::First).await;
    let next = queue_upcoming(&db).await.unwrap().remove(0);
    let tables = PlaybackQueueTables::new(db.clone());

    tables.remove(next.entry_id.parse().unwrap()).await.unwrap().expect("the lane holds it");
    assert_eq!(
        context_head(&db).await,
        (Some("release".to_string()), Some(RELEASE_A.to_string()), Some(false), Some(1))
    );

    tables.remove(playing.id).await.unwrap().expect("the lane holds it");
    assert_eq!(context_head(&db).await, (None, None, None, None));
}

/// No trigger runs on the context lane's rows: a play takes out and puts in
/// up to a library's worth of them, and a trigger would run once for each
/// row taken out and keep SQLite from emptying the table without visiting
/// its rows.
#[tokio::test]
async fn the_context_lane_has_no_trigger() {
    let (db, _tmp, _tracks) = release_db(0).await;
    let triggers: Vec<String> = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT name FROM sqlite_master \
                 WHERE type = 'trigger' AND tbl_name = 'playback_context'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert!(triggers.is_empty(), "{triggers:?}");
}

/// The resume row names the entry playing; deleting its track leaves the row
/// with nothing to resume.
#[tokio::test]
async fn the_resume_row_round_trips_and_forgets_a_deleted_track() {
    let (db, _tmp, tracks) = single_track_releases_db(2).await;
    let current = play_tracks(&db, &tracks, 0).await;
    let state = PlaybackResumeRow { current: Some(current.clone()), position_ms: Some(42_000) };
    PlaybackQueueTables::new(db.clone()).save_playback_state(&state).await.unwrap();

    let loaded = load_playback_state(&db).await.unwrap().expect("a stored row");
    assert_eq!(loaded.current, Some(current));
    assert_eq!(loaded.position_ms, Some(42_000));

    delete_release_of(&db, &tracks[0]).await;
    let forgotten = load_playback_state(&db).await.unwrap().expect("the row stays");
    assert_eq!(forgotten.current, None);

    PlaybackQueueTables::new(db.clone()).clear_playback_state().await.unwrap();
    assert!(load_playback_state(&db).await.unwrap().is_none());
}

/// Opening keeps the queue only when it will be resumed: "restore on launch"
/// on and a resume row stored. Otherwise the queue and the row are emptied.
/// Either way what playback is doing starts stopped; its volume and mute are
/// kept only to resume.
#[tokio::test]
async fn opening_keeps_the_queue_only_to_resume_it() {
    let (db, _tmp, tracks) = release_db(2).await;
    let current = play(&db, ReleaseStart::First).await;
    PlaybackQueueTables::new(db.clone()).set_repeat(crate::RepeatMode::Context).await.unwrap();
    let state = PlaybackResumeRow { current: Some(current.clone()), position_ms: None };
    PlaybackQueueTables::new(db.clone()).save_playback_state(&state).await.unwrap();
    let now = PlaybackNowRow::new(db.clone());
    for change in [
        crate::PlaybackChange::State(crate::PlaybackState::Playing {
            track: crate::PlayingTrack {
                track_id: current.track_id.clone(),
                duration_ms: Some(1_000),
            },
        }),
        crate::PlaybackChange::Volume(0.5),
        crate::PlaybackChange::Muted(true),
        crate::PlaybackChange::Failed(crate::PlaybackErrorReason::OriginalMissing),
    ] {
        now.apply(&change).await.unwrap();
    }

    settle_playback_queue(&db, true).await.unwrap();
    assert_eq!(upcoming(&db).await, vec![tracks[1].clone()], "kept to resume");
    let resumed = now_playing(&db).await.unwrap().expect("nothing to display");
    assert_eq!(resumed.playback.state, crate::PlaybackState::Stopped);
    assert_eq!(resumed.failure, None);
    assert_eq!(
        playback_level(&db).await.unwrap(),
        (crate::Volume::new(0.5).unwrap(), true),
        "kept to resume"
    );

    settle_playback_queue(&db, false).await.unwrap();
    assert_eq!(playback_level(&db).await.unwrap(), (crate::Volume::new(1.0).unwrap(), false));
    assert!(queue_value(&db).await.unwrap().context.is_none(), "restore off empties it");
    assert!(load_playback_state(&db).await.unwrap().is_none());
    assert_eq!(
        PlaybackQueueTables::new(db.clone()).repeat().await.unwrap(),
        crate::RepeatMode::Off
    );

    play(&db, ReleaseStart::First).await;
    settle_playback_queue(&db, true).await.unwrap();
    assert!(
        queue_value(&db).await.unwrap().context.is_none(),
        "with no resume row there is nothing to resume"
    );
}

async fn plan(db: &Store, sql: &str, args: Vec<rusqlite::types::Value>) -> String {
    let sql = format!("EXPLAIN QUERY PLAN {sql}");
    db.read(move |read| {
        Ok(read
            .query(&sql, rusqlite::params_from_iter(args), |row| row.get::<_, String>("detail"))?
            .join(" | "))
    })
    .await
    .unwrap()
}

/// The queue's reads walk the lane's position index from the playing track
/// rather than scanning the lane, and a deleted track's rows are found by its
/// id.
#[tokio::test]
async fn the_queue_reads_are_planned_on_the_indexes() {
    let (db, _tmp, _tracks) = release_db(3).await;
    let int = |value: i64| rusqlite::types::Value::Integer(value);
    let window = plan(
        &db,
        "SELECT e.entry, t.id FROM (SELECT 'c' || entry AS entry, track_id, position \
         FROM playback_context WHERE position > ? ORDER BY position LIMIT 100 OFFSET 10000) e \
         JOIN tracks t ON t.id = e.track_id ORDER BY e.position",
        vec![int(1)],
    )
    .await;
    assert!(
        window.contains(
            "SEARCH playback_context USING INDEX sqlite_autoindex_playback_context_1 (position>?)"
        ) && window.contains("INDEX sqlite_autoindex_tracks_1 (id=?)")
            && !window.contains("TEMP B-TREE"),
        "{window}"
    );
    let count =
        plan(&db, "SELECT count(*) FROM playback_context WHERE position > ?", vec![int(1)]).await;
    assert!(
        count.contains("USING COVERING INDEX sqlite_autoindex_playback_context_1 (position>?)"),
        "{count}"
    );
    let text = |value: &str| rusqlite::types::Value::Text(value.to_string());
    for (table, index) in [
        ("playback_context", "playback_context_track"),
        ("playback_manual", "playback_manual_track"),
    ] {
        let by_track =
            plan(&db, &format!("SELECT 1 FROM {table} WHERE track_id = ?"), vec![text("")]).await;
        assert!(by_track.contains(&format!("USING COVERING INDEX {index}")), "{by_track}");
    }
}

/// A context row is named by its entry, as an Up Next row is, and no entry
/// number names a row of either lane twice: a new play numbers its rows on
/// from the ones before it, so an id a screen still holds from the last play
/// names nothing.
#[tokio::test]
async fn a_context_row_is_named_by_its_entry() {
    let (db, _tmp, tracks) = release_db(3).await;
    let queued = queue_items(&db, &[&tracks[0]]).await;
    let first = play(&db, ReleaseStart::First).await;
    let first_upcoming = queue_upcoming(&db).await.unwrap();
    let second = play(&db, ReleaseStart::First).await;
    let upcoming = queue_upcoming(&db).await.unwrap();

    let ids: Vec<String> = [queued[0].entry_id.clone(), first.id.to_string()]
        .into_iter()
        .chain(first_upcoming.iter().map(|item| item.entry_id.clone()))
        .chain([second.id.to_string()])
        .chain(upcoming.iter().map(|item| item.entry_id.clone()))
        .collect();
    assert!(ids[0].starts_with('m'), "{ids:?}");
    assert!(ids[1..].iter().all(|id| id.starts_with('c')), "{ids:?}");
    let mut numbers: Vec<&str> = ids.iter().map(|id| &id[1..]).collect();
    numbers.sort();
    numbers.dedup();
    assert_eq!(numbers.len(), 7, "{ids:?}");

    let tables = PlaybackQueueTables::new(db.clone());
    assert_eq!(
        tables.skip_to(first_upcoming[1].entry_id.parse().unwrap()).await.unwrap(),
        None,
        "an entry of the last play"
    );
    let skipped = tables
        .skip_to(upcoming[1].entry_id.parse().unwrap())
        .await
        .unwrap()
        .expect("the row its entry names");
    assert_eq!(skipped.track_id, tracks[2]);
    assert!(queue_upcoming(&db).await.unwrap().is_empty());
}

/// Up Next rows are entries of their own, so the same track queued twice is
/// two rows the screens tell apart.
#[tokio::test]
async fn an_up_next_row_is_named_by_its_entry() {
    let (db, _tmp, tracks) = release_db(1).await;
    let items = queue_items(&db, &[&tracks[0], &tracks[0]]).await;
    assert!(items.iter().all(|item| item.entry_id.starts_with('m')));
    assert_ne!(items[0].entry_id, items[1].entry_id);
    let removed = PlaybackQueueTables::new(db.clone())
        .remove(items[1].entry_id.parse().unwrap())
        .await
        .unwrap();
    assert!(removed.is_some());
    assert_eq!(queue_value(&db).await.unwrap().manual, items[..1].to_vec());
}

/// A deleted track leaves the queue through a trigger on `tracks`, not a
/// foreign key, so filling the lane looks nothing up in `tracks` per row.
#[tokio::test]
async fn the_queue_tables_hold_no_foreign_key_and_a_deleted_track_still_leaves() {
    let (db, _tmp, tracks) = single_track_releases_db(3).await;
    let definitions: Vec<String> = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT sql FROM sqlite_master \
                 WHERE type = 'table' AND name IN ('playback_context', 'playback_manual')",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(definitions.len(), 2);
    assert!(definitions.iter().all(|sql| !sql.contains("REFERENCES")), "{definitions:?}");

    play_tracks(&db, &tracks, 0).await;
    queue_items(&db, &[&tracks[2]]).await;
    delete_release_of(&db, &tracks[2]).await;
    assert_eq!(upcoming(&db).await, vec![tracks[1].clone()]);
}

/// What a library-sized play costs the playback queue tables: filling,
/// replacing and emptying a context lane of `TRACKS` tracks through bae's
/// own writes, at bae's page cache size, then the same statements in plain
/// SQLite on the same file: the fill in play order, as bae fills, and in
/// track order, with SQLite's default page cache and larger ones, and the
/// emptying, with and without a pre-update hook.
/// Ignored by default; run with
/// `scripts/cargo.sh test -p bae-playback --lib -- playback_queue_bench --ignored --nocapture`.
/// Each line it prints is one measurement over `RUNS` runs: the fastest,
/// which other work on the machine slows least, the median and every run.
mod playback_queue_bench {
    use super::*;
    use std::time::Instant;
    use ContextFill;

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
        fill.replace("{rows}", LIBRARY_ROWS).replace("{table}", table)
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
        plain.busy_timeout(std::time::Duration::from_secs(60)).unwrap();
        plain
    }

    /// `RELEASE_A` holding `TRACKS` tracks with random ids, as a library's are.
    async fn library_db() -> (Store, tempfile::TempDir, std::path::PathBuf) {
        let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
        seed_artist_album_release(&db).await;
        test_records(&db)
            .insert_release_medium(&bae_records::DbReleaseMedium::new_test(RELEASE_A))
            .await
            .unwrap();
        let path = database_file(tmp.path());
        bae_records::test_fixtures::bulk_tracks::insert_with_triggers_off(&path, RELEASE_A, TRACKS);
        (db, tmp, path)
    }

    /// A library play through bae's writes: into an empty lane, replacing a full
    /// one, and the two writes that empty it.
    async fn bae_writes(db: &Store, label: &str) {
        let tables = PlaybackQueueTables::new(db.clone());
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
            settle_playback_queue(db, false).await.unwrap();
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
        plain: &mut rusqlite::Connection,
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
                            fn(
                                rusqlite::hooks::Action,
                                &str,
                                &str,
                                &rusqlite::hooks::PreUpdateCase,
                            ),
                        >,
                    )
                    .unwrap();
            }
            plain.execute_batch("ROLLBACK").unwrap();
        }
        let hook = if hooked { "with a pre-update hook" } else { "no hook" };
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
        plain.execute_batch("PRAGMA cache_size = -2000; PRAGMA cache_spill = ON").unwrap();
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
        let mut value = subscribe_queue_value(&db);
        let mut upcoming = subscribe_queue_upcoming(
            &db,
            bae_store::LibraryPageWindows::from([bae_store::LibraryPageWindow {
                offset: 0,
                limit: 100,
            }]),
        );
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
        let mut plain = plain_connection(&path);
        let into_lane = fill_into(LIBRARY_FILL, "playback_context");
        let into_unindexed = fill_into(LIBRARY_FILL, "bench_unindexed_lane");
        plain_query(&plain, "the fill's rows alone, read and counted", LIBRARY_ROWS);
        plain_statement(&mut plain, "fill", "", &into_lane, false);
        plain_statement(&mut plain, "fill", "", &into_lane, true);
        let in_track_order = fill_into(LIBRARY_FILL_IN_TRACK_ORDER, "playback_context");
        plain_fill(&plain, "fill", "", &into_lane);
        plain_fill(&plain, "fill in track order", "", &in_track_order);
        // Changed pages past the cache are written out mid-statement, then read
        // back and written again as the fill keeps changing them.
        for (label, fill) in [("fill", &into_lane), ("fill in track order", &in_track_order)] {
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
            plain_fill(&plain, &format!("fill, {mebibytes} MiB page cache"), &pragma, &into_lane);
            plain_fill(
                &plain,
                &format!("fill in track order, {mebibytes} MiB page cache"),
                &pragma,
                &in_track_order,
            );
        }
        plain_statement(
            &mut plain,
            "fill without the track_id index",
            UNINDEXED_LANE,
            &into_unindexed,
            false,
        );
        PlaybackQueueTables::new(db.clone()).fill(ContextFill::LibraryShuffled).await.unwrap();
        let copy_lane = format!(
            "{UNINDEXED_LANE} INSERT INTO bench_unindexed_lane SELECT entry, position, track_id, unshuffled FROM playback_context;"
        );
        let empty_lane = "DELETE FROM playback_context";
        let empty_unindexed = "DELETE FROM bench_unindexed_lane";
        plain_statement(&mut plain, "empty", "", empty_lane, false);
        plain_statement(&mut plain, "empty", "", empty_lane, true);
        plain_statement(
            &mut plain,
            "empty without the track_id index",
            &copy_lane,
            empty_unindexed,
            false,
        );
        plain_statement(
            &mut plain,
            "empty without the track_id index",
            &copy_lane,
            empty_unindexed,
            true,
        );
    }
}
