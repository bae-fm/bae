use std::collections::HashSet;

use tracing::warn;

use super::RepeatMode;
use crate::tables::{
    ContextFill, ContextFilled, ManualPlace, PlaybackQueueTables, PlaybackResumeRow, ShuffleChange,
};
use bae_store::DbError;
use bae_types::track_time::TrackTime;

mod context;
mod navigation;
pub use navigation::{NextEntry, PreviousAction};

/// What names a queued row to the screens and back. Every row of either lane
/// is an entry of its own — the same track queued twice is two entries, each
/// removable, reorderable and skippable on its own — numbered by the queue
/// tables, which never use a number twice.
///
/// Its text, the id the screens carry, is `m` for Up Next or `c` for the
/// context lane, then the entry number.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueueEntryId {
    Manual(i64),
    Context(i64),
}

impl std::str::FromStr for QueueEntryId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (lane, entry): (fn(i64) -> QueueEntryId, &str) =
            if let Some(entry) = value.strip_prefix('m') {
                (QueueEntryId::Manual, entry)
            } else if let Some(entry) = value.strip_prefix('c') {
                (QueueEntryId::Context, entry)
            } else {
                return Err(format!("{value:?} is not a queue entry id"));
            };
        entry.parse().map(lane).map_err(|e| format!("{value:?} names no queue entry: {e}"))
    }
}

impl std::fmt::Display for QueueEntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueEntryId::Manual(entry) => write!(f, "m{entry}"),
            QueueEntryId::Context(entry) => write!(f, "c{entry}"),
        }
    }
}

/// One queued instance: its entry id and the track it plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueEntry {
    pub id: QueueEntryId,
    pub track_id: String,
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
        Self { tables, current: None, repeat: RepeatMode::Off }
    }

    /// Take up the queue the tables hold, playing `current`.
    pub async fn restore(&mut self, current: Option<QueueEntry>) -> Result<(), DbError> {
        self.repeat = self.tables.repeat().await?;
        self.current = current;
        Ok(())
    }

    /// Record that this queue is playing its current entry, `position_ms`
    /// into it, in the device-local resume row.
    pub async fn save_playback_state(&self, position_ms: Option<u64>) -> Result<(), DbError> {
        self.tables
            .save_playback_state(&PlaybackResumeRow { current: self.current.clone(), position_ms })
            .await
    }

    /// Forget what the resume row recorded: nothing is playing.
    pub async fn clear_playback_state(&self) -> Result<(), DbError> {
        self.tables.clear_playback_state().await
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
        self.tables.add_manual(track_ids, ManualPlace::At(index)).await
    }

    /// Empty Up next ("Clear up next"). The context lane is untouched.
    pub async fn clear_up_next(&mut self) -> Result<(), DbError> {
        self.tables.clear_manual().await
    }

    // -- Entries in either lane ------------------------------------------------

    /// Take the entry out of whichever lane holds it. Removing the entry
    /// playing clears `current`. An unknown id logs and returns `None`.
    pub async fn remove(&mut self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        let removed = self.tables.remove(id.clone()).await?;
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
    /// both in one lane. Answers the lane's version the move made, or `None`
    /// when either names no entry of that lane, so the caller can count the
    /// anomaly; the mismatch is logged here.
    pub async fn reorder(
        &mut self,
        id: QueueEntryId,
        before: Option<QueueEntryId>,
    ) -> Result<Option<u64>, DbError> {
        self.tables.reorder(id, before).await
    }

    /// Make the entry current: an Up Next entry drains the ones ahead of it, a
    /// context entry becomes the lane's playing track. An unknown id logs and
    /// returns `None`.
    pub async fn skip_to(&mut self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        let entry = self.tables.skip_to(id.clone()).await?;
        match &entry {
            Some(entry) => self.current = Some(entry.clone()),
            None => warn!("skip_to: queue entry id not found: {id}"),
        }
        Ok(entry)
    }

    /// The library deleted `track_ids`, and with them their rows of the
    /// tables. Forget the current entry when its track is one; whether it was.
    pub fn forget_deleted_current(&mut self, track_ids: &HashSet<String>) -> bool {
        let deleted =
            self.current.as_ref().is_some_and(|current| track_ids.contains(&current.track_id));
        if deleted {
            self.current = None;
        }
        deleted
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
