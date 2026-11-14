//! Resolved types for the playback queue — the display-ready shapes the bridge
//! carries, read from the queue tables.

/// Display-ready queue entry. `entry_id` is unique across both lanes, so the UI
/// keys each row on a stable identity even when the queue holds a track twice.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub entry_id: String,
    pub track_id: String,
    pub display: crate::TrackDisplay,
    pub duration_ms: u64,
}

/// The context lane resolved for display: what it plays from (so the UI labels
/// the section — a release vs the library). Its tracks are read in windows of [`crate::QueueWindowRead`].
/// Carried separately from the manual lane so each UI renders the two as
/// distinct sections.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedContext {
    pub source: crate::ContextSource,
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or the
    /// whole library (those label themselves by kind alone).
    pub source_title: Option<String>,
}

/// The display-ready playback queue: manual entries, the current context lane,
/// and whether the transport can step forward/back.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedQueueSnapshot {
    pub manual: Vec<QueueItem>,
    pub context: Option<ResolvedContext>,
    /// Whether shuffle is on, which holds with no context too: the next play
    /// starts shuffled.
    pub shuffled: bool,
    pub has_next: bool,
    pub has_previous: bool,
    /// How many writes have changed Up Next, as the queue row counts them:
    /// a screen holding an Up Next row it moved shows the tables' order again
    /// once this reaches the version its move made.
    pub manual_version: u64,
}

/// The last time tracks went into Up Next since playback started with the
/// library: which addition it was, counting from one, and how many tracks it
/// put in. Two additions alike in their count are still two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueAddition {
    pub number: u64,
    pub tracks: u32,
}
