use super::*;

/// What one UI subscriber hears: each playback notice sent after it
/// subscribed. Nothing is dropped however far behind the subscriber falls:
/// notices wait in order.
pub struct UiEvents {
    playback: tokio::sync::mpsc::UnboundedReceiver<crate::playback::PlaybackProgress>,
}

impl UiEvents {
    pub(crate) fn of(app_services: &crate::library::AppServices) -> Self {
        Self {
            playback: app_services.subscribe_playback_progress(),
        }
    }

    /// The next events, in order. `None` once playback is gone.
    pub async fn next(&mut self) -> Option<Vec<UiEvent>> {
        loop {
            if let Some(event) = playback_notice(self.playback.recv().await?) {
                return Some(vec![event]);
            }
        }
    }
}

/// The notice a playback update gives the UI, if any: most updates reach it
/// through the playback values instead.
fn playback_notice(progress: crate::playback::PlaybackProgress) -> Option<UiEvent> {
    use crate::playback::PlaybackProgress;
    match progress {
        PlaybackProgress::QueueItemsAdded { count } => Some(UiEvent::QueueItemsAdded { count }),
        PlaybackProgress::PlaybackError { reason } => Some(UiEvent::PlaybackError { reason }),
        _ => None,
    }
}
