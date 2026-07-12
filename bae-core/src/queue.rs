//! Resolved types for the playback queue — the display-ready shapes the bridge
//! and UI event payloads carry. `db::get_queue_items` builds a [`QueueItem`]
//! directly from a SQL aggregate.

/// How much of the context's not-yet-played tail `resolve_queue_projection`
/// resolves eagerly. The tail is library-scaled (a `Library` source's tail is
/// every remaining library track); resolving only the first window keeps
/// `QueueUpdated` bounded regardless of library size. The rest pages in on
/// demand via `get_queue_upcoming_page`.
pub const QUEUE_UPCOMING_WINDOW: usize = 100;

/// Display-ready queue entry. `entry_id` is per-instance, so the UI keys each row
/// on a stable identity even when the same track is queued twice.
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
/// the section — a release vs the library), the first [`QUEUE_UPCOMING_WINDOW`]
/// entries of the not-yet-played tail, the tail's full length, and whether it was
/// ordered by shuffle. The display-ready counterpart of
/// [`crate::playback::ContextProjection`], carried separately from the manual lane
/// so each UI renders the two as distinct sections.
#[derive(Debug, Clone)]
pub struct ResolvedContext {
    pub source: crate::playback::ContextSource,
    pub shuffled: bool,
    /// The first [`QUEUE_UPCOMING_WINDOW`] entries of the tail — not the whole
    /// tail. Later indices are fetched on demand via `get_queue_upcoming_page`.
    pub upcoming: Vec<QueueItem>,
    /// The full length of the not-yet-played tail, including entries beyond
    /// `upcoming`. The UI renders a placeholder for every index up to this and
    /// pages in the rest as it scrolls.
    pub upcoming_total: u64,
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
    /// The `PlaybackQueue` revision this snapshot was resolved from. A UI
    /// stamps its fetched upcoming-pages with this and drops any page whose
    /// revision no longer matches — the newer `QueueUpdated` snapshot that
    /// bumped it already reset the view.
    pub revision: u64,
}

/// One page of the context's upcoming tail, fetched by offset/limit — the
/// counterpart to `ResolvedContext.upcoming` for indices past the initial
/// window. `revision` is the `PlaybackQueue` revision the page was computed
/// from; a UI drops the page if its own snapshot's revision has since moved on.
#[derive(Debug, Clone)]
pub struct ResolvedQueueUpcomingPage {
    pub revision: u64,
    pub items: Vec<QueueItem>,
}

/// Clamp an offset/limit page request against `tail`'s bounds: an offset past
/// the end yields an empty slice, and a limit extending past the end is
/// clamped to what remains. Offset 0 is the first not-yet-played entry after
/// the current track — the same coordinate space as `ResolvedContext.upcoming`.
pub fn clamp_upcoming_page(
    tail: &[crate::playback::QueueEntry],
    offset: u32,
    limit: u32,
) -> &[crate::playback::QueueEntry] {
    let start = (offset as usize).min(tail.len());
    let end = start.saturating_add(limit as usize).min(tail.len());
    &tail[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::QueueEntryId;

    fn entries(track_ids: &[&str]) -> Vec<crate::playback::QueueEntry> {
        track_ids
            .iter()
            .enumerate()
            .map(|(i, t)| crate::playback::QueueEntry {
                id: QueueEntryId(format!("e{i}")),
                track_id: t.to_string(),
            })
            .collect()
    }

    fn track_ids(page: &[crate::playback::QueueEntry]) -> Vec<&str> {
        page.iter().map(|e| e.track_id.as_str()).collect()
    }

    #[test]
    fn clamp_upcoming_page_slices_the_requested_range_in_order() {
        let tail = entries(&["t0", "t1", "t2", "t3", "t4"]);
        assert_eq!(
            track_ids(clamp_upcoming_page(&tail, 1, 2)),
            vec!["t1", "t2"]
        );
    }

    #[test]
    fn clamp_upcoming_page_offset_past_end_is_empty() {
        let tail = entries(&["t0", "t1"]);
        assert!(clamp_upcoming_page(&tail, 5, 10).is_empty());
    }

    #[test]
    fn clamp_upcoming_page_limit_past_end_clamps_to_what_remains() {
        let tail = entries(&["t0", "t1", "t2"]);
        assert_eq!(
            track_ids(clamp_upcoming_page(&tail, 1, 100)),
            vec!["t1", "t2"]
        );
    }

    #[test]
    fn clamp_upcoming_page_empty_tail_is_empty() {
        let tail: Vec<crate::playback::QueueEntry> = Vec::new();
        assert!(clamp_upcoming_page(&tail, 0, 10).is_empty());
    }
}
