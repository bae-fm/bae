use crate::playback::queue::QueueEntry;

/// How the context's track order was derived, kept so `Context` repeat can
/// re-derive it: `Sequential` replays the source order; `Shuffled` re-permutes
/// with a fresh seed each loop.
#[derive(Clone, Copy)]
pub enum Traversal {
    Sequential,
    Shuffled { seed: u64 },
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

/// The thing playback is "playing from": a release's track order, traversed by a
/// cursor.
///
/// The tracks are held as per-instance [`QueueEntry`]s so the cursor walks
/// forward (advance) and backward (Previous) over stable row ids, and `Context`
/// repeat loops the stored order without re-fetching it. A release is small, so
/// the whole order is held expanded. The release id the order came from is not a
/// field: the queue identifies tracks by `track_id` and rows by entry id, and
/// nothing keys off which release a context is.
pub struct PlaybackContext {
    /// The release's tracks in play order, each with a stable per-instance id.
    pub entries: Vec<QueueEntry>,
    /// Index into `entries` of the context track currently playing.
    pub cursor: usize,
    /// How `entries` was ordered — replayed/re-derived on `Context` repeat.
    pub traversal: Traversal,
}

impl PlaybackContext {
    /// The entry currently under the cursor. A present context is non-empty with
    /// a valid cursor (the queue only builds one from a non-empty release and
    /// keeps the cursor in range), so this is always a real entry.
    pub fn current(&self) -> &QueueEntry {
        &self.entries[self.cursor]
    }

    /// The not-yet-played tail of the context: everything after the cursor. The
    /// cursor is `< entries.len()`, so `cursor + 1 <= entries.len()` is always a
    /// valid slice bound (empty when the cursor is on the last entry).
    pub fn upcoming(&self) -> &[QueueEntry] {
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
    Traversal::Shuffled { seed }
}
