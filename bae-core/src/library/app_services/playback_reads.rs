//! The playback values on [`AppServices`], with the playing track's display
//! read live from the library, the timeline, and the playback positions
//! beside them.

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

    /// Where playback is, for a surface that moves the position on itself,
    /// read from the device's `playback_timeline` row: the first value at
    /// once, then one each time playback leaves the line it holds.
    pub fn subscribe_playback_timeline(
        &self,
    ) -> crate::library::LiveValues<crate::playback::PlaybackTimeline> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_playback_timeline())
    }
}
