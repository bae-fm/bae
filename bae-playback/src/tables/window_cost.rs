//! What the playback queue's upcoming windows cost to read, measured by the
//! library's window-cost harness.

use super::*;
use bae_store::test_fixtures::window_cost::{median, numbered, offsets, report, LIMIT};

/// The context lane holding the tracks `start..end`, track `i` at place `i`
/// and named by `track_id_sql`, an SQL expression over `i`, and the queue
/// playing the library from before the lane's first track. The lane goes in
/// by one set-based statement, at the sizes the lists' window costs are
/// measured at.
pub fn fill_context_lane_on(
    sql: &CovenWrite<'_>,
    start: u64,
    end: u64,
    track_id_sql: &str,
) -> Result<(), DbError> {
    let rows = numbered(start, end);
    sql.execute_batch(&format!(
        "{rows} INSERT INTO playback_context (entry, position, track_id, unshuffled) \
             SELECT i, i, {track_id_sql}, i FROM k;
         UPDATE playback_queue SET source_kind = 'library', shuffled = 0, cursor = -1;"
    ))?;
    Ok(())
}

/// What the playback queue's upcoming windows cost to read.
pub async fn measure_playback_queue(store: &Store, n: u64) {
    // The context lane's upcoming windows are read by offset alone: the
    // offset steps over the lane's positions before any track is read.
    let lane = format!(
        "SELECT e.entry, {columns} \
         FROM (SELECT 'c' || entry AS entry, track_id, position FROM playback_context \
               WHERE position > -1 ORDER BY position LIMIT ?1 OFFSET ?2) e \
         JOIN tracks t ON t.id = e.track_id \
         {joins} \
         ORDER BY e.position",
        columns = super::track_display::track_display_columns(),
        joins = super::track_display::TRACK_DISPLAY_JOINS,
    );
    for (at, offset) in offsets(n) {
        let lane = lane.clone();
        let timing = median(store, move |sql| {
            Ok(sql.query(&lane, params![LIMIT as i64, offset as i64], |_| Ok(()))?.len())
        })
        .await;
        report("queue_upcoming", "lane", "jump", at, offset, n, timing);
    }
}
