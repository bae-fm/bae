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
    ) -> crate::library::LiveValues<Option<crate::playback::NowPlayingValues>> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_now_playing())
    }

    /// Where playback is in what it plays: the library track's position and
    /// the preview's, which change as audio plays. A position names its track
    /// or clip, so a surface shows it only against that one.
    pub fn subscribe_playback_positions(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::playback::PlaybackPositions> {
        self.inner.playback.subscribe_positions()
    }

    /// The download a load of the playing track waits on, which changes as
    /// its bytes arrive: how much of what the load reads from storage is
    /// here, while the person waits on the load; `None` otherwise, and for a
    /// load this device serves from its own copy or the cache.
    pub fn subscribe_track_downloads(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<crate::playback::TrackDownload>> {
        self.inner.playback.subscribe_track_downloads()
    }

    /// Where playback is, for a surface that moves the position on itself,
    /// read from the device's `playback_timeline` row: the first value at
    /// once, then one each time playback leaves the line it holds.
    pub fn subscribe_playback_timeline(
        &self,
    ) -> crate::library::LiveValues<crate::playback::PlaybackTimeline> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_playback_timeline())
    }

    /// The receivers the cast picker lists, by name: the first list at
    /// once, then one after each write that changes it.
    pub fn subscribe_cast_devices(
        &self,
    ) -> crate::library::LiveValues<Vec<crate::renderer::CastDevice>> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_cast_devices())
    }

    /// List `devices` as the cast picker's receivers in place of the last
    /// list, in one write.
    pub async fn show_cast_devices(
        &self,
        devices: Vec<crate::renderer::CastDevice>,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.manager.show_cast_devices(devices).await
    }
}
