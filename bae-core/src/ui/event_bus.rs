use super::*;
use tokio::sync::broadcast;

/// Central event bus for UI events: subscribes to the service channels and
/// translates their domain events into `UiBusEvent`s, which the bridge forwards
/// to the native reducer. Clone-cheap.
#[derive(Clone)]
pub struct UiEventBus {
    tx: broadcast::Sender<UiBusEvent>,
}

impl UiEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self { tx }
    }

    pub fn emit(&self, event: UiBusEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UiBusEvent> {
        self.tx.subscribe()
    }

    /// Wire the bus to every service channel, spawning one forwarding task per
    /// channel. Call once at startup.
    pub fn wire(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        self.wire_playback(app_services, runtime_handle);
        // Import/scan/identify events come from the desktop-only import service.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.wire_import(app_services, runtime_handle);
    }

    fn wire_playback(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let mut rx = app_services.subscribe_playback_progress();
        let bus = self.clone();
        runtime_handle.spawn(async move {
            use crate::playback::PlaybackProgress;

            while let Some(event) = rx.recv().await {
                match event {
                    PlaybackProgress::QueueItemsAdded { count } => {
                        bus.emit(UiBusEvent::QueueItemsAdded { count });
                    }
                    PlaybackProgress::PlaybackError { reason } => {
                        bus.emit(UiBusEvent::PlaybackError { reason });
                    }
                    _ => {}
                }
            }
        });
    }

    /// Wire to the import event channel: scan events, import and loudness
    /// progress, identify-state changes, and extracted signals.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn wire_import(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        self.wire_import_events(
            app_services.subscribe_import_events(),
            app_services.clone(),
            runtime_handle,
        );
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn wire_import_events(
        &self,
        mut rx: broadcast::Receiver<crate::import::ImportEvent>,
        services: crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        let bus = self.clone();

        runtime_handle.spawn(async move {
            use crate::import::ImportEvent;

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        match event {
                            #[cfg(not(any(target_os = "ios", target_os = "android")))]
                            ImportEvent::ImportLoudnessProgress {
                                candidate_key,
                                tracks_done,
                                tracks_total,
                                fraction,
                            } => {
                                bus.emit(UiBusEvent::CandidateImportLoudnessProgress {
                                    key: candidate_key,
                                    tracks_done,
                                    tracks_total,
                                    fraction,
                                });
                            }
                            // The sidebar header's line and bar. It crosses as
                            // its own event rather than as a catalog value:
                            // it is two numbers, it changes once per candidate
                            // answered, and nothing about the row list changes
                            // with it.
                            #[cfg(not(any(target_os = "ios", target_os = "android")))]
                            ImportEvent::QueueIdentifyProgress { identified, total } => {
                                bus.emit(UiBusEvent::ImportQueueIdentifyProgress {
                                    identified,
                                    total,
                                });
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Import event bus lagged by {n} events");
                        services.record_telemetry(crate::diagnostics::TelemetryEvent::Anomaly {
                            kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                        });
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}
