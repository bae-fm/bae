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
                key: key.into_bytes(),
                signals: crate::types::BridgeSignals::from_core(signals),
            })
        }
        #[cfg(all(not(feature = "desktop"), desktop))]
        UiEvent::CandidateSignalsUpdated { .. } => None,
    }
}

#[uniffi::export]
impl AppHandle {
    /// Hand `callback` each UI event core raises, until the library closes.
    /// An event the host's callback throws on is reported, and the next one
    /// is still delivered; a panic in the delivery itself is reported, and
    /// ends it.
    pub fn subscribe_ui_events(&self, callback: Box<dyn crate::types::UiEventCallback>) {
        let mut events = self.services.subscribe_ui_events();
        let diagnostics = self.diagnostics.clone();
        self.runtime.spawn_reported(
            "deliver UI events",
            async move {
                while let Some(heard) = events.next().await {
                    for event in heard {
                        if let Some(bridge_event) = convert_ui_event(event) {
                            if let Err(failed) = crate::host_calls::host_call(
                                |failed| diagnostics.bug(failed),
                                "deliver a UI event to the host",
                                || callback.on_event(bridge_event),
                            ) {
                                tracing::debug!("the host missed a UI event: {failed}");
                            }
                        }
                    }
                }
            },
            // The callback has no way to hear that its events ended, and the
            // panic is reported.
            |_| {},
        );
    }
}
