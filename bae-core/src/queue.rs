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

/// The context lane resolved for display: what it plays from (so the UI labels
/// the section — a release vs the library), the not-yet-played tail, and whether
/// it was ordered by shuffle (which the UI surfaces as an indicator). The
/// display-ready counterpart of [`crate::playback::ContextProjection`], with each
/// entry resolved to a [`QueueItem`]. Carried separately from the manual lane so
/// each UI renders the two as distinct sections.
#[derive(Debug, Clone)]
pub struct ResolvedContext {
    pub source: crate::playback::ContextSource,
    pub shuffled: bool,
    pub upcoming: Vec<QueueItem>,
}

/// The display-ready playback queue: manual entries, the current context lane,
/// and whether the transport can step forward/back. Produced by resolving the
/// playback loop's queue projection through the library metadata layer.
#[derive(Debug, Clone)]
pub struct ResolvedQueueSnapshot {
    pub manual: Vec<QueueItem>,
    pub context: Option<ResolvedContext>,
    pub has_next: bool,
    pub has_previous: bool,
}
