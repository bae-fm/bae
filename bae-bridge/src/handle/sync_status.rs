//! The `AppHandle` surface for cloud sync: the loop's status and the operations
//! it left waiting, the artwork cache fill, and the upload outbox.

use super::*;

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub fn trigger_sync(&self) {
        self.services.trigger_sync();
    }

    /// Retry sync with the provider this library already has configured: connect
    /// if a failed startup left no connection, then run a cycle now. The failure
    /// reaches the caller and is recorded as the sync-status error the failure
    /// banner reads.
    pub async fn reconnect_sync(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .reconnect_sync()
                .await
                .map_err(BridgeError::internal)
        })
        .await
    }

    pub fn is_sync_ready(&self) -> bool {
        self.services.is_sync_ready()
    }

    pub fn get_sync_status(&self) -> BridgeSyncStatusSnapshot {
        crate::types::BridgeSyncStatusSnapshot::from_core(self.services.get_sync_status())
    }

    /// Hand one operation from `BridgeSyncStatusSnapshot::blocked` back to the
    /// sync loop, which revalidates it and runs a cycle. `id` is the one carried
    /// on that operation. The operation leaves the list on the next status the
    /// UI receives; an operation whose cause still stands simply blocks again,
    /// and an id that names nothing blocked fails here.
    pub async fn retry_blocked_sync_operation(
        self: std::sync::Arc<Self>,
        id: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .retry_blocked_sync_operation(&id)
                .await
                .map_err(BridgeError::from)
        })
        .await
    }

    pub fn subscribe_config(
        &self,
        callback: Box<dyn crate::types::ConfigCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values = services.subscribe_config_changes();
            callback.on_value(BridgeConfig::from_core(&values.borrow_and_update()));
            while values.changed().await.is_ok() {
                callback.on_value(BridgeConfig::from_core(&values.borrow_and_update()));
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    pub fn subscribe_sync_status(
        &self,
        callback: Box<dyn crate::types::SyncStatusCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values = services.subscribe_sync_status_values();
            callback.on_value(BridgeSyncStatusSnapshot::from_core(
                values.borrow_and_update().clone(),
            ));
            while values.changed().await.is_ok() {
                callback.on_value(BridgeSyncStatusSnapshot::from_core(
                    values.borrow_and_update().clone(),
                ));
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    pub fn subscribe_eager_cache_fill_status(
        &self,
        callback: Box<dyn crate::types::EagerCacheFillStatusCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values = services.subscribe_eager_cache_fill_status();
            callback.on_value(crate::types::BridgeEagerCacheFillStatus::from_core(
                values.borrow_and_update().clone(),
            ));
            while values.changed().await.is_ok() {
                callback.on_value(crate::types::BridgeEagerCacheFillStatus::from_core(
                    values.borrow_and_update().clone(),
                ));
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    pub fn cancel_eager_cache_fill(&self) {
        self.services.cancel_eager_cache_fill();
    }

    /// The current cloud outbox processing snapshot.
    pub async fn get_outbox_snapshot(
        self: std::sync::Arc<Self>,
    ) -> Result<crate::types::BridgeOutboxSnapshot, BridgeError> {
        self.run_exported(move |this| async move {
            let snapshot = this
                .services
                .outbox_snapshot()
                .await
                .map_err(BridgeError::internal)?;
            Ok(crate::types::BridgeOutboxSnapshot::from_core(snapshot))
        })
        .await
    }

    pub fn subscribe_outbox(
        &self,
        callback: Box<dyn crate::types::OutboxCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let services = self.services.clone();
        let runtime = self.runtime.handle().clone();
        let task = crate::operation_runtime::spawn(runtime, move || async move {
            let mut values = services.subscribe_outbox_values();
            let current = { values.borrow_and_update().clone() };
            let initial = match current {
                Some(value) => value,
                None => services
                    .outbox_snapshot()
                    .await
                    .map_err(|error| error.to_string()),
            };
            match initial {
                Ok(value) => {
                    callback.on_value(crate::types::BridgeOutboxSnapshot::from_core(value))
                }
                Err(error) => callback.on_error(BridgeError::internal(error)),
            }
            while values.changed().await.is_ok() {
                match values.borrow_and_update().clone() {
                    Some(Ok(value)) => {
                        callback.on_value(crate::types::BridgeOutboxSnapshot::from_core(value))
                    }
                    Some(Err(error)) => callback.on_error(BridgeError::internal(error)),
                    None => {
                        tracing::warn!("outbox value stream published an absent snapshot");
                    }
                }
            }
        });
        std::sync::Arc::new(crate::LiveSubscription::new(task))
    }

    /// Retry failed uploads now: drain coven's upload queue immediately instead
    /// of waiting for the next sync cycle.
    pub async fn retry_outbox(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .retry_outbox_now()
                .await
                .map_err(BridgeError::internal)
        })
        .await
    }

    /// Cancel whatever transition a release is mid-flight — a pin (download), a
    /// remote upload, or a make-Local transfer — leaving it in its prior state. The UI
    /// calls this from the storage row and the queue pane without knowing which
    /// is running; a no-op if nothing is in progress.
    pub async fn cancel_release_transition(
        self: std::sync::Arc<Self>,
        release_id: String,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services
                .cancel_release_transition(&release_id)
                .await
                .map_err(BridgeError::internal)
        })
        .await
    }

    /// Pause or resume the cloud-upload pipeline. While paused, new enqueues
    /// still land in the outbox but the sync cycle won't drain them; the
    /// snapshot's pause phase changes so the UI can distinguish pausing from
    /// fully paused.
    pub async fn set_sync_paused(
        self: std::sync::Arc<Self>,
        paused: bool,
    ) -> Result<(), BridgeError> {
        self.run_exported(move |this| async move {
            this.services.set_sync_paused(paused).await;
            Ok(())
        })
        .await
    }
}
