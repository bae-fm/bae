use crate::playback::queue::QueueEntry;

/// What the queue is playing from: a single release (its track order), several
/// releases concatenated in the order they were chosen, or the whole library. A
/// `Release` carries the id its tracks are fetched by; `Releases` carries the
/// ordered ids of a multi-album play; `Library` has no id — its tracks are every
/// track in the library. The service dispatches the track re-fetch on this
/// (release → `get_track_ids`, releases → each release's `get_track_ids`
/// concatenated, library → `get_all_track_ids`) and persistence encodes it in the
/// resume cache's `source` column.
///
/// `Releases` always holds at least two ids: a single-release play collapses to
/// `Release` (see [`ContextSource::releases`]) so it is byte-identical to
/// `play_release`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextSource {
    Release(String),
    Releases(Vec<String>),
    Library,
}

impl ContextSource {
    /// Build a source from an ordered, non-empty set of release ids, collapsing a
    /// single id to [`ContextSource::Release`] so a one-album play behaves exactly
    /// like `play_release`. The caller guarantees a non-empty list — an empty set
    /// of releases is a no-op decided upstream, never a source.
    pub fn releases(mut ids: Vec<String>) -> ContextSource {
        debug_assert!(!ids.is_empty(), "a release source needs at least one id");
        if ids.len() == 1 {
            ContextSource::Release(ids.remove(0))
        } else {
            ContextSource::Releases(ids)
        }
    }
}

/// How the context's track order was derived, kept so `Context` repeat and
/// restore can re-derive it: `Sequential` replays the source order; `Shuffled`
/// permutes the source by `seed`, then moves `anchor` — the track that was
/// playing when shuffle was turned on — to the front, so the whole rest of the
/// source stays upcoming instead of landing behind the cursor where Next never
/// reaches. `anchor` is `None` for orders no track was fronted into (a
/// shuffled play-from-scratch, a repeat wrap's fresh pass).
#[derive(Clone)]
pub enum Traversal {
    Sequential,
    Shuffled { seed: u64, anchor: Option<String> },
}

/// How a release becomes the playing context: at a chosen track in source order,
/// or shuffled by a seed the caller generated.
pub enum ContextStart {
    /// Start at this index into the release's track order (validated in range by
    /// the caller).
    Index(usize),
    /// Permute the release's tracks with this seed and start at the front.
    Shuffled { seed: u64 },
}

/// The thing playback is "playing from": a source's track order, traversed by a
/// cursor.
///
/// The tracks are held as per-instance [`QueueEntry`]s so the cursor walks
/// forward (advance) and backward (Previous) over stable row ids, and `Context`
/// repeat loops the stored order without re-fetching it. The whole order is held
/// expanded — a release is small, and the library materializes to cheap track-id
/// strings.
pub(crate) struct PlaybackContext {
    /// What this order came from (a release, or the whole library). Read by
    /// persistence and the shuffle/repeat re-derive to re-fetch the tracks and
    /// re-materialize the order.
    pub source: ContextSource,
    /// The release's tracks in play order, each with a stable per-instance id.
    pub entries: Vec<QueueEntry>,
    /// Index into `entries` of the context track currently playing.
    pub cursor: usize,
    /// How `entries` was ordered — replayed/re-derived on `Context` repeat.
    pub traversal: Traversal,
    /// Tracks the user removed from this context. A shuffle toggle rebuilds
    /// the order from the re-fetched source and filters it through this set —
    /// membership alone can't distinguish a user removal from a track newly
    /// added to the source, and only the former stays out. Session-scoped: a
    /// fresh context starts empty.
    pub removed_track_ids: std::collections::HashSet<String>,
}

impl PlaybackContext {
    /// The entry currently under the cursor. A present context is non-empty with
    /// a valid cursor (the queue only builds one from a non-empty release and
    /// keeps the cursor in range), so this is always a real entry.
    pub(crate) fn current(&self) -> &QueueEntry {
        &self.entries[self.cursor]
    }

    /// The not-yet-played tail of the context: everything after the cursor. The
    /// cursor is `< entries.len()`, so `cursor + 1 <= entries.len()` is always a
    /// valid slice bound (empty when the cursor is on the last entry).
    pub(crate) fn upcoming(&self) -> &[QueueEntry] {
        &self.entries[self.cursor + 1..]
    }
}

/// Permute `items` from `seed` and report the matching traversal. The queue uses
/// this both when first shuffling a release and when re-deriving a shuffled
/// context for a repeat loop. The same seed yields the same order, so a shuffled
/// order does not change until it is re-derived — which is what lets Previous
/// step back over the exact order that was played.
pub(crate) fn shuffled_traversal<T>(items: &mut [T], seed: u64) -> Traversal {
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    items.shuffle(&mut rng);
    Traversal::Shuffled { seed, anchor: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_of_one_collapses_to_release() {
        assert_eq!(
            ContextSource::releases(vec!["only".into()]),
            ContextSource::Release("only".into())
        );
    }

    #[test]
    fn releases_of_many_keeps_input_order() {
        assert_eq!(
            ContextSource::releases(vec!["a".into(), "b".into(), "c".into()]),
            ContextSource::Releases(vec!["a".into(), "b".into(), "c".into()])
        );
    }
}
