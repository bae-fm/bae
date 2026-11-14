//! Track domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Resolve a mix of album and track IDs into track IDs. An album expands to the
    /// tracks of its primary release, its lowest-ordered one.
    pub async fn resolve_to_track_ids(&self, ids: &[String]) -> Result<Vec<String>, LibraryError> {
        let mut track_ids = Vec::new();
        for id in ids {
            if let Some(album_track_ids) =
                self.records.get_primary_release_track_ids_for_album(id).await?
            {
                track_ids.extend(album_track_ids);
            } else if self.records.find_track_by_id(id).await?.is_some() {
                track_ids.push(id.clone());
            } else {
                return Err(LibraryError::TrackMapping(format!(
                    "ID not found as album or track: {id}"
                )));
            }
        }
        Ok(track_ids)
    }

    /// Follow the queue as its screens show it.
    pub fn subscribe_queue_value(
        &self,
    ) -> coven::LiveQuery<bae_playback::ResolvedQueueSnapshot, bae_store::DbError> {
        bae_playback::tables::subscribe_queue_value(&self.store)
    }

    /// Follow the queue's last addition to Up Next.
    pub fn subscribe_queue_addition(
        &self,
    ) -> coven::LiveQuery<Option<bae_playback::QueueAddition>, bae_store::DbError> {
        bae_playback::tables::subscribe_queue_addition(&self.store)
    }

    /// Follow the windows `initial` names of the context lane's upcoming tracks.
    pub fn subscribe_queue_upcoming(
        &self,
        initial: bae_playback::QueueWindows,
    ) -> coven::ReconfigurableLiveQuery<
        bae_playback::QueueWindows,
        bae_playback::tables::QueueUpcomingRead,
        bae_store::DbError,
    > {
        bae_playback::tables::subscribe_queue_upcoming(&self.store, initial)
    }

    /// Test-only. Production reads audio format as part of the resolved track-audio
    /// / playback-info aggregates below, never standalone.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<bae_records::DbAudioFormat>, LibraryError> {
        Ok(self.records.find_audio_format_by_track_id(track_id).await?)
    }

    /// What playback decides a side or disc pause with about a track: its
    /// release and side as the library holds them now. Resolved here so
    /// `PlaybackService` never sees a `DbTrack`.
    pub async fn get_playback_track_info(
        &self,
        track_id: &str,
    ) -> Result<bae_playback::PlaybackTrackInfo, LibraryError> {
        bae_playback::PlaybackTrackInfo::read(&self.records, track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {track_id}")))
    }

    /// What surfaces show of a track, as the library holds it now.
    pub async fn get_track_display(
        &self,
        track_id: &str,
    ) -> Result<bae_playback::TrackDisplay, LibraryError> {
        bae_playback::tables::track_display(&self.store, track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {track_id}")))
    }

    /// Follow where playback is, as the surfaces that move the position on
    /// themselves read it.
    pub fn subscribe_playback_timeline(
        &self,
    ) -> coven::LiveQuery<bae_playback::PlaybackTimeline, bae_store::DbError> {
        bae_playback::tables::subscribe_playback_timeline(&self.store)
    }

    /// Follow what playback is doing as its screens show it; `None` while it
    /// names a track the library no longer holds.
    pub fn subscribe_now_playing(
        &self,
    ) -> coven::LiveQuery<Option<bae_playback::NowPlayingValues>, bae_store::DbError> {
        bae_playback::tables::subscribe_now_playing(&self.store)
    }
}

#[cfg(test)]
#[path = "track_tests.rs"]
mod tests;
