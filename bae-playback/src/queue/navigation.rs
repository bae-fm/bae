//! Moving through the queue: what plays next under the repeat mode, what
//! Back does, and the reads that look ahead without moving.

use super::*;

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

impl PlaybackQueue {
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
        let current = self.current.as_ref().map(|current| current.id.clone());
        let Some(entry) = self.tables.step_back(current).await? else {
            return Ok(PreviousAction::RestartCurrent);
        };
        let track = entry.track_id.clone();
        self.current = Some(entry);
        Ok(PreviousAction::PlayPrevious(track))
    }

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
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
