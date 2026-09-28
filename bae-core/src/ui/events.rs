use super::*;

/// What one UI subscriber hears: each playback notice sent after it
/// subscribed, and on desktop the import runtime's values, first as they
/// stand and then as they change. Nothing is dropped however far behind the
/// subscriber falls: notices wait in order, and values are read where the
/// runtime is when the subscriber looks.
pub struct UiEvents {
    playback: tokio::sync::mpsc::UnboundedReceiver<crate::playback::PlaybackProgress>,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    import_values: crate::import::candidate_runtime::RuntimeValuesWatch,
}

impl UiEvents {
    pub(crate) fn of(app_services: &crate::library::AppServices) -> Self {
        Self {
            playback: app_services.subscribe_playback_progress(),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            import_values: app_services.watch_import_runtime_values(),
        }
    }

    /// The next events, in order. `None` once playback or the import runtime
    /// is gone.
    pub async fn next(&mut self) -> Option<Vec<UiEvent>> {
        loop {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            let events: Vec<UiEvent> = tokio::select! {
                progress = self.playback.recv() => playback_notice(progress?).into_iter().collect(),
                values = self.import_values.next() => values?.into_iter().map(import_value).collect(),
            };
            #[cfg(any(target_os = "ios", target_os = "android"))]
            let events: Vec<UiEvent> = playback_notice(self.playback.recv().await?)
                .into_iter()
                .collect();
            if !events.is_empty() {
                return Some(events);
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn import_value(value: crate::import::candidate_runtime::RuntimeValue) -> UiEvent {
    use crate::import::candidate_runtime::RuntimeValue;
    match value {
        RuntimeValue::Signals { key, signals } => UiEvent::CandidateSignalsUpdated { key, signals },
        RuntimeValue::IdentificationProgress { identified, total } => {
            UiEvent::ImportIdentificationProgress { identified, total }
        }
        RuntimeValue::ImportsInFlight { count } => UiEvent::ImportsInFlight { count },
    }
}
