//! What the playback queue's upcoming windows cost to read, measured by the
//! library's window-cost harness.

use super::*;
use bae_store::test_fixtures::window_cost::{median, offsets, report};

/// What the playback queue's upcoming windows cost to read.
pub async fn measure_playback_queue(store: &Store, n: u64) {
    // A jump into the context lane's upcoming tracks reads the key at its
    // position: SQLite steps over the lane's earlier places and reads no
    // track.
    for (at, offset) in offsets(n) {
        let timing = median(store, move |sql| {
            let key = super::queue_upcoming::upcoming_key_at_on(sql, false, offset)?;
            Ok(usize::from(key.is_some()))
        })
        .await;
        report("queue_upcoming", "lane", "jump", at, offset, n, timing);
    }
}
