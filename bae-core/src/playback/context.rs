use crate::playback::queue::{QueueEntry, QueueEntryId};

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

/// The lane's shuffle state, present exactly while the lane is shuffled — its
/// absence is what "sequential" means.
pub(crate) struct ShuffleState {
    /// Every row's id in the order the lane had at the moment shuffle turned on.
    /// Unshuffling rearranges the upcoming rows into their relative positions
    /// here. Recorded for the whole lane, not just the upcoming tail, so it stays
    /// well-defined after a repeat wrap moves played rows back into upcoming.
    pub restore_order: Vec<QueueEntryId>,
    /// Seed for the next in-place permutation of the lane. A repeat wrap
    /// re-permutes; taking the seed advances it, so each pass differs.
    pub next_seed: u64,
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

/// The thing playback is "playing from": the rows a source filled the lane with,
/// traversed by a cursor.
///
/// The rows are the single authority over what plays: every operation — shuffle,
/// removal, reorder, a repeat wrap — is surgery on `entries` themselves, and
/// nothing consults `source` again during a session. The tracks are held as
/// per-instance [`QueueEntry`]s so the cursor walks forward (advance) and
/// backward (Previous) over stable row ids. The whole lane is held expanded — a
/// release is small, and the library materializes to cheap track-id strings.
pub(crate) struct PlaybackContext {
    /// What filled this lane (a release, several, or the whole library). Read for
    /// exactly two things: the UI's section label, and the restart recipe.
    pub source: ContextSource,
    /// The lane's rows in play order, each with a stable per-instance id.
    pub entries: Vec<QueueEntry>,
    /// Index into `entries` of the context track currently playing.
    pub cursor: usize,
    /// Present exactly while the lane is shuffled; `None` is sequential.
    pub shuffle: Option<ShuffleState>,
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

/// Permute `items` in place from `seed`. The queue permutes a slice of the lane
/// (the upcoming tail, when shuffle turns on) or the whole lane (a fill, or a
/// repeat wrap's fresh pass). The permutation is stable for a seed, so a shuffled
/// order does not change until something permutes it again — which is what lets
/// Previous step back over the exact order that was played.
pub(crate) fn permute<T>(items: &mut [T], seed: u64) {
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    items.shuffle(&mut rng);
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
