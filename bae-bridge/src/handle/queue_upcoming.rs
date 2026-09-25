use super::*;

/// One requested window of the context's upcoming tail, with its entries.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingWindow {
    pub window: crate::types::BridgeLibraryPageWindow,
    pub entries: Vec<crate::types::BridgeQueueEntry>,
}

/// Every requested window of the upcoming tail as of one queue revision.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingSnapshot {
    /// The queue revision the windows were sliced from. The UI shows them
    /// only while its queue snapshot carries the same revision.
    pub revision: u64,
    pub windows: Vec<BridgeQueueUpcomingWindow>,
}

/// The context's upcoming tail past the queue snapshot's first window, read
/// in the windows the UI asks for: one live read for as long as the queue is
/// shown, moved in place as it scrolls and as the queue changes.
#[derive(uniffi::Object)]
pub struct QueueUpcomingSubscription {
    inner: bae_core::library::QueueUpcomingSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    /// A subscription that reads no windows until `set_windows`.
    pub fn subscribe_queue_upcoming(&self) -> std::sync::Arc<QueueUpcomingSubscription> {
        std::sync::Arc::new(QueueUpcomingSubscription {
            inner: self.services.subscribe_queue_upcoming(&self.runtime),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl QueueUpcomingSubscription {
    /// Read `windows` of the upcoming tail from now on; offsets count from
    /// the first entry after the playing track.
    pub fn set_windows(
        &self,
        windows: Vec<crate::types::BridgeLibraryPageWindow>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set_windows(
                windows
                    .into_iter()
                    .map(crate::types::BridgeLibraryPageWindow::into_core)
                    .collect(),
            )
            .map_err(upcoming_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgeQueueUpcomingSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(BridgeQueueUpcomingSnapshot::from_core)
                .map_err(upcoming_error)
        })
        .await
    }

    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner.cancel();
            Ok(())
        })
        .await
    }
}

impl BridgeQueueUpcomingSnapshot {
    fn from_core(snapshot: bae_core::library::QueueUpcomingSnapshot) -> Self {
        Self {
            revision: snapshot.revision,
            windows: snapshot
                .windows
                .into_iter()
                .map(|window| BridgeQueueUpcomingWindow {
                    window: crate::types::BridgeLibraryPageWindow::from_core(window.window),
                    entries: window
                        .items
                        .into_iter()
                        .map(crate::types::BridgeQueueEntry::from_core)
                        .collect(),
                })
                .collect(),
        }
    }
}

fn upcoming_error(error: bae_core::library::QueueUpcomingSubscriptionError) -> BridgeError {
    match error {
        bae_core::library::QueueUpcomingSubscriptionError::Cancelled => BridgeError::Cancelled,
        bae_core::library::QueueUpcomingSubscriptionError::Query(error) => {
            BridgeError::internal(error)
        }
    }
}
