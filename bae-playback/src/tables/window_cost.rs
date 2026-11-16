//! What the playback queue's upcoming windows cost to read, measured by the
//! library's window-cost harness.

use super::*;
use bae_store::test_fixtures::window_cost::{median, offsets, report, LIMIT};

/// What the playback queue's upcoming windows cost to read.
pub async fn measure_playback_queue(store: &Store, n: u64) {
    // A window that jumps into the context lane's upcoming tracks finds them
    // by offset alone: SQLite skips the lane's earlier places by offset
    // before it reads any track.
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
