//! The context lane's whole-lane edits: filling it from what a play names,
//! shuffling it and putting it back in order, and dropping it.

use super::*;

impl PlaybackQueue {
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

    /// Drop the context lane ("Clear playing from"). The playing track keeps
    /// playing; when it ends Up Next drains and then playback stops.
    pub async fn clear_playing_from(&mut self) -> Result<(), DbError> {
        if !self.tables.clear_context().await? {
            warn!("clear_playing_from: nothing is playing from a source; ignoring");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
