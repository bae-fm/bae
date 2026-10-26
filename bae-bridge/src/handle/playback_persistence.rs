use super::*;

/// Playback-state persistence, deliberately **not** `cancellable`: dropping
/// the future partway leaves the queue, current track, and position unwritten,
/// so the next cold launch restores stale state. Closing the library saves it
/// and stops playback.
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Persist the current playback state without stopping playback. Mobile
    /// calls this when the app is backgrounded so the queue, current track, and
    /// position survive a later cold launch — closing the library would stop
    /// the background audio.
    pub async fn save_playback_state(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services.playback_save_state().await;
            Ok(())
        })
        .await
    }
}
