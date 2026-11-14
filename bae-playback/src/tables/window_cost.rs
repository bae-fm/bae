//! What the playback queue's upcoming windows cost to read, measured by the
//! library's window-cost harness.

use super::*;
use bae_store::test_fixtures::window_cost::{median, offsets, report, LIMIT};

pub use super::now::shaped_now_playing_on;
pub use super::queue::{fill_context_lane_on, shaped_playback_on};

/// What the playback queue's upcoming windows cost to read.
pub async fn measure_playback_queue(store: &Store, n: u64) {
    // The context lane's upcoming windows are read by offset alone: the
    // offset steps over the lane's places before any track is read.
    let lane = format!(
        "SELECT e.entry, {columns} \
         FROM (SELECT 'c' || entry AS entry, track_id, ordered AS position \
               FROM playback_context \
               WHERE ordered > -1 ORDER BY ordered LIMIT ?1 OFFSET ?2) e \
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
