//! Resolved types for the playback queue — the display-ready shapes the bridge
//! and UI event payloads carry, read from the queue tables.

/// Display-ready queue entry. `entry_id` is unique across both lanes, so the UI
/// keys each row on a stable identity even when the queue holds a track twice.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub entry_id: String,
    pub track_id: String,
    pub display: crate::playback::TrackDisplay,
    pub duration_ms: u64,
}

/// The context lane resolved for display: what it plays from (so the UI labels
/// the section — a release vs the library) and whether it was ordered by
/// shuffle. Its tracks are read in windows through a
/// [`crate::library::QueueUpcomingSubscription`]. Carried separately from the
/// manual lane so each UI renders the two as distinct sections.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedContext {
    pub source: crate::playback::ContextSource,
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or the
    /// whole library (those label themselves by kind alone).
    pub source_title: Option<String>,
    pub shuffled: bool,
}

/// The display-ready playback queue: manual entries, the current context lane,
/// and whether the transport can step forward/back.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedQueueSnapshot {
    pub manual: Vec<QueueItem>,
    pub context: Option<ResolvedContext>,
    pub has_next: bool,
    pub has_previous: bool,
}
