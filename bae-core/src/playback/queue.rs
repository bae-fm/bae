use std::collections::HashSet;

use tracing::warn;

use super::RepeatMode;
use crate::db::{
    ContextFill, ContextFilled, DbError, ManualPlace, PlaybackQueueTables, ShuffleChange,
};
use crate::playback::timeline::TrackTime;

/// Per-instance identity for a queued track: the same track queued twice is
/// two entries, each removable, reorderable and skippable on its own. The
/// queue tables number them, and no number is used twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueueEntryId(pub i64);

impl std::str::FromStr for QueueEntryId {
    type Err = std::num::ParseIntError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(QueueEntryId)
    }
}

impl std::fmt::Display for QueueEntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// One queued instance: its entry id and the track it plays.
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
    /// Play this track next (from Up Next or the context lane).
    Play(String),
    /// Nothing left to play.
    Stop,
}

/// How far into the current track, from its start (INDEX 01), Back restarts it
/// instead of going to the previous track.
const RESTART_THRESHOLD: TrackTime = TrackTime::from_millis(3_000);

/// What to do when going to the previous track.
pub enum PreviousAction {
    /// Go back to this track.
    PlayPrevious(String),
    /// Restart the current track (past the 3 s threshold, or nothing before it).
    RestartCurrent,
}

/// The playback service's queue: the queue tables, which hold both lanes and
/// where the context lane stands, and the entry playing now, which may have
/// left the tables (a drained Up Next entry, or a track played on its own).
/// The service is the tables' only writer, so the repeat mode it keeps here is
/// the one they hold.
pub struct PlaybackQueue {
    tables: PlaybackQueueTables,
    current: Option<QueueEntry>,
    repeat: RepeatMode,
}

impl PlaybackQueue {
    /// The queue over `tables`, with nothing playing and repeat off: what an
    /// opened library's tables hold until [`Self::restore`] reads them.
    pub fn new(tables: PlaybackQueueTables) -> Self {
        Self {
            tables,
            current: None,
            repeat: RepeatMode::Off,
        }
    }

    /// Take up the queue the tables hold, playing `current`.
    pub async fn restore(&mut self, current: Option<QueueEntry>) -> Result<(), DbError> {
        self.repeat = self.tables.repeat().await?;
        self.current = current;
        Ok(())
    }

    /// How many tracks the context lane holds, played or not.
    pub async fn context_track_count(&self) -> Result<usize, DbError> {
        self.tables.context_track_count().await
    }

    // -- Up Next ---------------------------------------------------------------

    /// Add tracks to the end of Up Next; how many the library held.
    pub async fn add_to_queue(&mut self, track_ids: Vec<String>) -> Result<usize, DbError> {
        self.tables.add_manual(track_ids, ManualPlace::End).await
    }

    /// Add tracks to the front of Up Next (play next), in their order.
    pub async fn add_next(&mut self, track_ids: Vec<String>) -> Result<usize, DbError> {
        self.tables.add_manual(track_ids, ManualPlace::Front).await
    }

    /// Insert tracks before the Up Next entry at `index`.
    pub async fn insert_at(
        &mut self,
        index: usize,
        track_ids: Vec<String>,
    ) -> Result<usize, DbError> {
        self.tables
            .add_manual(track_ids, ManualPlace::At(index))
            .await
    }

    /// Empty Up Next ("Clear Up Next"). The context lane is untouched.
    pub async fn clear_up_next(&mut self) -> Result<(), DbError> {
        self.tables.clear_manual().await
    }

    // -- Context ---------------------------------------------------------------

    /// Replace the queue with what `fill` names and make its first track
    /// current. `NoTracks` and `StartMissing` leave the queue as it was.
    pub async fn play(&mut self, fill: ContextFill) -> Result<ContextFilled, DbError> {
        let filled = self.tables.fill(fill).await?;
        if let ContextFilled::Playing { current, .. } = &filled {
            self.current = Some(current.clone());
        }
        Ok(filled)
    }

    /// Shuffle the context lane's upcoming tracks, or put them back in the
    /// order they had before. The playing track and the ones before it stay.
    pub async fn set_shuffle(&mut self, on: bool) -> Result<(), DbError> {
        if self.tables.set_shuffle(on).await? == ShuffleChange::NoContext {
            warn!("set_shuffle: no playing context to shuffle; ignoring");
        }
        Ok(())
    }

    /// Drop the context lane ("Clear Playing From"). The playing track keeps
    /// playing; when it ends Up Next drains and then playback stops.
    pub async fn clear_playing_from(&mut self) -> Result<(), DbError> {
        if !self.tables.clear_context().await? {
            warn!("clear_playing_from: nothing is playing from a source; ignoring");
        }
        Ok(())
    }

    // -- Entries in either lane ------------------------------------------------

    /// Take the entry out of whichever lane holds it. Removing the entry
    /// playing clears `current`. An unknown id logs and returns `None`.
    pub async fn remove(&mut self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        let removed = self.tables.remove(id).await?;
        match &removed {
            Some(removed) => {
                if self.current.as_ref().is_some_and(|c| c.id == removed.id) {
                    self.current = None;
                }
            }
            None => warn!("remove: queue entry id not found: {id}"),
        }
        Ok(removed)
    }

    /// Move the entry to sit just before `before`, or to the end of its lane;
    /// both in one lane. `false` when either names no entry of that lane, so
    /// the caller can count the anomaly; the mismatch is logged here.
    pub async fn reorder(
        &mut self,
        id: QueueEntryId,
        before: Option<QueueEntryId>,
    ) -> Result<bool, DbError> {
        self.tables.reorder(id, before).await
    }

    /// Make the entry current: an Up Next entry drains the ones ahead of it, a
    /// context entry becomes the lane's playing track. An unknown id logs and
    /// returns `None`.
    pub async fn skip_to(&mut self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        let entry = self.tables.skip_to(id).await?;
        match &entry {
            Some(entry) => self.current = Some(entry.clone()),
            None => warn!("skip_to: queue entry id not found: {id}"),
        }
        Ok(entry)
    }

    /// The library deleted `track_ids`, and with them their rows of the
    /// tables. Forget the current entry when its track is one; whether it was.
    pub fn forget_deleted_current(&mut self, track_ids: &HashSet<String>) -> bool {
        let deleted = self
            .current
            .as_ref()
            .is_some_and(|current| track_ids.contains(&current.track_id));
        if deleted {
            self.current = None;
        }
        deleted
    }

    // -- Advance / retreat -----------------------------------------------------

    /// Decide the next track: repeat-track pins the current; otherwise drain
    /// Up Next, then the context lane, then (under `Context` repeat) play the
    /// lane again from its start, reshuffled when shuffled, else stop.
    pub async fn next_entry(&mut self) -> Result<NextEntry, DbError> {
        if self.repeat == RepeatMode::Track {
            if let Some(current) = &self.current {
                return Ok(NextEntry::RepeatCurrent(current.track_id.clone()));
            }
        }
        if let Some(track) = self.advance_to_front().await? {
            return Ok(NextEntry::Play(track));
        }
        if self.repeat == RepeatMode::Context {
            if let Some(first) = self.tables.wrap().await? {
                let track = first.track_id.clone();
                self.current = Some(first);
                return Ok(NextEntry::Play(track));
            }
        }
        Ok(NextEntry::Stop)
    }

    /// Advance to the upcoming front and make it current, bypassing
    /// repeat-track and the context-repeat loop — the preload path, where the
    /// front track is already buffered. `None` when nothing is queued ahead.
    pub async fn advance_to_front(&mut self) -> Result<Option<String>, DbError> {
        let Some(entry) = self.tables.advance().await? else {
            return Ok(None);
        };
        let track = entry.track_id.clone();
        self.current = Some(entry);
        Ok(Some(track))
    }

    /// Decide the previous action from `track_time`, how far the current track
    /// has played from its start (INDEX 01) — negative while its pregap plays.
    /// Under the 3 s threshold, step the context lane back: from its playing
    /// track to the one before, or, when Up Next's track is playing, to the
    /// context track it followed. Otherwise restart the current track.
    pub async fn previous_action(
        &mut self,
        track_time: TrackTime,
    ) -> Result<PreviousAction, DbError> {
        if track_time >= RESTART_THRESHOLD {
            return Ok(PreviousAction::RestartCurrent);
        }
        let current = self.current.as_ref().map(|current| current.id);
        let Some(entry) = self.tables.step_back(current).await? else {
            return Ok(PreviousAction::RestartCurrent);
        };
        let track = entry.track_id.clone();
        self.current = Some(entry);
        Ok(PreviousAction::PlayPrevious(track))
    }

    // -- Reads -----------------------------------------------------------------

    /// The track that plays next, for preload, without moving to it.
    pub async fn front(&self) -> Result<Option<String>, DbError> {
        self.tables.front().await
    }

    /// The only edge that can produce a physical-side pause: the next track of
    /// the context lane while Up Next is empty and the lane is in its own
    /// order. Shuffling closes this gate, unshuffling reopens it.
    pub async fn next_sequential_context_track(&self) -> Result<Option<String>, DbError> {
        self.tables.next_in_order().await
    }

    pub async fn set_repeat_mode(&mut self, mode: RepeatMode) -> Result<(), DbError> {
        if self.repeat == mode {
            return Ok(());
        }
        self.tables.set_repeat(mode).await?;
        self.repeat = mode;
        Ok(())
    }

    pub fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    pub fn current(&self) -> Option<&QueueEntry> {
        self.current.as_ref()
    }

    pub fn current_track_id(&self) -> Option<&str> {
        self.current.as_ref().map(|e| e.track_id.as_str())
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
