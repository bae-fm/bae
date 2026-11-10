//! The playback values on [`AppServices`], with the playing track's display
//! read live from the library, the timeline, the playback positions beside
//! them, and the receivers the cast picker lists.

use super::*;

impl AppServices {
    /// What playback is doing as its screens show it, read from the
    /// device's `playback_now` row with the playing track's current display:
    /// the first value at once, then one after each write that changes it —
    /// playback's own, or the library's to what the playing track shows.
    /// `None` while the row names a track the library no longer holds, until
    /// playback moves off it. Where playback is in the track is
    /// [`subscribe_playback_positions`](Self::subscribe_playback_positions).
    pub fn subscribe_playback_values(
        &self,
    ) -> bae_store::live_query::LiveValues<Option<bae_playback::NowPlayingValues>> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_now_playing())
    }

    /// Where playback is in what it plays: the library track's position and
    /// the preview's, which change as audio plays. A position names its track
    /// or clip, so a surface shows it only against that one.
    pub fn subscribe_playback_positions(&self) -> bae_playback::PlaybackPositionsWatch {
        self.inner.playback.subscribe_positions()
    }

    /// The playing track while the person waits on its load and the load
    /// reads from storage, by id; `None` otherwise, and for a load this
    /// device serves from its own copy or the cache.
    pub fn subscribe_storage_waits(&self) -> bae_track_audio::StorageWaitWatch {
        self.inner.playback.subscribe_storage_waits()
    }

    /// The parts of the playing track whose bytes this device holds, as
    /// fractions of its length, which change as its bytes arrive and as its
    /// buffer drops what playback has passed.
    pub fn follow_track_buffered(&self) -> bae_playback::TrackBufferedFollow {
        self.inner.playback.follow_track_buffered()
    }

    /// Where playback is, for a surface that moves the position on itself,
    /// read from the device's `playback_timeline` row: the first value at
    /// once, then one each time playback leaves the line it holds.
    pub fn subscribe_playback_timeline(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_playback::PlaybackTimeline> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_playback_timeline())
    }

    /// The receivers the cast picker lists, the cast-to receiver first, then
    /// by name: the first list at once, then one after each write that
    /// changes it.
    pub fn subscribe_cast_receivers(
        &self,
    ) -> bae_store::live_query::LiveValues<Vec<bae_playback::renderer::CastReceiver>> {
        self.inner.manager.values_until_closed(self.inner.cast_picker.subscribe())
    }

    /// List `devices` as the cast picker's receivers in place of the last
    /// list, in one write.
    pub async fn show_cast_devices(
        &self,
        devices: Vec<bae_playback::renderer::CastDevice>,
    ) -> Result<(), bae_store::DbError> {
        self.inner.cast_picker.show(devices).await
    }
}
