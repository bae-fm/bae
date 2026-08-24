use super::*;

/// Convert events exposed by this bridge build. A host bridge without the
/// desktop feature omits the desktop import stream.
pub(super) fn convert_ui_event(
    event: bae_core::ui::UiBusEvent,
) -> Option<crate::types::BridgeUiEvent> {
    use crate::types::*;
    use bae_core::ui::UiBusEvent;

    match event {
        UiBusEvent::PlaybackError { reason } => Some(BridgeUiEvent::PlaybackError {
            reason: crate::types::BridgePlaybackErrorReason::from_core(reason),
        }),
        UiBusEvent::QueueItemsAdded { count } => Some(BridgeUiEvent::QueueItemsAdded { count }),
        #[cfg(feature = "desktop")]
        UiBusEvent::CandidateSignalsUpdated { key, signals } => {
            Some(BridgeUiEvent::CandidateSignalsUpdated {
                key,
                signals: crate::types::BridgeSignals::from_core(signals),
            })
        }
        #[cfg(feature = "desktop")]
        UiBusEvent::ImportQueueIdentifyProgress { identified, total } => {
            Some(BridgeUiEvent::ImportQueueIdentifyProgress { identified, total })
        }
        #[cfg(all(
            not(feature = "desktop"),
            not(any(target_os = "ios", target_os = "android"))
        ))]
        UiBusEvent::CandidateSignalsUpdated { .. }
        | UiBusEvent::ImportQueueIdentifyProgress { .. } => None,
        UiBusEvent::Error { error } => Some(BridgeUiEvent::Error {
            error: crate::types::BridgeError::from_core(error),
        }),
    }
}

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_ui_events(&self, callback: Box<dyn crate::types::UiEventCallback>) {
        let bus = self.ui_event_bus.clone();
        let runtime = self.runtime.handle().clone();
        crate::operation_runtime::spawn(runtime, move || async move {
            let rx = bus.subscribe();
            pump_ui_events(rx, callback).await;
        });
    }
}
