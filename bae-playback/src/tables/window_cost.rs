//! What the playback queue's upcoming windows cost to read, measured by the
//! library's window-cost harness.

use super::*;
use bae_store::test_fixtures::window_cost::{median, offsets, report, LIMIT};

pub use super::now::shaped_now_playing_on;
pub use super::queue::{fill_context_lane_on, shaped_playback_on};

/// What the playback queue's upcoming windows cost to read.
pub async fn measure_playback_queue(store: &Store, n: u64) {
    // A jump into the context lane's upcoming tracks reads by offset alone:
    // the offset steps over the lane's places before any track is read.
    for (at, offset) in offsets(n) {
        let timing = median(store, move |sql| {
            let order = super::queue_upcoming::upcoming_order(false);
            Ok(super::queue_upcoming::upcoming_rows_on(
                sql,
                false,
                &order.everything(),
                LIMIT,
                offset,
            )?
            .len())
        })
        .await;
        report("queue_upcoming", "lane", "jump", at, offset, n, timing);
    }
}
