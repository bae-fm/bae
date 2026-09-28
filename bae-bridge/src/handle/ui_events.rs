use super::*;

/// Convert events exposed by this bridge build. A host bridge without the
/// desktop feature omits the desktop import stream.
pub(super) fn convert_ui_event(
    event: bae_core::ui::UiEvent,
) -> Option<crate::types::BridgeUiEvent> {
    use crate::types::*;
    use bae_core::ui::UiEvent;

    match event {
        UiEvent::PlaybackError { reason } => Some(BridgeUiEvent::PlaybackError {
            reason: crate::types::BridgePlaybackErrorReason::from_core(reason),
        }),
        UiEvent::QueueItemsAdded { count } => Some(BridgeUiEvent::QueueItemsAdded { count }),
        #[cfg(feature = "desktop")]
        UiEvent::CandidateSignalsUpdated { key, signals } => {
            Some(BridgeUiEvent::CandidateSignalsUpdated {
                key,
                signals: crate::types::BridgeSignals::from_core(signals),
            })
        }
        #[cfg(feature = "desktop")]
        UiEvent::ImportIdentificationProgress { identified, total } => {
            Some(BridgeUiEvent::ImportIdentificationProgress { identified, total })
        }
        #[cfg(feature = "desktop")]
        UiEvent::ImportsInFlight { count } => Some(BridgeUiEvent::ImportsInFlight { count }),
        #[cfg(all(
            not(feature = "desktop"),
            not(any(target_os = "ios", target_os = "android"))
        ))]
        UiEvent::CandidateSignalsUpdated { .. }
        | UiEvent::ImportIdentificationProgress { .. }
        | UiEvent::ImportsInFlight { .. } => None,
    }
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_ui_events(&self, callback: Box<dyn crate::types::UiEventCallback>) {
        let mut events = self.services.subscribe_ui_events();
        let runtime = self.runtime.clone();
        crate::operation_runtime::spawn(runtime, move || async move {
            while let Some(heard) = events.next().await {
                for event in heard {
                    if let Some(bridge_event) = convert_ui_event(event) {
                        callback.on_event(bridge_event);
                    }
                }
            }
        });
    }
}
