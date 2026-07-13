use std::collections::{HashSet, VecDeque};

use tracing::warn;

use super::RepeatMode;
use crate::playback::context::{
    shuffled_traversal, ContextSource, ContextStart, PlaybackContext, Traversal,
};
use coven::IdRef;

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

/// The context lane projected for the UI: what it plays from (so the UI labels
/// the section — a release vs the library), its not-yet-played tail, and whether
/// it was ordered by shuffle (which the UI surfaces as an indicator). Kept
/// distinct from the manual lane so the two render as separate sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextProjection {
    pub source: ContextSource,
    pub shuffled: bool,
    pub upcoming: Vec<QueueEntry>,
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

/// The persistable view of a playing context (what it plays from and how it's
/// ordered), without the per-instance entry ids — those are local UI identity,
/// re-minted on restore. Re-materialized from `source` + `traversal` on load.
pub struct ContextSnapshot {
    pub source: ContextSource,
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
    /// Bumped by every mutation that changes the projected queue (enqueue,
    /// insert, remove, reorder, clear, advance/previous/skip-to, context
    /// set/replace, restore) — never by a read. Lets a UI that fetched a page
    /// of the upcoming tail tell whether the page still corresponds to the
    /// queue it is rendering: the page is stamped with the revision it was
    /// computed from, and a fetch answered under a since-bumped revision is
    /// stale and must be dropped. Only `PlaybackQueue` bumps it — it is the
    /// single authority on the queue's contents.
    revision: u64,
}

impl PlaybackQueue {
    pub fn new(ids: IdRef) -> Self {
        Self {
            manual: VecDeque::new(),
            context: None,
            current: None,
            repeat: RepeatMode::Off,
            ids,
            revision: 0,
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
        self.revision += 1;
    }

    /// Add track ids to the front of the manual lane (play next), minting ids.
    pub fn add_next(&mut self, track_ids: Vec<String>) {
        for track_id in track_ids.into_iter().rev() {
            let entry = self.mint(track_id);
            self.manual.push_front(entry);
        }
        self.revision += 1;
    }

    /// Insert track ids at a position in the manual lane, minting ids.
    pub fn insert_at(&mut self, index: usize, track_ids: Vec<String>) {
        let pos = index.min(self.manual.len());
        for (i, track_id) in track_ids.into_iter().enumerate() {
            let entry = self.mint(track_id);
            self.manual.insert(pos + i, entry);
        }
        self.revision += 1;
    }

    /// Clear the manual lane. The context (the release being played from) is
    /// left intact — matching "Clear" leaving "Continue Playing".
    pub fn clear(&mut self) {
        self.manual.clear();
        self.revision += 1;
    }

    // -- Context ---------------------------------------------------------------

    /// Order a source's tracks under a traversal and mint a fresh
    /// per-instance id per track: a shuffled traversal re-permutes `tracks` from
    /// its seed (the same seed yields the same order, so a restored shuffle
    /// reproduces what was played); a sequential one keeps source order. Shared
    /// by `build_context` (which pairs the order with a caller-supplied cursor)
    /// and `set_shuffle` (which derives the cursor from the playing track).
    fn materialize_entries(
        &self,
        mut tracks: Vec<String>,
        traversal: Traversal,
    ) -> Vec<QueueEntry> {
        if let Traversal::Shuffled { seed } = traversal {
            shuffled_traversal(&mut tracks, seed);
        }
        tracks.into_iter().map(|t| self.mint(t)).collect()
    }

    /// Build a `PlaybackContext` from a source's tracks under a traversal,
    /// pairing the materialized order with `cursor`. The caller passes non-empty
    /// `tracks` and an in-range `cursor` (`play_release` validates its index;
    /// `restore` validates the saved cursor against the re-fetched tracks), so the
    /// context is non-empty with a valid cursor.
    fn build_context(
        &self,
        source: ContextSource,
        tracks: Vec<String>,
        cursor: usize,
        traversal: Traversal,
    ) -> PlaybackContext {
        let entries = self.materialize_entries(tracks, traversal);
        debug_assert!(
            cursor < entries.len(),
            "caller must pass an in-range cursor"
        );
        PlaybackContext {
            source,
            entries,
            cursor,
            traversal,
        }
    }

    /// Make a source's tracks the playing context: build their order under
    /// `start`, set the cursor, and make the cursor entry current. Clears the
    /// manual lane (a fresh playback session). Returns the track to play. The
    /// caller passes a non-empty `track_ids` (and an in-range `Index`); the
    /// context is therefore non-empty with a valid cursor.
    pub fn play_release(
        &mut self,
        source: ContextSource,
        track_ids: Vec<String>,
        start: ContextStart,
    ) -> String {
        self.manual.clear();
        let (cursor, traversal) = match start {
            ContextStart::Index(index) => (index, Traversal::Sequential),
            ContextStart::Shuffled { seed } => (0, Traversal::Shuffled { seed }),
        };
        let context = self.build_context(source, track_ids, cursor, traversal);
        let track = context.current().track_id.clone();
        self.current = Some(context.current().clone());
        self.context = Some(context);
        self.revision += 1;
        track
    }

    /// Play a single track with no surrounding context — nothing is queued after
    /// it. Used when a track's release can't be loaded; clears the manual lane.
    pub fn play_single(&mut self, track_id: String) {
        self.manual.clear();
        self.context = None;
        self.current = Some(self.mint(track_id));
        self.revision += 1;
    }

    /// Set the playing context to sequential or shuffled order while the playing
    /// track keeps playing. `on` materializes a shuffled order from `seed`; off
    /// restores `source_tracks` order. `source_tracks` is the context's own source
    /// release in source order (the service re-fetched it). The new order is stable
    /// until changed again — Previous walks exactly it, not a fresh shuffle per
    /// skip. `seed` is used only when `on`.
    ///
    /// The cursor lands on the context's own cursor track — the one the context is
    /// playing from — which came from `source_tracks` and so is present in the new
    /// order. (When a manual-lane track is the playing track, the context isn't the
    /// `current` entry; it resumes from where its cursor sat, not from the start.
    /// `self.manual` and `self.current` are untouched.)
    pub fn set_shuffle(&mut self, on: bool, source_tracks: Vec<String>, seed: u64) {
        let Some(ctx) = self.context.as_ref() else {
            warn!("set_shuffle: no playing context to shuffle; ignoring");
            return;
        };
        let anchor_track_id = ctx.current().track_id.clone();
        let traversal = if on {
            Traversal::Shuffled { seed }
        } else {
            Traversal::Sequential
        };
        let entries = self.materialize_entries(source_tracks, traversal);
        let cursor = entries
            .iter()
            .position(|e| e.track_id == anchor_track_id)
            .unwrap_or_else(|| {
                warn!(
                    "set_shuffle: context track {anchor_track_id} absent from \
                     re-fetched source; resuming the context at its start"
                );
                0
            });
        self.context = Some(PlaybackContext {
            source: ctx.source.clone(),
            entries,
            cursor,
            traversal,
        });
        self.revision += 1;
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
        // The playing track is a manual entry or the context's cursor entry. If it
        // is neither — the track survives but its context was dropped (the release
        // shrank past the cursor) — resume it as a standalone track.
        if let Some(tid) = snapshot.current_track_id {
            let from_manual = self.manual.iter().find(|e| e.track_id == tid).cloned();
            let from_context = self
                .context
                .as_ref()
                .filter(|c| c.current().track_id == tid)
                .map(|c| c.current().clone());
            self.current = Some(from_manual.or(from_context).unwrap_or_else(|| {
                warn!(
                    "restored current track {tid:?} is in neither the manual lane nor the \
                     context cursor; resuming it standalone"
                );
                self.mint(tid)
            }));
        } else {
            self.current = None;
        }
        self.revision += 1;
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
            self.revision += 1;
        }
        removed
    }

    /// Move the entry `id` to sit immediately before `before`; `before = None`
    /// moves it to the end of its lane. Reordering is within one lane — the
    /// manual lane or the context order — so `id` and `before` must be in the
    /// same lane; otherwise this logs and no-ops. (Reordering the context order
    /// mutates the order initially set by the release/shuffle; the cursor stays
    /// on the same playing track.)
    /// Returns whether the reorder happened: `false` when `id` or `before` names
    /// no entry in the target lane, so the caller (which holds the diagnostics
    /// sink) can count the unknown-entry anomaly. The specific mismatch is logged
    /// here at the pure layer.
    pub fn reorder(&mut self, id: &QueueEntryId, before: Option<&QueueEntryId>) -> bool {
        match self.locate(id) {
            Some(EntryLocation::Manual(from)) => {
                let before_pos = match before {
                    Some(before_id) => match self.manual.iter().position(|e| &e.id == before_id) {
                        Some(pos) => Some(pos),
                        None => {
                            warn!("reorder: before id not in the manual lane: {}", before_id.0);
                            return false;
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
                self.revision += 1;
                true
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
                            return false;
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
                self.revision += 1;
                true
            }
            None => {
                warn!("reorder: queue entry id not found: {}", id.0);
                false
            }
        }
    }

    /// Skip to the entry with the given id and make it current. A manual-lane
    /// target drains the entries ahead of it in the manual lane; a context target
    /// moves the cursor to it. Unknown id logs and no-ops; returns the entry now
    /// playing.
    pub fn skip_to(&mut self, id: &QueueEntryId) -> Option<QueueEntry> {
        match self.locate(id) {
            Some(EntryLocation::Manual(pos)) => {
                let entry = self
                    .manual
                    .drain(..=pos)
                    .next_back()
                    .expect("locate reported a manual entry");
                self.current = Some(entry.clone());
                self.revision += 1;
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
        self.revision += 1;
    }

    // -- Advance / retreat -----------------------------------------------------

    /// Move the context cursor to `cursor`, make that entry current, and return
    /// its track id. The context is present and `cursor` is in range. The single
    /// leaf mutator behind every cursor movement (advance, previous, skip-to-context,
    /// context-repeat loop), so it is the one place that bumps the revision for
    /// all of them.
    fn play_context_at(&mut self, cursor: usize) -> String {
        let ctx = self
            .context
            .as_mut()
            .expect("play_context_at: context present");
        ctx.cursor = cursor;
        let entry = ctx.current().clone();
        let track = entry.track_id.clone();
        self.current = Some(entry);
        self.revision += 1;
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
            self.revision += 1;
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

    /// The manual lane ("Up Next") in order — the explicitly enqueued entries,
    /// which drain before the context.
    pub fn manual_entries(&self) -> Vec<QueueEntry> {
        self.manual.iter().cloned().collect()
    }

    /// The context's projection — what it plays from, its not-yet-played tail,
    /// and whether it was ordered by shuffle — or `None` when nothing is playing.
    /// The two lanes are kept separate (not flattened) so each UI renders the
    /// manual lane and the context as distinct sections.
    pub fn context_projection(&self) -> Option<ContextProjection> {
        self.context.as_ref().map(|ctx| ContextProjection {
            source: ctx.source.clone(),
            shuffled: matches!(ctx.traversal, Traversal::Shuffled { .. }),
            upcoming: ctx.upcoming().to_vec(),
        })
    }

    /// The flat play order from the current track onward: the manual lane, then
    /// the not-yet-played tail of the context. Used where a single linear order
    /// is the right shape (the unit tests' play-order assertions, the platform
    /// media-session playlist) — not the two-section UI projection, which reads
    /// `manual_entries` and `context_projection` separately.
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

    /// The only edge that can produce a physical-side pause: the next track in
    /// the current release context while that context is sequential. Manual
    /// queue entries and shuffled traversals are not physical side playback.
    pub fn next_sequential_context_track(&self) -> Option<&str> {
        if !self.manual.is_empty() {
            return None;
        }
        let ctx = self.context.as_ref()?;
        if !matches!(ctx.traversal, Traversal::Sequential) {
            return None;
        }
        ctx.upcoming().first().map(|e| e.track_id.as_str())
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

    /// Monotonic counter bumped by every mutation that changes the projected
    /// queue, never by a read. See the field doc for what it's for.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// What the playing context plays from (a release or the library), or `None`
    /// when nothing is playing. The shuffle toggle and `Context` repeat read this
    /// to re-fetch the context's source tracks before re-materializing the order.
    pub fn context_source(&self) -> Option<&ContextSource> {
        self.context.as_ref().map(|ctx| &ctx.source)
    }

    /// The whole context order (including the tracks behind the cursor) as track
    /// ids. For tests that assert re-materialization keeps every track.
    #[cfg(test)]
    fn context_order(&self) -> Vec<String> {
        let ctx = self
            .context
            .as_ref()
            .expect("context_order requires a playing context");
        ctx.entries.iter().map(|e| e.track_id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coven::SequentialIdProvider;
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

    /// A release source for the given id, the common `play_release` source.
    fn rel_src(id: &str) -> ContextSource {
        ContextSource::Release(id.to_string())
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
            rel_src("r1"),
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
            rel_src("r1"),
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
            rel_src("r1"),
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
            rel_src("r1"),
            rel(&["t1", "t2", "t3", "t4"]),
            ContextStart::Shuffled { seed: 7 },
        );
        let mut all = full_order(&q);
        all.sort();
        assert_eq!(all, vec!["t1", "t2", "t3", "t4"]);
    }

    /// Both shuffle entry points — playing a release pre-shuffled, and toggling
    /// shuffle on later over an already-playing sequential context — derive the
    /// same order from the same seed each time, not a fresh random one per call.
    #[test]
    fn shuffle_entry_points_are_deterministic_for_a_seed() {
        let play_release_shuffled_order = |seed| {
            let mut q = queue();
            q.play_release(
                rel_src("r1"),
                rel(&["t1", "t2", "t3", "t4", "t5"]),
                ContextStart::Shuffled { seed },
            );
            full_order(&q)
        };
        assert_eq!(
            play_release_shuffled_order(42),
            play_release_shuffled_order(42),
            "play_release with a shuffled start yields the same order for the same seed"
        );

        let set_shuffle_on_order = |seed| {
            let mut q = queue();
            q.play_release(
                rel_src("r1"),
                rel(&["t1", "t2", "t3", "t4", "t5"]),
                ContextStart::Index(0),
            );
            q.set_shuffle(true, rel(&["t1", "t2", "t3", "t4", "t5"]), seed);
            q.context_order()
        };
        assert_eq!(
            set_shuffle_on_order(42),
            set_shuffle_on_order(42),
            "set_shuffle(true) yields the same order for the same seed"
        );
    }

    /// A repeating shuffled context loops a freshly re-derived order each pass,
    /// not the same order every time. Both passes' orders are read from the queue
    /// (no re-implementation of the shuffle) and are deterministic for a fixed
    /// seed.
    #[test]
    fn test_context_repeat_shuffled_loops_a_re_derived_order() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
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

    /// A `Library` source is the same construct as a release: its tracks
    /// materialize into the context under the seed, keeping every track, and the
    /// snapshot reports the `Library` source.
    #[test]
    fn test_library_source_context_materializes_all_tracks() {
        let mut q = queue();
        q.play_release(
            ContextSource::Library,
            rel(&["t1", "t2", "t3", "t4", "t5"]),
            ContextStart::Shuffled { seed: 3 },
        );
        let mut all = full_order(&q);
        all.sort();
        assert_eq!(all, vec!["t1", "t2", "t3", "t4", "t5"]);
        assert_eq!(q.snapshot().context.unwrap().source, ContextSource::Library);
    }

    // -- shuffle toggle --------------------------------------------------------

    #[test]
    fn test_set_shuffle_on_keeps_current_track_with_cursor_on_it() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3", "t4", "t5"]),
            ContextStart::Index(2),
        );
        assert_eq!(q.current_track_id(), Some("t3"));

        q.set_shuffle(true, rel(&["t1", "t2", "t3", "t4", "t5"]), 7);

        // The playing track keeps playing; the cursor sits on it.
        assert_eq!(q.current_track_id(), Some("t3"));
        let ctx = q.context_projection().expect("a release is playing");
        assert!(
            ctx.shuffled,
            "the context reports shuffled after turning shuffle on"
        );

        // Every source track is retained in the new order, just re-ordered.
        let mut all = q.context_order();
        all.sort();
        assert_eq!(all, vec!["t1", "t2", "t3", "t4", "t5"], "no track is lost");
    }

    #[test]
    fn test_set_shuffle_off_restores_source_order_from_current() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3", "t4", "t5"]),
            ContextStart::Index(2),
        );
        assert_eq!(q.current_track_id(), Some("t3"));

        // On then off; the playing track rides through to a restored source order.
        q.set_shuffle(true, rel(&["t1", "t2", "t3", "t4", "t5"]), 7);
        assert!(q.context_projection().unwrap().shuffled);
        assert_eq!(q.current_track_id(), Some("t3"));

        q.set_shuffle(false, rel(&["t1", "t2", "t3", "t4", "t5"]), 0);

        // Same playing track; the order is back to source order, cursor on it.
        assert_eq!(q.current_track_id(), Some("t3"));
        let ctx = q.context_projection().expect("a release is playing");
        assert!(
            !ctx.shuffled,
            "the context is sequential after turning shuffle off"
        );
        assert_eq!(
            full_order(&q),
            vec!["t3", "t4", "t5"],
            "source order resumes from the current track"
        );
    }

    // -- persistence round-trips -----------------------------------------------

    #[test]
    fn test_snapshot_restore_sequential_context() {
        let mut q = queue();
        q.play_release(
            rel_src("rel-A"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(1),
        );
        let snap = q.snapshot();
        assert_eq!(
            snap.context.as_ref().unwrap().source,
            ContextSource::Release("rel-A".into())
        );
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
            rel_src("rel-A"),
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
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(0));
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
            rel_src("r1"),
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
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(0));
        q.set_repeat_mode(RepeatMode::Context);
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t2"));
        // Exhausted under Context repeat → loop from the start of the same order.
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t1"));
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "t2"));
    }

    #[test]
    fn test_repeat_track_pins_current() {
        let mut q = queue();
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(0));
        q.set_repeat_mode(RepeatMode::Track);
        assert!(matches!(q.next_entry(), NextEntry::RepeatCurrent(t) if t == "t1"));
    }

    #[test]
    fn test_previous_steps_cursor_back_multiple() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
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
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(1));
        assert!(matches!(
            q.previous_action(5000),
            PreviousAction::RestartCurrent
        ));
    }

    #[test]
    fn test_skip_to_context_tail_moves_cursor() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
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
            rel_src("r1"),
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
            rel_src("r1"),
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
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(0));
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
            rel_src("r1"),
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
            rel_src("r1"),
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
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(0));
        let ids: HashSet<String> = ["t1"].iter().map(|s| s.to_string()).collect();
        q.remove_by_ids(&ids);
        assert_eq!(q.current_track_id(), None);
    }

    #[test]
    fn test_remove_by_ids_deleting_current_last_entry_keeps_cursor_valid() {
        let mut q = queue();
        // current = t2 at the last position (cursor == len-1).
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(1));
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

    /// The projection keeps the two lanes separate: the manual lane is its own
    /// list, the context is its not-yet-played tail, and no manual entry leaks
    /// into the context list (or vice versa). A sequential context is not
    /// shuffled.
    #[test]
    fn test_projection_keeps_manual_and_context_separate() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        q.add_to_queue(rel(&["m1", "m2"]));

        let manual: Vec<String> = q.manual_entries().into_iter().map(|e| e.track_id).collect();
        assert_eq!(manual, vec!["m1", "m2"], "the manual lane is its own list");

        let ctx = q.context_projection().expect("a release is playing");
        let context_tracks: Vec<String> = ctx.upcoming.into_iter().map(|e| e.track_id).collect();
        assert_eq!(
            context_tracks,
            vec!["t2", "t3"],
            "the context is only the not-yet-played tail"
        );
        assert!(!ctx.shuffled, "a sequential context is not shuffled");

        // The lanes don't bleed into each other.
        assert!(
            !context_tracks.iter().any(|t| t == "m1" || t == "m2"),
            "manual entries are not mixed into the context list"
        );
        assert!(
            !manual.iter().any(|t| t == "t2" || t == "t3"),
            "context entries are not mixed into the manual list"
        );
    }

    /// A shuffled context carries its `shuffled` flag through the projection.
    #[test]
    fn test_projection_context_carries_shuffled_flag() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3", "t4"]),
            ContextStart::Shuffled { seed: 7 },
        );
        let ctx = q.context_projection().expect("a release is playing");
        assert!(ctx.shuffled, "a shuffled context reports shuffled");
    }

    /// No context → no projection; the manual lane still projects on its own.
    #[test]
    fn test_projection_no_context_is_none() {
        let mut q = queue();
        q.add_to_queue(rel(&["m1", "m2"]));
        assert!(
            q.context_projection().is_none(),
            "nothing is playing from a release"
        );
        let manual: Vec<String> = q.manual_entries().into_iter().map(|e| e.track_id).collect();
        assert_eq!(manual, vec!["m1", "m2"]);
    }

    #[test]
    fn test_has_upcoming_and_has_previous() {
        let mut q = queue();
        assert!(!q.has_upcoming());
        assert!(!q.has_previous());
        q.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(0));
        assert!(q.has_upcoming());
        assert!(!q.has_previous());
        q.next_entry(); // → t2
        assert!(!q.has_upcoming());
        assert!(q.has_previous());
    }

    // -- next_sequential_context_track (physical-side pause edge) ---------------

    /// The only edge that yields a physical side pause: the next track of a
    /// sequential release context, with an empty manual lane. All other shapes —
    /// no context, a shuffled traversal, a pending manual entry, or the last
    /// context track — report `None`.
    #[test]
    fn test_next_sequential_context_track_all_branches() {
        // No context at all → None.
        let mut q = queue();
        assert_eq!(q.next_sequential_context_track(), None);

        // Sequential context with an upcoming track → that track.
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        assert_eq!(q.next_sequential_context_track(), Some("t2"));

        // A pending manual entry is not physical-side playback → None.
        q.add_to_queue(rel(&["m1"]));
        assert_eq!(q.next_sequential_context_track(), None);

        // A shuffled traversal is not a physical side → None.
        let mut shuffled = queue();
        shuffled.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Shuffled { seed: 5 },
        );
        assert_eq!(shuffled.next_sequential_context_track(), None);

        // The last track of a sequential context has no upcoming track → None.
        let mut last = queue();
        last.play_release(rel_src("r1"), rel(&["t1", "t2"]), ContextStart::Index(1));
        assert_eq!(last.next_sequential_context_track(), None);
    }

    // -- set_shuffle edge cases ------------------------------------------------

    /// With nothing playing there is no context to reorder, so `set_shuffle` is a
    /// no-op: no context appears and no track becomes current.
    #[test]
    fn test_set_shuffle_with_no_context_is_noop() {
        let mut q = queue();
        q.set_shuffle(true, rel(&["t1", "t2", "t3"]), 7);
        assert!(q.context_projection().is_none());
        assert_eq!(q.current_track_id(), None);
    }

    /// When the playing track is absent from the re-fetched source (the release
    /// shrank under it), the shuffle toggle resumes the new order from its start
    /// rather than dropping the context.
    #[test]
    fn test_set_shuffle_anchor_missing_resumes_at_start() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(1),
        );
        assert!(q.has_previous(), "cursor sits on t2, so t1 is behind it");

        // The re-fetched source no longer contains the playing track t2.
        q.set_shuffle(false, rel(&["x1", "x2", "x3"]), 0);

        assert_eq!(
            q.context_order(),
            vec!["x1", "x2", "x3"],
            "the new source order replaces the old one"
        );
        assert!(
            !q.has_previous(),
            "the cursor reset to the start of the new order"
        );
    }

    // -- restore fallback ------------------------------------------------------

    /// A restored current track that is neither a manual entry nor the context's
    /// cursor track (its context was dropped when the release shrank past the
    /// cursor) resumes standalone with a freshly minted entry.
    #[test]
    fn test_restore_current_in_neither_lane_resumes_standalone() {
        let snapshot = QueueSnapshot {
            context: Some(ContextSnapshot {
                source: rel_src("r1"),
                traversal: Traversal::Sequential,
                cursor: 0,
            }),
            manual: vec!["m1".into()],
            current_track_id: Some("ghost".into()),
            repeat: RepeatMode::Off,
        };
        let mut q = queue();
        // Context cursor lands on t1; the manual lane holds m1; "ghost" is neither.
        q.restore(snapshot, rel(&["t1", "t2"]));
        assert_eq!(q.current_track_id(), Some("ghost"));
        // The context and manual lane are still restored around the standalone
        // current track.
        assert_eq!(q.context_order(), vec!["t1", "t2"]);
    }

    // -- previous_action with a manual current ---------------------------------

    /// When the current track is a manual entry (not the context's cursor entry),
    /// stepping back within 3s lands on the context's cursor track — the release
    /// track that preceded the manual insertion.
    #[test]
    fn test_previous_action_from_manual_current_lands_on_cursor() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        q.add_to_queue(rel(&["m1"]));
        // Drain the manual lane: current becomes m1 while the cursor stays on t1.
        assert!(matches!(q.next_entry(), NextEntry::Play(t) if t == "m1"));
        assert_eq!(q.current_track_id(), Some("m1"));

        // Current is a manual item, so Previous lands on the cursor entry t1.
        assert!(matches!(
            q.previous_action(1000),
            PreviousAction::PlayPrevious(t) if t == "t1"
        ));
    }

    // -- remove of the currently-playing context entry -------------------------

    /// Removing the context entry that is currently playing clears `current`
    /// (nothing is playing until the service advances), and the cursor stays in
    /// bounds.
    #[test]
    fn test_remove_current_context_entry_clears_current() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        // Skip to t2 so it is both the cursor entry and current.
        let t2_id = q.upcoming()[0].id.clone();
        q.skip_to(&t2_id);
        assert_eq!(q.current_track_id(), Some("t2"));

        let removed = q.remove(&t2_id);
        assert_eq!(removed.map(|e| e.track_id), Some("t2".into()));
        assert_eq!(
            q.current_track_id(),
            None,
            "removing the playing context entry clears current"
        );
    }

    // -- revision ---------------------------------------------------------------

    /// Every mutating op bumps the revision; reads never do.
    #[test]
    fn test_revision_bumps_on_mutations_not_reads() {
        let mut q = queue();
        assert_eq!(q.revision(), 0);

        q.add_to_queue(rel(&["a", "b"]));
        assert_eq!(q.revision(), 1, "add_to_queue bumps");

        // Reads never bump.
        let _ = q.upcoming();
        let _ = q.front();
        let _ = q.has_upcoming();
        let _ = q.context_projection();
        let _ = q.manual_entries();
        assert_eq!(q.revision(), 1, "reads never bump");

        let id = manual_ids(&q)[0].clone();
        q.reorder(&id, None);
        assert_eq!(q.revision(), 2, "reorder bumps");

        q.remove(&id);
        assert_eq!(q.revision(), 3, "remove bumps");

        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3"]),
            ContextStart::Index(0),
        );
        assert_eq!(q.revision(), 4, "play_release bumps");

        q.next_entry();
        assert_eq!(q.revision(), 5, "advancing to the next track bumps");

        q.previous_action(1000);
        assert_eq!(q.revision(), 6, "stepping back bumps");

        let ctx_id = q.upcoming()[0].id.clone();
        q.skip_to(&ctx_id);
        assert_eq!(q.revision(), 7, "skip_to a context entry bumps");

        q.set_shuffle(true, rel(&["t1", "t2", "t3"]), 7);
        assert_eq!(q.revision(), 8, "set_shuffle bumps");

        q.clear();
        assert_eq!(q.revision(), 9, "clear bumps");
    }

    /// Unknown ids and other documented no-ops don't bump the revision.
    #[test]
    fn test_revision_unchanged_on_noops() {
        let mut q = queue();
        q.add_to_queue(rel(&["a"]));
        let after_add = q.revision();

        assert_eq!(q.remove(&QueueEntryId("nope".into())), None);
        assert_eq!(q.revision(), after_add, "unknown remove id doesn't bump");

        q.reorder(&QueueEntryId("nope".into()), None);
        assert_eq!(
            q.revision(),
            after_add,
            "unknown reorder source doesn't bump"
        );

        assert_eq!(q.skip_to(&QueueEntryId("nope".into())), None);
        assert_eq!(q.revision(), after_add, "unknown skip_to id doesn't bump");

        q.set_shuffle(true, rel(&["x"]), 1);
        assert_eq!(
            q.revision(),
            after_add,
            "set_shuffle with no playing context doesn't bump"
        );
    }

    // -- context reorder to the end --------------------------------------------

    /// Reordering a context entry with `before = None` moves it to the end of the
    /// context order while the cursor stays on the playing track.
    #[test]
    fn test_reorder_context_to_end() {
        let mut q = queue();
        q.play_release(
            rel_src("r1"),
            rel(&["t1", "t2", "t3", "t4"]),
            ContextStart::Index(0),
        );
        let t2_id = q.upcoming()[0].id.clone(); // upcoming = [t2, t3, t4]
        q.reorder(&t2_id, None);
        assert_eq!(upcoming_tracks(&q), vec!["t3", "t4", "t2"]);
        assert_eq!(
            q.current_track_id(),
            Some("t1"),
            "the cursor stays on the playing track"
        );
    }
}
