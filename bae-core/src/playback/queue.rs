use std::collections::{HashMap, HashSet, VecDeque};

use tracing::warn;

use super::RepeatMode;
use crate::playback::context::{
    permute, ContextSource, ContextStart, PlaybackContext, ShuffleState,
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

/// The persistable view of a playing context: the recipe to refill the lane, not
/// the lane itself. Session edits (removals, reorders, the shuffled order) are
/// deliberately not persisted — storing the row list would rewrite a
/// library-sized blob on every edit. Restore fills the lane from `source` in
/// source order and shuffles it afresh when `shuffled`.
pub struct ContextSnapshot {
    pub source: ContextSource,
    pub shuffled: bool,
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

/// Pair a lane's rows with the cursor and shuffle state that complete a context.
/// The caller passes non-empty `entries` and an in-range `cursor` (`play_release`
/// validates its index; `restore` derives the cursor from the current track's
/// position), so a present context is always non-empty with a valid cursor.
fn build_context(
    source: ContextSource,
    entries: Vec<QueueEntry>,
    cursor: usize,
    shuffle: Option<ShuffleState>,
) -> PlaybackContext {
    debug_assert!(
        cursor < entries.len(),
        "caller must pass a non-empty lane and an in-range cursor"
    );
    PlaybackContext {
        source,
        entries,
        cursor,
        shuffle,
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

    /// Empty the manual lane ("Clear Up Next"). The context lane is untouched —
    /// each lane's Clear empties only its own section.
    pub fn clear_up_next(&mut self) {
        self.manual.clear();
        self.revision += 1;
    }

    // -- Context ---------------------------------------------------------------

    /// Mint one row per track, in the order given. Rows enter the context lane
    /// only here — at fill time — so mid-session the lane can shrink and
    /// rearrange but never grow.
    fn mint_entries(&self, tracks: Vec<String>) -> Vec<QueueEntry> {
        tracks.into_iter().map(|t| self.mint(t)).collect()
    }

    /// Make a source's tracks the playing context: clear Up Next, fill the lane,
    /// set the cursor, and make the cursor row current. Returns the track to play.
    /// The caller passes a non-empty `track_ids` (and an in-range `Index`); the
    /// lane is therefore non-empty with a valid cursor.
    pub fn play_release(
        &mut self,
        source: ContextSource,
        track_ids: Vec<String>,
        start: ContextStart,
    ) -> String {
        let mut entries = self.mint_entries(track_ids);
        let (cursor, shuffle) = match start {
            ContextStart::Index(index) => (index, None),
            ContextStart::Shuffled { seed } => {
                // Stamped before the permutation, so a later unshuffle lands the
                // lane back in source order.
                let restore_order = entries.iter().map(|e| e.id.clone()).collect();
                permute(&mut entries, seed);
                (
                    0,
                    Some(ShuffleState {
                        restore_order,
                        next_seed: seed.wrapping_add(1),
                    }),
                )
            }
        };
        let context = build_context(source, entries, cursor, shuffle);
        let track = context.current().track_id.clone();
        self.manual.clear();
        self.current = Some(context.current().clone());
        self.context = Some(context);
        self.revision += 1;
        track
    }

    /// Play a single track with no surrounding context. Used when a track's
    /// release can't be loaded. Up Next is untouched and drains after it.
    pub fn play_single(&mut self, track_id: String) {
        self.context = None;
        self.current = Some(self.mint(track_id));
        self.revision += 1;
    }

    /// Set the context lane to shuffled or sequential order in place. The rows are
    /// the authority — nothing is re-fetched, so removals and reorders survive the
    /// toggle and no track can enter the lane. The current row and the history
    /// before it never move; only the upcoming tail is rearranged. `seed` is used
    /// only when turning shuffle on.
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        let Some(ctx) = self.context.as_mut() else {
            warn!("set_shuffle: no playing context to shuffle; ignoring");
            return;
        };
        if ctx.shuffle.is_some() == on {
            return;
        }
        let upcoming = ctx.cursor + 1;
        match ctx.shuffle.take() {
            Some(state) => {
                // Every upcoming row's id is in `restore_order` — rows enter the
                // lane only at fill time, and the stamp covers the whole lane —
                // so sorting by position in it is total. Rows removed while
                // shuffled are simply absent from the slice being sorted, and
                // every survivor keeps its place in line.
                let position: HashMap<&QueueEntryId, usize> = state
                    .restore_order
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (id, i))
                    .collect();
                ctx.entries[upcoming..].sort_by_key(|e| {
                    *position
                        .get(&e.id)
                        .expect("every lane row was stamped when shuffle turned on")
                });
            }
            None => {
                let restore_order = ctx.entries.iter().map(|e| e.id.clone()).collect();
                permute(&mut ctx.entries[upcoming..], seed);
                ctx.shuffle = Some(ShuffleState {
                    restore_order,
                    next_seed: seed.wrapping_add(1),
                });
            }
        }
        self.revision += 1;
    }

    /// Drop the whole context lane ("Clear Playing From") — its rows, its
    /// history, its cursor, and the label the UI drew from its source. The
    /// playing track is deliberately left alone: it keeps playing, and when it
    /// ends Up Next drains and then playback stops. Nothing to clear without a
    /// context, so that logs and leaves the revision alone.
    pub fn clear_playing_from(&mut self) {
        if self.context.is_none() {
            warn!("clear_playing_from: nothing is playing from a source; ignoring");
            return;
        }
        self.context = None;
        self.revision += 1;
    }

    // -- Persistence -----------------------------------------------------------

    /// The persistable view of the queue (the refill recipe and track ids, no
    /// entry ids).
    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            context: self.context.as_ref().map(|ctx| ContextSnapshot {
                source: ctx.source.clone(),
                shuffled: ctx.shuffle.is_some(),
            }),
            manual: self.manual.iter().map(|e| e.track_id.clone()).collect(),
            current_track_id: self.current.as_ref().map(|e| e.track_id.clone()),
            repeat: self.repeat,
        }
    }

    /// Rebuild from a snapshot. `context_tracks` is the source's tracks in SOURCE
    /// order (the service re-fetched them); empty when there is no context.
    /// `shuffle_seed` permutes a shuffled restore and is unused otherwise.
    ///
    /// The lane comes back pristine — session edits were never persisted. A
    /// sequential restore is source order with the cursor on the current track,
    /// so history is the source prefix and Previous works; a shuffled restore is
    /// the current track first with the rest freshly permuted behind it.
    pub fn restore(
        &mut self,
        snapshot: QueueSnapshot,
        context_tracks: Vec<String>,
        shuffle_seed: u64,
    ) {
        self.repeat = snapshot.repeat;
        self.manual = snapshot.manual.into_iter().map(|t| self.mint(t)).collect();
        self.context = self.restore_context(
            snapshot.context,
            context_tracks,
            snapshot.current_track_id.as_deref(),
            shuffle_seed,
        );
        // The playing track is a manual entry or the context's cursor entry. If it
        // is neither — the track survives but its context was dropped (the source
        // no longer holds it) — resume it as a standalone track.
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

    /// Refill the context lane from a snapshot's recipe, or drop it. The recipe
    /// only resumes when it is whole: a source with tracks, and a current track
    /// the lane can put the cursor on. Any other shape logs what was missing and
    /// yields `None` — [`PlaybackQueue::restore`] then resumes the current track
    /// standalone rather than cueing a lane the user was never playing.
    fn restore_context(
        &self,
        snapshot: Option<ContextSnapshot>,
        tracks: Vec<String>,
        current_track_id: Option<&str>,
        shuffle_seed: u64,
    ) -> Option<PlaybackContext> {
        let cs = snapshot?;
        if tracks.is_empty() {
            warn!("resume context {:?} has no tracks; dropping it", cs.source);
            return None;
        }
        let Some(current) = current_track_id else {
            warn!(
                "resume context {:?} has no current track to resume at; dropping it",
                cs.source
            );
            return None;
        };
        let Some(index) = tracks.iter().position(|t| t == current) else {
            warn!(
                "resume current track {current:?} is absent from the {} tracks of {:?}; \
                 dropping the context",
                tracks.len(),
                cs.source
            );
            return None;
        };
        let mut entries = self.mint_entries(tracks);
        if !cs.shuffled {
            return Some(build_context(cs.source, entries, index, None));
        }
        // Stamped over source order before the permutation, so unshuffling after
        // a restart lands the lane in source order.
        let restore_order = entries.iter().map(|e| e.id.clone()).collect();
        let current = entries.remove(index);
        permute(&mut entries, shuffle_seed);
        entries.insert(0, current);
        Some(build_context(
            cs.source,
            entries,
            0,
            Some(ShuffleState {
                restore_order,
                next_seed: shuffle_seed.wrapping_add(1),
            }),
        ))
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
        // `Context` repeat, loop the (non-empty) lane from its start, permuting
        // it afresh when shuffled so each pass differs.
        if self.repeat == RepeatMode::Context && self.context.is_some() {
            self.repermute_for_repeat_pass();
            return NextEntry::Play(self.play_context_at(0));
        }

        NextEntry::Stop
    }

    /// Permute the whole lane for a fresh repeat pass when it is shuffled — a new
    /// pass over the same rows, so removals and reorders carry into it. Taking the
    /// seed advances it, so consecutive passes differ. `restore_order` is left
    /// alone: it stamps the lane, not this pass, so unshuffling after a wrap is
    /// still well-defined. A sequential lane keeps its order; the cursor is reset
    /// to the start by the `play_context_at(0)` that follows.
    fn repermute_for_repeat_pass(&mut self) {
        let ctx = self
            .context
            .as_mut()
            .expect("repermute_for_repeat_pass: context present");
        if let Some(shuffle) = ctx.shuffle.as_mut() {
            let seed = shuffle.next_seed;
            shuffle.next_seed = seed.wrapping_add(1);
            permute(&mut ctx.entries, seed);
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
            shuffled: ctx.shuffle.is_some(),
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
    /// the current release context while that lane is sequential. Manual queue
    /// entries and a shuffled lane are not physical side playback — shuffling
    /// closes this gate, unshuffling reopens it.
    pub fn next_sequential_context_track(&self) -> Option<&str> {
        if !self.manual.is_empty() {
            return None;
        }
        let ctx = self.context.as_ref()?;
        if ctx.shuffle.is_some() {
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

    /// The whole context lane (including the rows behind the cursor) as track
    /// ids. For tests that assert an operation kept every row.
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
#[path = "queue_tests.rs"]
mod tests;
