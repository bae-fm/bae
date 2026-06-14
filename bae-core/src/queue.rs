//! Resolved types for the playback queue.
//!
//! `QueueItem` is the display-ready shape the bridge and UI event payloads
//! carry. The raw DB-shape aggregate is `DbQueueItem` in `crate::db::models`;
//! `LibraryManager` turns it into `QueueItem` via `resolve_queue_item`.

/// Display-ready queue entry: a track with the album/artist context the UI
/// needs, plus a pre-formatted duration label.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub track_id: String,
    pub title: String,
    pub artist_names: String,
    pub duration_ms: Option<i64>,
    pub duration_label: String,
    pub album_title: String,
    pub cover_image_id: Option<String>,
}
