//! Resolved type for the playback queue.
//!
//! `QueueItem` is the display-ready shape the bridge and UI event payloads
//! carry: a queue entry's per-instance id plus the track's album/artist
//! context. `db::get_queue_items` builds it directly from a SQL aggregate.

/// Display-ready queue entry: a per-instance `entry_id` (so the UI keys each
/// row on a stable unique identity even when the same track is queued twice)
/// plus the track and the album/artist context the UI needs.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub entry_id: String,
    pub track_id: String,
    pub title: String,
    pub artist_names: String,
    pub duration_ms: Option<i64>,
    pub album_title: String,
    pub cover_image_id: Option<String>,
}
