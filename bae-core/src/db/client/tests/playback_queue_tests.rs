//! The playback queue tables' own rules: what the schema refuses, what a
//! deleted track takes with it, what opening keeps, and how the reads are
//! planned.

use super::super::*;
use super::*;
use crate::db::{ContextFill, ContextFilled, ReleaseStart};

/// `RELEASE_A` holding `count` tracks, in order, and their ids.
async fn release_db(count: usize) -> (Database, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = super::temp_db().await;
    let track_ids: Vec<String> = (0..count)
        .map(|index| bae_test_support::test_uuid(&format!("queue-track-{index}")))
        .collect();
    let seed = track_ids.clone();
    db.call(move |conn| {
        super::seed_artist_album_release(conn)?;
        let medium_id = crate::db::identity::release_medium_id(RELEASE_A, 1);
        conn.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
             VALUES (?1, ?2, 1, 'cd', 1, '2026-01-01T00:00:00Z')",
            params![medium_id, RELEASE_A],
        )?;
        for (index, track_id) in seed.iter().enumerate() {
            conn.execute(
                "INSERT INTO tracks \
                     (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
                 VALUES (?3, ?1, ?5, ?2, ?4, 1, ?3 + 1, 1000, '2026-01-01T00:00:00Z')",
                params![track_id, format!("Track Title {index}"), index as i64, medium_id, RELEASE_A],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    (db, tmp, track_ids)
}

/// Play `RELEASE_A` from `start`, returning the entry playing.
async fn play(db: &Database, start: ReleaseStart) -> crate::playback::QueueEntry {
    match db
        .playback_queue_tables()
        .fill(ContextFill::Release {
            release_id: RELEASE_A.to_string(),
            start,
        })
        .await
        .unwrap()
    {
        ContextFilled::Playing { current, .. } => current,
        other => panic!("the release plays nothing: {other:?}"),
    }
}

async fn upcoming(db: &Database) -> Vec<String> {
    let manual = db.queue_value().await.unwrap().manual;
    manual
        .into_iter()
        .chain(db.queue_upcoming().await.unwrap())
        .map(|item| item.track_id)
        .collect()
}

async fn rejects(db: &Database, sql: &str) -> bool {
    let sql = sql.to_string();
    db.call(move |conn| conn.execute_batch(&sql).map_err(DbError::from))
        .await
        .is_err()
}

/// What the context lane plays from, whether it is shuffled and where it
/// stands are NULL together; a position is never negative.
#[tokio::test]
async fn the_schema_refuses_a_half_written_context_and_a_negative_position() {
    let (db, _tmp, _tracks) = release_db(1).await;
    assert!(rejects(&db, "UPDATE playback_queue SET source = 'library'").await);
    assert!(
        rejects(
            &db,
            "UPDATE playback_queue SET source = 'library', shuffled = 1"
        )
        .await
    );
    assert!(
        rejects(
            &db,
            "INSERT INTO playback_state (id, current_entry, current_track_id, position_ms, volume, is_muted) \
             VALUES ('current', NULL, NULL, -1, 1.0, 0)"
        )
        .await
    );
}

/// The same track queued twice shows twice, in order, each with its own
/// entry id.
#[tokio::test]
async fn duplicate_entries_show_in_order_with_distinct_ids() {
    let (db, _tmp, tracks) = release_db(2).await;
    let items = super::queue_items(&db, &[&tracks[0], &tracks[0], &tracks[1]]).await;
    let track_ids: Vec<&str> = items.iter().map(|i| i.track_id.as_str()).collect();
    assert_eq!(
        track_ids,
        vec![tracks[0].as_str(), tracks[0].as_str(), tracks[1].as_str()]
    );
    let mut entry_ids: Vec<&str> = items.iter().map(|i| i.entry_id.as_str()).collect();
    entry_ids.dedup();
    assert_eq!(entry_ids.len(), 3);
}

/// A deleted track leaves both lanes.
#[tokio::test]
async fn deleting_a_queued_track_takes_its_rows() {
    let (db, _tmp, tracks) = release_db(3).await;
    play(&db, ReleaseStart::First).await;
    super::queue_items(&db, &[&tracks[1], &tracks[2]]).await;

    exec(&db, "DELETE FROM tracks WHERE id = ?", &[&tracks[1]]).await;

    assert_eq!(
        upcoming(&db).await,
        vec![tracks[2].clone(), tracks[2].clone()]
    );
}

/// A track queued twice in each lane leaves all four places when it is
/// deleted, through each lane's plain index on its track.
#[tokio::test]
async fn deleting_a_track_queued_twice_takes_every_entry() {
    let (db, _tmp, tracks) = release_db(2).await;
    let filled = db
        .playback_queue_tables()
        .fill(ContextFill::Releases(vec![
            RELEASE_A.to_string(),
            RELEASE_A.to_string(),
        ]))
        .await
        .unwrap();
    assert!(matches!(
        filled,
        ContextFilled::Playing { track_count: 4, .. }
    ));
    super::queue_items(&db, &[&tracks[1], &tracks[1]]).await;
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

    exec(&db, "DELETE FROM tracks WHERE id = ?", &[&tracks[1]]).await;

    assert_eq!(upcoming(&db).await, vec![tracks[0].clone()]);
}

/// Deleting the playing track keeps where the lane stands: the track after it
/// still plays next and the one before it is still behind.
#[tokio::test]
async fn deleting_the_playing_track_keeps_the_lane_s_place() {
    let (db, _tmp, tracks) = release_db(3).await;
    play(&db, ReleaseStart::Track(tracks[1].clone())).await;

    exec(&db, "DELETE FROM tracks WHERE id = ?", &[&tracks[1]]).await;

    assert_eq!(upcoming(&db).await, vec![tracks[2].clone()]);
    assert!(db.queue_value().await.unwrap().has_previous);
    let back = db.playback_queue_tables().step_back(None).await.unwrap();
    assert_eq!(back.map(|entry| entry.track_id), Some(tracks[0].clone()));
}

/// What the `playback_queue` row says the context lane plays from: its
/// source, whether it is shuffled, and its cursor.
async fn context_head(db: &Database) -> (Option<String>, Option<bool>, Option<i64>) {
    db.read(|sql| {
        Ok(sql.query_row(
            "SELECT source, shuffled, cursor FROM playback_queue",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?)
    })
    .await
    .unwrap()
}

/// The context lane's last row leaving takes what it played from with it.
#[tokio::test]
async fn the_lane_s_last_track_deleted_clears_the_context() {
    let (db, _tmp, tracks) = release_db(1).await;
    play(&db, ReleaseStart::First).await;
    assert!(db.queue_value().await.unwrap().context.is_some());

    exec(&db, "DELETE FROM tracks WHERE id = ?", &[&tracks[0]]).await;

    assert!(db.queue_value().await.unwrap().context.is_none());
    assert_eq!(context_head(&db).await, (None, None, None));
}

/// Deleting every track of the lane in one write takes what it played from
/// with the last of them.
#[tokio::test]
async fn the_lane_s_tracks_deleted_together_clear_the_context() {
    let (db, _tmp, _tracks) = release_db(3).await;
    play(&db, ReleaseStart::First).await;

    exec(&db, "DELETE FROM tracks WHERE release_id = ?", &[RELEASE_A]).await;

    assert_eq!(context_head(&db).await, (None, None, None));
}

/// Removing the lane's last row takes what it played from with it; removing
/// any other row leaves it.
#[tokio::test]
async fn removing_the_lane_s_last_row_clears_the_context() {
    let (db, _tmp, _tracks) = release_db(2).await;
    let playing = play(&db, ReleaseStart::First).await;
    let next = db.queue_upcoming().await.unwrap().remove(0);
    let tables = db.playback_queue_tables();

    tables
        .remove(next.entry_id.parse().unwrap())
        .await
        .unwrap()
        .expect("the lane holds it");
    assert_eq!(
        context_head(&db).await,
        (Some(RELEASE_A.to_string()), Some(false), Some(1))
    );

    tables
        .remove(playing.id)
        .await
        .unwrap()
        .expect("the lane holds it");
    assert_eq!(context_head(&db).await, (None, None, None));
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
    let (db, _tmp, tracks) = release_db(2).await;
    let current = play(&db, ReleaseStart::First).await;
    let state = DbPlaybackState {
        current: Some(current.clone()),
        position_ms: Some(42_000),
        volume: 0.5,
        is_muted: true,
    };
    db.save_playback_state(&state).await.unwrap();

    let loaded = db
        .load_playback_state()
        .await
        .unwrap()
        .expect("a stored row");
    assert_eq!(loaded.current, Some(current));
    assert_eq!(loaded.position_ms, Some(42_000));
    assert!((loaded.volume - 0.5).abs() < f32::EPSILON);
    assert!(loaded.is_muted);

    exec(&db, "DELETE FROM tracks WHERE id = ?", &[&tracks[0]]).await;
    let forgotten = db
        .load_playback_state()
        .await
        .unwrap()
        .expect("the row stays");
    assert_eq!(forgotten.current, None);

    db.clear_playback_state().await.unwrap();
    assert!(db.load_playback_state().await.unwrap().is_none());
}

/// Opening keeps the queue only when it will be resumed: "restore on launch"
/// on and a resume row stored. Otherwise the queue and the row are emptied.
#[tokio::test]
async fn opening_keeps_the_queue_only_to_resume_it() {
    let (db, _tmp, tracks) = release_db(2).await;
    let current = play(&db, ReleaseStart::First).await;
    db.playback_queue_tables()
        .set_repeat(crate::playback::RepeatMode::Context)
        .await
        .unwrap();
    let state = DbPlaybackState {
        current: Some(current),
        position_ms: None,
        volume: 1.0,
        is_muted: false,
    };
    db.save_playback_state(&state).await.unwrap();

    db.settle_playback_queue(true).await.unwrap();
    assert_eq!(
        upcoming(&db).await,
        vec![tracks[1].clone()],
        "kept to resume"
    );

    db.settle_playback_queue(false).await.unwrap();
    assert!(
        db.queue_value().await.unwrap().context.is_none(),
        "restore off empties it"
    );
    assert!(db.load_playback_state().await.unwrap().is_none());
    assert_eq!(
        db.playback_queue_tables().repeat().await.unwrap(),
        crate::playback::RepeatMode::Off
    );

    play(&db, ReleaseStart::First).await;
    db.settle_playback_queue(true).await.unwrap();
    assert!(
        db.queue_value().await.unwrap().context.is_none(),
        "with no resume row there is nothing to resume"
    );
}

async fn plan(db: &Database, sql: &str, args: Vec<rusqlite::types::Value>) -> String {
    let sql = format!("EXPLAIN QUERY PLAN {sql}");
    db.read(move |read| {
        Ok(read
            .query(&sql, rusqlite::params_from_iter(args), |row| {
                row.get::<_, String>("detail")
            })?
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
    let count = plan(
        &db,
        "SELECT count(*) FROM playback_context WHERE position > ?",
        vec![int(1)],
    )
    .await;
    assert!(
        count.contains("USING COVERING INDEX sqlite_autoindex_playback_context_1 (position>?)"),
        "{count}"
    );
    let text = |value: &str| rusqlite::types::Value::Text(value.to_string());
    for (table, index) in [
        ("playback_context", "playback_context_track"),
        ("playback_manual", "playback_manual_track"),
    ] {
        let by_track = plan(
            &db,
            &format!("SELECT 1 FROM {table} WHERE track_id = ?"),
            vec![text("")],
        )
        .await;
        assert!(
            by_track.contains(&format!("USING COVERING INDEX {index}")),
            "{by_track}"
        );
    }
}

/// A context row is named by its entry, as an Up Next row is, and no entry
/// number names a row of either lane twice: a new play numbers its rows on
/// from the ones before it, so an id a screen still holds from the last play
/// names nothing.
#[tokio::test]
async fn a_context_row_is_named_by_its_entry() {
    let (db, _tmp, tracks) = release_db(3).await;
    let queued = super::queue_items(&db, &[&tracks[0]]).await;
    let first = play(&db, ReleaseStart::First).await;
    let first_upcoming = db.queue_upcoming().await.unwrap();
    let second = play(&db, ReleaseStart::First).await;
    let upcoming = db.queue_upcoming().await.unwrap();

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

    let tables = db.playback_queue_tables();
    assert_eq!(
        tables
            .skip_to(first_upcoming[1].entry_id.parse().unwrap())
            .await
            .unwrap(),
        None,
        "an entry of the last play"
    );
    let skipped = tables
        .skip_to(upcoming[1].entry_id.parse().unwrap())
        .await
        .unwrap()
        .expect("the row its entry names");
    assert_eq!(skipped.track_id, tracks[2]);
    assert!(db.queue_upcoming().await.unwrap().is_empty());
}

/// Up Next rows are entries of their own, so the same track queued twice is
/// two rows the screens tell apart.
#[tokio::test]
async fn an_up_next_row_is_named_by_its_entry() {
    let (db, _tmp, tracks) = release_db(1).await;
    let items = super::queue_items(&db, &[&tracks[0], &tracks[0]]).await;
    assert!(items.iter().all(|item| item.entry_id.starts_with('m')));
    assert_ne!(items[0].entry_id, items[1].entry_id);
    let removed = db
        .playback_queue_tables()
        .remove(items[1].entry_id.parse().unwrap())
        .await
        .unwrap();
    assert!(removed.is_some());
    assert_eq!(db.queue_value().await.unwrap().manual, items[..1].to_vec());
}

/// A deleted track leaves the queue through a trigger on `tracks`, not a
/// foreign key, so filling the lane looks nothing up in `tracks` per row.
#[tokio::test]
async fn the_queue_tables_hold_no_foreign_key_and_a_deleted_track_still_leaves() {
    let (db, _tmp, tracks) = release_db(3).await;
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
    assert!(
        definitions.iter().all(|sql| !sql.contains("REFERENCES")),
        "{definitions:?}"
    );

    play(&db, ReleaseStart::First).await;
    super::queue_items(&db, &[&tracks[2]]).await;
    exec(&db, "DELETE FROM tracks WHERE id = ?", &[&tracks[2]]).await;
    assert_eq!(upcoming(&db).await, vec![tracks[1].clone()]);
}
