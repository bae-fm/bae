//! Resolved types for the playback queue — the display-ready shapes the bridge
//! and UI event payloads carry, read from the queue tables.

/// How much of the context's not-yet-played tail the queue value
/// resolves eagerly. The tail is library-scaled (a `Library` source's tail is
/// every remaining library track); resolving only the first window keeps
/// the queue value bounded regardless of library size. The rest is read in
/// the windows a [`crate::library::QueueUpcomingSubscription`] is asked for.
pub const QUEUE_UPCOMING_WINDOW: usize = 100;

/// Display-ready queue entry. `entry_id` is per-instance, so the UI keys each row
/// on a stable identity even when the same track is queued twice.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueItem {
    pub entry_id: String,
    pub track_id: String,
    pub display: crate::playback::TrackDisplay,
    pub duration_ms: u64,
}

/// The context lane resolved for display: what it plays from (so the UI labels
/// the section — a release vs the library), the first [`QUEUE_UPCOMING_WINDOW`]
/// entries of the not-yet-played tail, the tail's full length, and whether it was
/// ordered by shuffle. Carried separately from the manual lane so each UI
/// renders the two as distinct sections.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedContext {
    pub source: crate::playback::ContextSource,
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or the
    /// whole library (those label themselves by kind alone).
    pub source_title: Option<String>,
    pub shuffled: bool,
    /// The first [`QUEUE_UPCOMING_WINDOW`] entries of the tail — not the whole
    /// tail. Later indices are read by a
    /// [`crate::library::QueueUpcomingSubscription`].
    pub upcoming: Vec<QueueItem>,
    /// The full length of the not-yet-played tail, including entries beyond
    /// `upcoming`. The UI renders a placeholder for every index up to this and
    /// pages in the rest as it scrolls.
    pub upcoming_total: u64,
}

/// The display-ready playback queue: manual entries, the current context lane,
/// and whether the transport can step forward/back.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedQueueSnapshot {
    pub manual: Vec<QueueItem>,
    pub context: Option<ResolvedContext>,
    pub has_next: bool,
    pub has_previous: bool,
    /// The queue's revision (`playback_queue.revision`) this snapshot was read
    /// at. A UI shows the upcoming windows past the first only while they
    /// carry this same revision, since their offsets count from this queue's
    /// tail.
    pub revision: u64,
}
