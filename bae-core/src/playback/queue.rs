use std::collections::{HashSet, VecDeque};

use tracing::warn;

use super::RepeatMode;
use crate::id_provider::IdRef;
use crate::playback::context::{shuffled_traversal, ContextStart, PlaybackContext, Traversal};

/// Per-instance identity for an enqueued track. Distinct from `track_id`: the
/// same track enqueued twice yields two entries with two ids, each removable,
/// reorderable, and skippable independently. Minted from the injected
/// `IdProvider` on enqueue.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueueEntryId(pub String);

/// One enqueued instance: a stable per-instance id plus the track it plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: QueueEntryId,
    pub track_id: String,
}

/// What to do when advancing to the next track.
#[derive(Debug)]
pub enum NextEntry {
    /// Repeat the current track (`RepeatMode::Track`).
    RepeatCurrent(String),
    /// Play this track next (drained from the manual lane or the context).
    Play(String),
    /// Nothing left to play.
    Stop,
}

/// What to do when going to the previous track.
pub enum PreviousAction {
    /// Go back to this track.
    PlayPrevious(String),
    /// Restart the current track (past the 3s threshold, or nothing before it).
    RestartCurrent,
}

/// Where an operable id sits: the manual lane, or a position in the context.
enum EntryLocation {
    Manual(usize),
    Context(usize),
}

/// The persistable view of a playing context (its release and how it's ordered),
/// without the per-instance entry ids — those are local UI identity, re-minted on
/// restore. Re-materialized from `source` + `traversal` on load.
pub struct ContextSnapshot {
    pub source: String,
    pub traversal: Traversal,
    pub cursor: usize,
}

/// The persistable view of the whole queue. The service pairs this with the live
/// position / volume / mute to build the `playback_state` row, and on restart
/// reads the row back into this shape and fetches the context's tracks before
/// calling [`PlaybackQueue::restore`].
pub struct QueueSnapshot {
    /// The playing context, or `None` (a single track, or nothing playing).
    pub context: Option<ContextSnapshot>,
    /// The manual lane as track ids in order.
    pub manual: Vec<String>,
    /// The track currently playing (resume anchor).
    pub current_track_id: Option<String>,
    pub repeat: RepeatMode,
}

/// The insert index for a reorder, after the source entry at `from` has been
/// removed from a lane of post-removal length `len`: `before = None` appends at
/// the end; a `before` position past `from` shifts down by one.
fn before_insert_index(before_pos: Option<usize>, from: usize, len: usize) -> usize {
    match before_pos {
        Some(pos) if pos > from => pos - 1,
        Some(pos) => pos,
        None => len,
    }
}

/// Pure data structure for the playback queue: a **manual lane** (explicitly
/// enqueued tracks, drained first) and a **context** (the release being played
/// from, traversed by a cursor). The current instance is whichever lane produced
/// it. Holds no I/O — the service feeds it track ids it fetched.
pub struct PlaybackQueue {
    /// Explicitly enqueued instances (Play Next / Add to Queue), drained first.
    manual: VecDeque<QueueEntry>,
    /// The release being played from, or `None` when nothing is.
    context: Option<PlaybackContext>,
    /// The now-playing instance, from whichever lane produced it.
    current: Option<QueueEntry>,
    repeat: RepeatMode,
    ids: IdRef,
}

impl PlaybackQueue {
    pub fn new(ids: IdRef) -> Self {
        Self {
            manual: VecDeque::new(),
            context: None,
            current: None,
            repeat: RepeatMode::Off,
            ids,
        }
    }

    /// Wrap a track id in a fresh `QueueEntry` with a newly minted id.
    fn mint(&self, track_id: String) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId(self.ids.new_id()),
            track_id,
        }
    }

    /// Locate an operable id in the manual lane or the context.
    fn locate(&self, id: &QueueEntryId) -> Option<EntryLocation> {
        if let Some(pos) = self.manual.iter().position(|e| &e.id == id) {
            return Some(EntryLocation::Manual(pos));
        }
        if let Some(ctx) = self.context.as_ref() {
            if let Some(pos) = ctx.entries.iter().position(|e| &e.id == id) {
                return Some(EntryLocation::Context(pos));
            }
        }
        None
    }

    // -- Manual lane mutations -------------------------------------------------

    /// Add track ids to the end of the manual lane, minting a fresh id each.
    pub fn add_to_queue(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids {
            let entry = self.mint(track_id);
            self.manual.push_back(entry);
        }
    }

    /// Add track ids to the front of the manual lane (play next), minting ids.
    pub fn add_next(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids.into_iter().rev() {
            let entry = self.mint(track_id);
            self.manual.push_front(entry);
        }
    }

    /// Insert track ids at a position in the manual lane, minting ids.
    pub fn insert_at(&mut self, index: usize, track_ids: Vec<String>) {
        let pos = index.min(self.manual.len());
        for (i, track_id) in track_ids.into_iter().enumerate() {
            let entry = self.mint(track_id);
            self.manual.insert(pos + i, entry);
        }
    }

    /// Clear the manual lane. The context (the release being played from) is
    /// left intact — matching "Clear" leaving "Continue Playing".
    pub fn clear(&mut self) {
        self.manual.clear();
    }

    // -- Context ---------------------------------------------------------------

    /// Build a `PlaybackContext` from a release's tracks under a traversal: a
    /// shuffled traversal re-permutes `tracks` from its seed (the same seed
    /// yields the same order, so a restored shuffle reproduces what was played);
    /// a sequential one keeps source order. Mints a fresh per-instance id per
    /// track and clamps the cursor in range. The caller passes a non-empty
    /// release, so the context is non-empty with a valid cursor.
    fn build_context(
        &self,
        source: String,
        mut tracks: Vec<String>,
        cursor: usize,
        traversal: Traversal,
    ) -> PlaybackContext {
        if let Traversal::Shuffled { seed } = traversal {
            shuffled_traversal(&mut tracks, seed);
        }
        let entries: Vec<QueueEntry> = tracks.into_iter().map(|t| self.mint(t)).collect();
        let cursor = cursor.min(entries.len() - 1);
        PlaybackContext {
            source,
            entries,
            cursor,
            traversal,
        }
    }

    /// Make a release's tracks the playing context: build their order under
    /// `start`, set the cursor, and make the cursor entry current. Clears the
    /// manual lane (a fresh playback session). Returns the track to play. The
    /// caller passes a non-empty release (and an in-range `Index`); the context
    /// is therefore non-empty with a valid cursor.
    pub fn play_release(
        &mut self,
        release_id: String,
        track_ids: Vec<String>,
        start: ContextStart,
    ) -> String {
        self.manual.clear();
        let (cursor, traversal) = match start {
            ContextStart::Index(index) => (index, Traversal::Sequential),
            ContextStart::Shuffled { seed } => (0, Traversal::Shuffled { seed }),
        };
        let context = self.build_context(release_id, track_ids, cursor, traversal);
        let track = context.current().track_id.clone();
        self.current = Some(context.current().clone());
        self.context = Some(context);
        track
    }

    /// Play a single track with no surrounding context — nothing is queued after
    /// it. Used when a track's release can't be loaded; clears the manual lane.
    pub fn play_single(&mut self, track_id: String) {
        self.manual.clear();
        self.context = None;
        self.current = Some(self.mint(track_id));
    }

    // -- Persistence -----------------------------------------------------------

    /// The persistable view of the queue (track ids and ordering, no entry ids).
    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            context: self.context.as_ref().map(|ctx| ContextSnapshot {
                source: ctx.source.clone(),
                traversal: ctx.traversal,
                cursor: ctx.cursor,
            }),
            manual: self.manual.iter().map(|e| e.track_id.clone()).collect(),
            current_track_id: self.current.as_ref().map(|e| e.track_id.clone()),
            repeat: self.repeat,
        }
    }

    /// Rebuild from a snapshot. `context_tracks` is the source release's tracks in
    /// SOURCE order (the service fetched them from the snapshot's context source);
    /// empty when there is no context. Re-materializes the context order under the
    /// stored traversal — a shuffled order re-derives identically from its seed, so
    /// the cursor still lands on the same track — re-instantiates the manual lane
    /// with fresh entry ids, restores repeat, and sets `current` from
    /// `current_track_id`.
    pub fn restore(&mut self, snapshot: QueueSnapshot, context_tracks: Vec<String>) {
        self.repeat = snapshot.repeat;
        self.manual = snapshot.manual.into_iter().map(|t| self.mint(t)).collect();
        self.context = match snapshot.context {
            Some(cs) if !context_tracks.is_empty() => {
                Some(self.build_context(cs.source, context_tracks, cs.cursor, cs.traversal))
            }
            _ => None,
        };
        // The playing track is a manual entry, the context's cursor entry, or a
        // standalone single track.
        self.current = snapshot.current_track_id.map(|tid| {
            let from_manual = self.manual.iter().find(|e| e.track_id == tid).cloned();
            let from_context = self
                .context
                .as_ref()
                .filter(|c| c.current().track_id == tid)
                .map(|c| c.current().clone());
            from_manual
                .or(from_context)
                .unwrap_or_else(|| self.mint(tid))
        });
    }

    // -- id-based operations across both lanes ---------------------------------

    /// Remove the entry with the given id from whichever lane holds it. The
    /// context cursor stays on the same playing track (a removal before it shifts
    /// it down; a removal at it leaves it on the track that shifted into place,
    /// clamped in bounds); emptying the context drops it. Removing the entry that
    /// is currently playing clears `current`. Unknown id logs and no-ops.
    pub fn remove(&mut self, id: &QueueEntryId) -> Option<QueueEntry> {
        let removed = match self.locate(id) {
            Some(EntryLocation::Manual(pos)) => Some(
                self.manual
                    .remove(pos)
                    .expect("locate reported a manual entry"),
            ),
            Some(EntryLocation::Context(pos)) => {
                let ctx = self
                    .context
                    .as_mut()
                    .expect("locate reported a context entry");
                let removed = ctx.entries.remove(pos);
                if pos < ctx.cursor {
                    ctx.cursor -= 1;
                }
                if ctx.entries.is_empty() {
                    self.context = None;
                } else {
                    // A removal at or after the cursor can leave it past the end.
                    ctx.cursor = ctx.cursor.min(ctx.entries.len() - 1);
                }
                Some(removed)
            }
            None => {
                warn!("remove: queue entry id not found: {}", id.0);
                None
            }
        };
        if let Some(ref removed) = removed {
            if self.current.as_ref().is_some_and(|c| c.id == removed.id) {
                self.current = None;
            }
        }
        removed
    }

    /// Move the entry `id` to sit immediately before `before`; `before = None`
    /// moves it to the end of its lane. Reordering is within one lane — the
    /// manual lane or the context order — so `id` and `before` must be in the
    /// same lane; otherwise this logs and no-ops. (Reordering the context order
    /// mutates the order initially set by the release/shuffle; the cursor stays
    /// on the same playing track.)
    pub fn reorder(&mut self, id: &QueueEntryId, before: Option<&QueueEntryId>) {
        match self.locate(id) {
            Some(EntryLocation::Manual(from)) => {
                let before_pos = match before {
                    Some(before_id) => match self.manual.iter().position(|e| &e.id == before_id) {
                        Some(pos) => Some(pos),
                        None => {
                            warn!("reorder: before id not in the manual lane: {}", before_id.0);
                            return;
                        }
                    },
                    None => None,
                };
                // `from` is in bounds; reorder-before-self falls through to a
                // remove-and-reinsert at the same position — a correct no-op.
                let entry = self
                    .manual
                    .remove(from)
                    .expect("reorder: located index in bounds");
                let insert_at = before_insert_index(before_pos, from, self.manual.len());
                self.manual.insert(insert_at, entry);
            }
            Some(EntryLocation::Context(from)) => {
                let ctx = self
                    .context
                    .as_mut()
                    .expect("locate reported a context entry");
                let before_pos = match before {
                    Some(before_id) => match ctx.entries.iter().position(|e| &e.id == before_id) {
                        Some(pos) => Some(pos),
                        None => {
                            warn!("reorder: before id not in the context: {}", before_id.0);
                            return;
                        }
                    },
                    None => None,
                };
                // Keep the cursor on the same playing track across the move.
                let cursor_id = ctx.current().id.clone();
                let entry = ctx.entries.remove(from);
                let insert_at = before_insert_index(before_pos, from, ctx.entries.len());
                ctx.entries.insert(insert_at, entry);
                ctx.cursor = ctx
                    .entries
                    .iter()
                    .position(|e| e.id == cursor_id)
                    .expect("reorder: the cursor entry is still present after reinsert");
            }
            None => warn!("reorder: queue entry id not found: {}", id.0),
        }
    }

    /// Skip to the entry with the given id and make it current. A manual-lane
    /// target drains the entries ahead of it in the manual lane; a context target
    /// moves the cursor to it. Unknown id logs and no-ops; returns the entry now
    /// playing.
    pub fn skip_to(&mut self, id: &QueueEntryId) -> Option<QueueEntry> {
        match self.locate(id) {
            Some(EntryLocation::Manual(pos)) => {
                for _ in 0..pos {
                    self.manual.pop_front();
                }
                let entry = self
                    .manual
                    .pop_front()
                    .expect("locate reported a manual entry");
                self.current = Some(entry.clone());
                Some(entry)
            }
            Some(EntryLocation::Context(pos)) => {
                self.play_context_at(pos);
                self.current.clone()
            }
            None => {
                warn!("skip_to: queue entry id not found: {}", id.0);
                None
            }
        }
    }

    /// Remove every entry whose track id is in the set, from the manual lane and
    /// the context (a deleted track leaves everywhere it sits). Clears `current`
    /// if it matches. Keeps the cursor pointing at the same context track by
    /// counting how many removed entries sat before it.
    pub fn remove_by_ids(&mut self, ids: &HashSet<String>) {
        self.manual.retain(|entry| !ids.contains(&entry.track_id));
        if let Some(ctx) = self.context.as_mut() {
            // The cursor is in range (a present context is non-empty with a valid
            // cursor), so `..cursor` is a valid slice.
            let before_cursor = ctx.entries[..ctx.cursor]
                .iter()
                .filter(|e| ids.contains(&e.track_id))
                .count();
            ctx.entries.retain(|entry| !ids.contains(&entry.track_id));
            if ctx.entries.is_empty() {
                self.context = None;
            } else {
                ctx.cursor = ctx
                    .cursor
                    .saturating_sub(before_cursor)
                    .min(ctx.entries.len() - 1);
            }
        }
        if let Some(ref current) = self.current {
            if ids.contains(&current.track_id) {
                self.current = None;
            }
        }
    }

    // -- Advance / retreat -----------------------------------------------------

    /// Move the context cursor to `cursor`, make that entry current, and return
    /// its track id. The context is present and `cursor` is in range.
    fn play_context_at(&mut self, cursor: usize) -> String {
        let ctx = self
            .context
            .as_mut()
            .expect("play_context_at: context present");
        ctx.cursor = cursor;
        let entry = ctx.current().clone();
        let track = entry.track_id.clone();
        self.current = Some(entry);
        track
    }

    /// Decide the next track: repeat-track pins the current; otherwise drain the
    /// manual lane, then advance the context cursor, then (under `Context` repeat)
    /// loop the context from its start, else stop. Mutates `current` and the
    /// cursor to reflect what now plays.
    pub fn next_entry(&mut self) -> NextEntry {
        if self.repeat == RepeatMode::Track {
            if let Some(ref cur) = self.current {
                return NextEntry::RepeatCurrent(cur.track_id.clone());
            }
        }

        if let Some(track) = self.advance_to_front() {
            return NextEntry::Play(track);
        }

        // The manual lane and the context tail are both exhausted. Under
        // `Context` repeat, loop the (non-empty) context from its start, re-
        // deriving a shuffled order with a fresh seed so each pass differs.
        if self.repeat == RepeatMode::Context && self.context.is_some() {
            self.rederive_context();
            return NextEntry::Play(self.play_context_at(0));
        }

        NextEntry::Stop
    }

    /// Re-derive the context's order for a fresh repeat pass: a shuffled context
    /// re-permutes its entries with the next seed so the loop is a different
    /// order; a sequential one is left as-is. The cursor is reset to the start by
    /// the `play_context_at(0)` that follows.
    fn rederive_context(&mut self) {
        let ctx = self
            .context
            .as_mut()
            .expect("rederive_context: context present");
        if let Traversal::Shuffled { seed } = ctx.traversal {
            ctx.traversal = shuffled_traversal(&mut ctx.entries, seed.wrapping_add(1));
        }
    }

    /// Advance directly to the upcoming front and make it current, bypassing
    /// repeat-track and the context-repeat loop — used by the preload path, where
    /// the front track is already buffered. Pops the manual front or advances the
    /// context cursor. Returns the track now playing, or `None` if nothing is
    /// queued ahead.
    pub fn advance_to_front(&mut self) -> Option<String> {
        if let Some(entry) = self.manual.pop_front() {
            let track = entry.track_id.clone();
            self.current = Some(entry);
            return Some(track);
        }
        let next = {
            let ctx = self.context.as_ref()?;
            if ctx.cursor + 1 < ctx.entries.len() {
                ctx.cursor + 1
            } else {
                return None;
            }
        };
        Some(self.play_context_at(next))
    }

    /// Decide the previous action. Within 3s of the start, step the context cursor
    /// back (multi-step over the traversed order); otherwise restart the current
    /// track. When the current track is a manual item, the cursor entry is the
    /// context track that preceded it, so stepping back lands there.
    pub fn previous_action(&mut self, position_ms: u64) -> PreviousAction {
        if position_ms >= 3000 {
            return PreviousAction::RestartCurrent;
        }
        let target = {
            let Some(ctx) = self.context.as_ref() else {
                return PreviousAction::RestartCurrent;
            };
            // A present context is non-empty with a valid cursor, so the cursor
            // entry is real. If the current track is that cursor entry, step back
            // one; if it's a manual item, the cursor entry is the context track
            // that preceded it, so land there.
            let current_is_cursor = self.current.as_ref().map(|c| &c.id) == Some(&ctx.current().id);
            if current_is_cursor {
                match ctx.cursor.checked_sub(1) {
                    Some(t) => t,
                    None => return PreviousAction::RestartCurrent,
                }
            } else {
                ctx.cursor
            }
        };
        PreviousAction::PlayPrevious(self.play_context_at(target))
    }

    // -- Projection / accessors ------------------------------------------------

    /// The flat upcoming list the UI renders: the manual lane, then the
    /// not-yet-played tail of the context.
    pub fn upcoming(&self) -> Vec<QueueEntry> {
        let mut out: Vec<QueueEntry> = self.manual.iter().cloned().collect();
        if let Some(ctx) = self.context.as_ref() {
            out.extend(ctx.upcoming().iter().cloned());
        }
        out
    }

    /// Whether anything is queued to play after the current track (ignoring
    /// repeat) — i.e. there is an upcoming front.
    pub fn has_upcoming(&self) -> bool {
        self.front().is_some()
    }

    /// Whether stepping back is possible: a context track sits before the cursor.
    pub fn has_previous(&self) -> bool {
        self.context.as_ref().is_some_and(|ctx| ctx.cursor > 0)
    }

    /// The first upcoming track id, for preload — without consuming it.
    pub fn front(&self) -> Option<&str> {
        if let Some(entry) = self.manual.front() {
            return Some(entry.track_id.as_str());
        }
        self.context
            .as_ref()
            .and_then(|ctx| ctx.upcoming().first().map(|e| e.track_id.as_str()))
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    pub fn current_track_id(&self) -> Option<&str> {
        self.current.as_ref().map(|e| e.track_id.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id_provider::SequentialIdProvider;
    use std::sync::Arc;

    fn queue() -> PlaybackQueue {
        PlaybackQueue::new(Arc::new(SequentialIdProvider::new("entry")))
    }

    /// The upcoming projection's track ids in order — the per-instance ids vary,
    /// so most assertions are about which tracks sit where.
    fn upcoming_tracks(q: &PlaybackQueue) -> Vec<String> {
        q.upcoming().into_iter().map(|e| e.track_id).collect()
    }

    /// The full play order from the current track onward: the current track then
    /// the upcoming projection.
    fn full_order(q: &PlaybackQueue) -> Vec<String> {
        let mut order = vec![q.current_track_id().unwrap().to_string()];
        order.extend(upcoming_tracks(q));
        order
    }

    fn manual_ids(q: &PlaybackQueue) -> Vec<QueueEntryId> {
        q.manual.iter().map(|e| e.id.clone()).collect()
    }

    fn rel(tracks: &[&str]) -> Vec<String> {
        tracks.iter().map(|s| s.to_string()).collect()
    }

    // -- manual lane -----------------------------------------------------------

    #[test]
    fn test_add_to_queue_fills_manual_lane() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c"]));
        assert_eq!(upcoming_tracks(&q), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_add_to_queue_mints_distinct_ids() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "a"]));
        let ids = manual_ids(&q);
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "duplicate tracks get distinct ids");
    }

    #[test]
    fn test_add_next_preserves_order() {
        let mut q = queue();
        q.add_to_queue(rel(&["x"]));
        q.add_next(rel(&["a", "b"]));
        assert_eq!(upcoming_tracks(&q), vec!["a", "b", "x"]);
    }

    #[test]
    fn test_remove_by_entry_id() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c"]));
        let b_id = manual_ids(&q)[1].clone();
        let removed = q.remove(&b_id);
        assert_eq!(removed.map(|e| e.track_id), Some("b".into()));
        assert_eq!(upcoming_tracks(&q), vec!["a", "c"]);
    }

    /// The load-bearing dup test: the same track enqueued twice, removing one
    /// instance by its id leaves the other instance — and its id — intact.
    #[test]
    fn test_remove_one_duplicate_keeps_the_other() {
        let mut q = queue();
        q.add_to_queue(rel(&["dup", "dup"]));
        let ids = manual_ids(&q);
        let removed = q.remove(&ids[0]).expect("first instance removed");
        assert_eq!(removed.id, ids[0]);

        let remaining = manual_ids(&q);
        assert_eq!(remaining.len(), 1, "exactly one instance remains");
        assert_eq!(remaining[0], ids[1], "the other instance's id survives");
    }

    #[test]
    fn test_remove_unknown_id_is_noop() {
        let mut q = queue();
        q.add_to_queue(rel(&["a"]));
        assert_eq!(q.remove(&QueueEntryId("nope".into())), None);
        assert_eq!(upcoming_tracks(&q), vec!["a"]);
    }

    #[test]
    fn test_reorder_forward() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c", "d"]));
        let ids = manual_ids(&q);
        q.reorder(&ids[0], Some(&ids[2]));
        assert_eq!(upcoming_tracks(&q), vec!["b", "a", "c", "d"]);
    }

    #[test]
    fn test_reorder_to_end() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c", "d"]));
        let ids = manual_ids(&q);
        q.reorder(&ids[0], None);
        assert_eq!(upcoming_tracks(&q), vec!["b", "c", "d", "a"]);
    }

    #[test]
    fn test_reorder_backward() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c", "d"]));
        let ids = manual_ids(&q);
        q.reorder(&ids[2], Some(&ids[0]));
        assert_eq!(upcoming_tracks(&q), vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn test_reorder_before_self_is_noop() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c"]));
        let ids = manual_ids(&q);
        q.reorder(&ids[1], Some(&ids[1]));
        assert_eq!(upcoming_tracks(&q), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_reorder_unknown_source_is_noop() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b"]));
        q.reorder(&QueueEntryId("nope".into()), None);
        assert_eq!(upcoming_tracks(&q), vec!["a", "b"]);
    }

    #[test]
    fn test_clear_empties_manual_keeps_context() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        q.add_to_queue(rel(&["m1", "m2"]));
        q.clear();
        // Manual gone; context tail (t2, t3) survives.
        assert_eq!(upcoming_tracks(&q), vec!["t2", "t3"]);
    }

    #[test]
    fn test_skip_to_manual_drains_prefix() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c", "d"]));
        let c_id = manual_ids(&q)[2].clone();
        let entry = q.skip_to(&c_id);
        assert_eq!(entry.map(|e| e.track_id), Some("c".into()));
        assert_eq!(upcoming_tracks(&q), vec!["d"]);
        assert_eq!(q.current_track_id(), Some("c"));
    }

    #[test]
    fn test_skip_to_unknown_id_is_noop() {
        let mut q = queue();
        q.add_to_queue(rel(&["a"]));
        assert_eq!(q.skip_to(&QueueEntryId("nope".into())), None);
        assert_eq!(upcoming_tracks(&q), vec!["a"]);
    }

    // -- context ---------------------------------------------------------------

    #[test]
    fn test_play_release_sets_current_and_upcoming() {
        let mut q = queue();
        let first = q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        assert_eq!(first, "t1");
        assert_eq!(q.current_track_id(), Some("t1"));
        assert_eq!(upcoming_tracks(&q), vec!["t2", "t3"]);
    }

    #[test]
    fn test_play_release_start_index() {
        let mut q = queue();
        let first = q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(1),
        );
        assert_eq!(first, "t2");
        assert_eq!(upcoming_tracks(&q), vec!["t3"]);
    }

    #[test]
    fn test_play_release_shuffled_keeps_all_tracks() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3", "t4"]),
            ContextStart::Shuffled { seed: 7 },
        );
        let mut all = full_order(&q);
        all.sort();
        assert_eq!(all, vec!["t1", "t2", "t3", "t4"]);
    }

    #[test]
    fn test_play_release_shuffled_is_deterministic_for_a_seed() {
        let order = |seed| {
            let mut q = queue();
            q.play_release(
                "r1".into(),
                rel(&["t1", "t2", "t3", "t4", "t5"]),
                ContextStart::Shuffled { seed },
            );
            full_order(&q)
        };
        assert_eq!(order(42), order(42), "same seed yields the same order");
    }

    /// A repeating shuffled context loops a freshly re-derived order each pass,
    /// not the same order every time. Both passes' orders are read from the queue
    /// (no re-implementation of the shuffle) and are deterministic for a fixed
    /// seed.
    #[test]
    fn test_context_repeat_shuffled_loops_a_re_derived_order() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3", "t4", "t5"]),
            ContextStart::Shuffled { seed: 1 },
        );
        q.set_repeat_mode(RepeatMode::Context);
        let first_pass = full_order(&q);
        // Advance to the end of the pass, then one more advance loops it.
        for _ in 0..first_pass.len() - 1 {
            q.next_entry();
        }
        match q.next_entry() {
            NextEntry::Play(_) => {}
            other => panic!("expected a looped Play, got {other:?}"),
        }
        let second_pass = full_order(&q);

        let (mut a, mut b) = (first_pass.clone(), second_pass.clone());
        a.sort();
        b.sort();
        assert_eq!(a, b, "the loop replays exactly the same tracks");
        assert_ne!(
            first_pass, second_pass,
            "but in a freshly re-derived order each pass"
        );
    }

    // -- persistence round-trips -----------------------------------------------

    #[test]
    fn test_snapshot_restore_sequential_context() {
        let mut q = queue();
        q.play_release(
            "rel-A".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(1),
        );
        let snap = q.snapshot();
        assert_eq!(snap.context.as_ref().unwrap().source, "rel-A");
        assert_eq!(snap.current_track_id.as_deref(), Some("t2"));

        let mut restored = queue();
        restored.restore(snap, rel(&["t1", "t2", "t3"]));
        assert_eq!(restored.current_track_id(), Some("t2"));
        assert_eq!(upcoming_tracks(&restored), vec!["t3"]);
        assert!(
            restored.has_previous(),
            "the cursor (1) restored, so t1 is behind"
        );
    }

    #[test]
    fn test_snapshot_restore_shuffled_keeps_cursor_on_same_track() {
        let mut q = queue();
        q.play_release(
            "rel-A".into(),
            rel(&["t1", "t2", "t3", "t4", "t5"]),
            ContextStart::Shuffled { seed: 99 },
        );
        q.next_entry();
        q.next_entry();
        let current_before = q.current_track_id().unwrap().to_string();
        let upcoming_before = upcoming_tracks(&q);

        // Restore from the SOURCE order; re-deriving from the same seed reproduces
        // the shuffled order, so the cursor lands on the same track.
        let mut restored = queue();
        restored.restore(q.snapshot(), rel(&["t1", "t2", "t3", "t4", "t5"]));
        assert_eq!(restored.current_track_id().unwrap(), current_before);
        assert_eq!(upcoming_tracks(&restored), upcoming_before);
    }

    #[test]
    fn test_snapshot_restore_single_track_with_manual_lane() {
        let mut q = queue();
        q.play_single("solo".into());
        q.add_to_queue(rel(&["m1", "m2"]));
        let snap = q.snapshot();
        assert!(snap.context.is_none());

        let mut restored = queue();
        restored.restore(snap, vec![]);
        assert_eq!(restored.current_track_id(), Some("solo"));
        assert_eq!(upcoming_tracks(&restored), vec!["m1", "m2"]);
    }

    #[test]
    fn test_manual_drains_before_context() {
        let mut q = queue();
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(0));
        q.add_to_queue(rel(&["m1"]));
        // current = t1; next drains manual (m1) before advancing the context.
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t2"));
        assert!(matches!(q.next_entry(), NextEntry::Stop));
    }

    #[test]
    fn test_context_advances_by_cursor() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t2"));
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t3"));
        assert!(matches!(q.next_entry(), NextEntry::Stop));
    }

    #[test]
    fn test_context_repeat_loops_from_stored_order() {
        // The queue holds the context order, so looping reuses it: the queue has
        // no library access, so it structurally cannot re-fetch.
        let mut q = queue();
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(0));
        q.set_repeat_mode(RepeatMode::Context);
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t2"));
        // Exhausted under Context repeat → loop from the start of the same order.
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t1"));
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t2"));
    }

    #[test]
    fn test_repeat_track_pins_current() {
        let mut q = queue();
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(0));
        q.set_repeat_mode(RepeatMode::Track);
        assert!(matches!(q.next_entry(), NextEntry::RepeatCurrent(t) if t == "t1"));
    }

    #[test]
    fn test_previous_steps_cursor_back_multiple() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        q.next_entry(); // t2
        q.next_entry(); // t3
        assert!(matches!(q.previous_action(1000), PreviousAction::PlayPrevious(t) if t == "t2"));
        assert!(matches!(q.previous_action(1000), PreviousAction::PlayPrevious(t) if t == "t1"));
        // At the context start, Previous restarts.
        assert!(matches!(
            q.previous_action(1000),
            PreviousAction::RestartCurrent
        ));
    }

    #[test]
    fn test_previous_past_3s_restarts() {
        let mut q = queue();
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(1));
        assert!(matches!(
            q.previous_action(5000),
            PreviousAction::RestartCurrent
        ));
    }

    #[test]
    fn test_skip_to_context_tail_moves_cursor() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3", "t4"]),
            ContextStart::Index(0),
        );
        let t3_id = q.upcoming()[1].id.clone(); // upcoming = [t2, t3, t4]
        let entry = q.skip_to(&t3_id);
        assert_eq!(entry.map(|e| e.track_id), Some("t3".into()));
        assert_eq!(q.current_track_id(), Some("t3"));
        assert_eq!(upcoming_tracks(&q), vec!["t4"]);
    }

    #[test]
    fn test_remove_context_tail_entry() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        let t2_id = q.upcoming()[0].id.clone();
        let removed = q.remove(&t2_id);
        assert_eq!(removed.map(|e| e.track_id), Some("t2".into()));
        assert_eq!(upcoming_tracks(&q), vec!["t3"]);
    }

    #[test]
    fn test_reorder_context_tail_keeps_cursor() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "t2", "t3", "t4"]),
            ContextStart::Index(0),
        );
        let up = q.upcoming(); // [t2, t3, t4]
        let t2_id = up[0].id.clone();
        let t3_id = up[1].id.clone();
        // Move t3 before t2 → upcoming becomes [t3, t2, t4].
        q.reorder(&t3_id, Some(&t2_id));
        assert_eq!(upcoming_tracks(&q), vec!["t3", "t2", "t4"]);
        assert_eq!(
            q.current_track_id(),
            Some("t1"),
            "the cursor stays on the playing track"
        );
    }

    #[test]
    fn test_reorder_cross_lane_is_noop() {
        let mut q = queue();
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(0));
        q.add_to_queue(rel(&["m1"]));
        let manual_id = manual_ids(&q)[0].clone();
        let context_id = q
            .upcoming()
            .into_iter()
            .find(|e| e.track_id == "t2")
            .unwrap()
            .id;
        // Manual source, context target → no-op (can't cross lanes).
        q.reorder(&manual_id, Some(&context_id));
        assert_eq!(upcoming_tracks(&q), vec!["m1", "t2"]);
    }

    #[test]
    fn test_repeat_mode_default_is_off() {
        let q = queue();
        assert_eq!(q.repeat_mode(), RepeatMode::Off);
    }

    #[test]
    fn test_insert_at_middle() {
        let mut q = queue();
        q.add_to_queue(rel(&["a", "b", "c"]));
        q.insert_at(1, rel(&["x", "y"]));
        assert_eq!(upcoming_tracks(&q), vec!["a", "x", "y", "b", "c"]);
    }

    #[test]
    fn test_insert_at_beyond_end_clamps() {
        let mut q = queue();
        q.add_to_queue(rel(&["a"]));
        q.insert_at(999, rel(&["x"]));
        assert_eq!(upcoming_tracks(&q), vec!["a", "x"]);
    }

    // -- remove_by_ids (library deletion) --------------------------------------

    #[test]
    fn test_remove_by_ids_clears_manual_and_context() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["t1", "dup", "t3"]),
            ContextStart::Index(0),
        );
        q.add_to_queue(rel(&["dup", "m2"]));
        let ids: HashSet<String> = ["dup"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        // Manual "dup" gone (m2 stays); context "dup" gone (t1, t3 stay).
        assert_eq!(upcoming_tracks(&q), vec!["m2", "t3"]);
    }

    #[test]
    fn test_remove_by_ids_keeps_cursor_on_same_track() {
        let mut q = queue();
        q.play_release(
            "r1".into(),
            rel(&["gone", "t2", "t3"]),
            ContextStart::Index(1),
        );
        // current = t2 (cursor 1). Deleting t1 (before cursor) keeps current at t2.
        let ids: HashSet<String> = ["gone"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.current_track_id(), Some("t2"));
        assert_eq!(upcoming_tracks(&q), vec!["t3"]);
    }

    #[test]
    fn test_remove_by_ids_clears_current_when_deleted() {
        let mut q = queue();
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(0));
        let ids: HashSet<String> = ["t1"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.current_track_id(), None);
    }

    #[test]
    fn test_remove_by_ids_deleting_current_last_entry_keeps_cursor_valid() {
        let mut q = queue();
        // current = t2 at the last position (cursor == len-1).
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(1));
        let ids: HashSet<String> = ["t2"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(
            q.current_track_id(),
            None,
            "the deleted playing track clears current"
        );
        // The cursor must not be stranded at == len: Previous must not panic.
        assert!(matches!(
            q.previous_action(1000),
            PreviousAction::PlayPrevious(t) if t == "t1"
        ));
    }

    #[test]
    fn test_front_peeks_without_consuming() {
        let mut q = queue();
        assert_eq!(q.front(), None);
        q.add_to_queue(rel(&["a", "b"]));
        assert_eq!(q.front(), Some("a"));
        assert_eq!(
            upcoming_tracks(&q),
            vec!["a", "b"],
            "front must not consume"
        );
    }

    #[test]
    fn test_has_upcoming_and_has_previous() {
        let mut q = queue();
        assert!(!q.has_upcoming());
        assert!(!q.has_previous());
        q.play_release("r1".into(), rel(&["t1", "t2"]), ContextStart::Index(0));
        assert!(q.has_upcoming());
        assert!(!q.has_previous());
        q.next_entry(); // → t2
        assert!(!q.has_upcoming());
        assert!(q.has_previous());
    }
}
