use super::*;

/// Playback-state persistence, deliberately **not** `cancellable`.
///
/// Both writes must run to completion: dropping the future partway leaves the
/// queue, current track, and position unwritten, so the next cold launch restores
/// stale state. `BaeApp`'s `ShutdownRace` lets the losing task keep running rather
/// than cancelling it.
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Graceful shutdown: saves playback state to disk, then stops playback.
    pub async fn shutdown(&self) {
        #[cfg(feature = "desktop")]
        {
            self.desktop.shutdown_mcp();
            self.desktop.shutdown_subsonic();
            self.services.playback_shutdown().await;
        }
        #[cfg(not(feature = "desktop"))]
        self.services.playback_shutdown().await;
    }

    /// Persist the current playback state without stopping playback. Mobile
    /// calls this when the app is backgrounded so the queue, current track, and
    /// position survive a later cold launch — it can't call `shutdown`, which
    /// would stop the background audio.
    pub async fn save_playback_state(&self) {
        self.services.playback_save_state().await;
    }
}
