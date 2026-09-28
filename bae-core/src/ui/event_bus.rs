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

    /// Wire to the import runtime: each candidate's extracted signals and the
    /// sidebar header's identification and import counts, as they change.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn wire_import(
        &self,
        app_services: &crate::library::AppServices,
        runtime_handle: &tokio::runtime::Handle,
    ) {
        use crate::import::candidate_runtime::RuntimeValue;
        let mut values = app_services.watch_import_runtime_values();
        let bus = self.clone();
        runtime_handle.spawn(async move {
            while let Some(changed) = values.next().await {
                for value in changed {
                    bus.emit(match value {
                        RuntimeValue::Signals { key, signals } => {
                            UiBusEvent::CandidateSignalsUpdated { key, signals }
                        }
                        RuntimeValue::IdentificationProgress { identified, total } => {
                            UiBusEvent::ImportIdentificationProgress { identified, total }
                        }
                        RuntimeValue::ImportsInFlight { count } => {
                            UiBusEvent::ImportsInFlight { count }
                        }
                    });
                }
            }
        });
    }
}
