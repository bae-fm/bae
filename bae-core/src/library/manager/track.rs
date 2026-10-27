//! Track domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Ordered track IDs for a release, without pulling full `DbTrack` rows. For
    /// callers that only need IDs (queue building, repeat-album rebuild).
    pub async fn get_track_ids(&self, release_id: &str) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_track_ids_for_release(release_id).await?)
    }

    /// Every track id in the library, in a deterministic base order, for a
    /// listing that pages through every song.
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_all_track_ids().await?)
    }

    /// The subset of `ids` that still exist in the tracks table, in one query
    /// rather than one per track.
    pub async fn filter_existing_track_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.filter_existing_track_ids(ids).await?)
    }

    /// Resolve a mix of album and track IDs into track IDs. An album expands to the
    /// tracks of its primary release, its lowest-ordered one.
    pub async fn resolve_to_track_ids(&self, ids: &[String]) -> Result<Vec<String>, LibraryError> {
        let mut track_ids = Vec::new();
        for id in ids {
            if let Some(album_track_ids) = self
                .database
                .get_primary_release_track_ids_for_album(id)
                .await?
            {
                track_ids.extend(album_track_ids);
            } else if self.database.find_track_by_id(id).await?.is_some() {
                track_ids.push(id.clone());
            } else {
                return Err(LibraryError::TrackMapping(format!(
                    "ID not found as album or track: {id}"
                )));
            }
        }
        Ok(track_ids)
    }

    /// The playback queue's tables, which the playback service changes.
    pub(crate) fn playback_queue_tables(&self) -> crate::db::PlaybackQueueTables {
        self.database.playback_queue_tables()
    }

    /// Follow the queue as its screens show it.
    pub(crate) fn subscribe_queue_value(
        &self,
    ) -> coven::LiveQuery<crate::queue::ResolvedQueueSnapshot> {
        self.database.subscribe_queue_value()
    }

    /// The queue as its screens show it now.
    pub async fn queue_value(&self) -> Result<crate::queue::ResolvedQueueSnapshot, LibraryError> {
        Ok(self.database.queue_value().await?)
    }

    /// Every one of the context lane's tracks after the playing one.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn queue_upcoming(&self) -> Result<Vec<crate::queue::QueueItem>, LibraryError> {
        Ok(self.database.queue_upcoming().await?)
    }

    /// Follow the windows `initial` names of the context lane's upcoming tracks.
    pub(crate) fn subscribe_queue_upcoming(
        &self,
        initial: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        crate::library::LibraryPageWindows,
        crate::db::QueueUpcomingRead,
    > {
        self.database.subscribe_queue_upcoming(initial)
    }

    /// The file record for a file id — streaming looks the id up on the track's
    /// audio segments, then fetches the row here.
    pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, LibraryError> {
        Ok(self.database.find_file_by_id(file_id).await?)
    }

    /// Test-only. Production reads audio format as part of the resolved track-audio
    /// / playback-info aggregates below, never standalone.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, LibraryError> {
        Ok(self
            .database
            .find_audio_format_by_track_id(track_id)
            .await?)
    }

    /// Resolve a track's audio into a `ResolvedTrackAudio` with its sample window
    /// resolved and all raw `Db*` fields hidden.
    pub async fn resolve_track_audio(
        &self,
        track_id: &str,
    ) -> Result<ResolvedTrackAudio, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        Ok(ResolvedTrackAudio::from_meta(&meta))
    }

    /// What playback decides a side or disc pause with about a track: its
    /// release and side as the library holds them now. Resolved here so
    /// `PlaybackService` never sees a `DbTrack`.
    pub async fn get_playback_track_info(
        &self,
        track_id: &str,
    ) -> Result<crate::playback::PlaybackTrackInfo, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let mediums = self.database.get_release_mediums(&track.release_id).await?;
        Ok(crate::playback::PlaybackTrackInfo::of(&track, &mediums))
    }

    /// What surfaces show of a track, as the library holds it now.
    pub async fn get_track_display(
        &self,
        track_id: &str,
    ) -> Result<crate::playback::TrackDisplay, LibraryError> {
        self.database
            .track_display(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {track_id}")))
    }

    pub(crate) fn subscribe_track_display(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<crate::playback::TrackDisplay>> {
        self.database.subscribe_track_display(initial)
    }

    pub(crate) fn subscribe_playback_track_infos(
        &self,
        initial: Vec<String>,
    ) -> coven::ReconfigurableLiveQuery<Vec<String>, Vec<crate::playback::PlaybackTrackInfo>> {
        self.database.subscribe_playback_track_infos(initial)
    }
}
