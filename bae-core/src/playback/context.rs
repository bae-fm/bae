use crate::playback::queue::QueueEntry;

/// Where to place the cursor when a release becomes the playing context.
pub enum ContextStart {
    /// Start at this index into the release's track order (validated in range by
    /// the caller).
    Index(usize),
    /// Permute the release's tracks once and start at the front.
    Shuffled,
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
}

impl PlaybackContext {
    /// Build a context from a release's entries (already minted by the queue).
    /// `entries` is non-empty and an `Index` start is in range — the queue only
    /// builds a context from a non-empty release and the command layer validates
    /// the start index, so a present context always has a valid cursor.
    pub fn new(mut entries: Vec<QueueEntry>, start: ContextStart) -> Self {
        let cursor = match start {
            ContextStart::Shuffled => {
                use rand::seq::SliceRandom;
                let mut rng = rand::rng();
                entries.shuffle(&mut rng);
                0
            }
            ContextStart::Index(index) => index,
        };

        Self { entries, cursor }
    }

    /// The entry currently under the cursor. A present context is non-empty with
    /// a valid cursor (see `new`), so this is always a real entry.
    pub fn current(&self) -> &QueueEntry {
        &self.entries[self.cursor]
    }

    /// The not-yet-played tail of the context: everything after the cursor. The
    /// cursor is `< entries.len()` (see `new`), so `cursor + 1 <= entries.len()`
    /// is always a valid slice bound (empty when the cursor is on the last entry).
    pub fn upcoming(&self) -> &[QueueEntry] {
        &self.entries[self.cursor + 1..]
    }
}
