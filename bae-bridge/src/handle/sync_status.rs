//! The `AppHandle` surface for cloud sync: the loop's status and the operations
//! it left waiting, the artwork cache fill, and the upload outbox.

use super::*;

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_config(
        &self,
        callback: Box<dyn crate::types::ConfigCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_config_changes(),
            move |value| callback.on_value(BridgeConfig::from_core(value)),
        )
    }

    pub fn subscribe_sync_status(
        &self,
        callback: Box<dyn crate::types::SyncStatusCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_sync_status_values(),
            move |value| callback.on_value(BridgeSyncStatusSnapshot::from_core(value.clone())),
        )
    }

    pub fn subscribe_eager_cache_fill_status(
        &self,
        callback: Box<dyn crate::types::EagerCacheFillStatusCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_eager_cache_fill_status(),
            move |value| {
                callback.on_value(crate::types::BridgeEagerCacheFillStatus::from_core(
                    value.clone(),
                ))
            },
        )
    }

    pub fn subscribe_outbox(
        &self,
        callback: Box<dyn crate::types::OutboxCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.live_subscription(move |services, _| async move {
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
        })
    }
}

forward! { sync this => {
    fn trigger_sync() {
        this.services.trigger_sync();
    }

    fn is_sync_ready() -> bool {
        this.services.is_sync_ready()
    }

    fn get_sync_status() -> BridgeSyncStatusSnapshot {
        crate::types::BridgeSyncStatusSnapshot::from_core(this.services.get_sync_status())
    }

    fn cancel_eager_cache_fill() {
        this.services.cancel_eager_cache_fill();
    }
} }

forward! { async this => {
    /// Retry sync with the provider this library already has configured: connect
    /// if a failed startup left no connection, then run a cycle now. The failure
    /// reaches the caller and is recorded as the sync-status error the failure
    /// banner reads.
    fn reconnect_sync() -> () {
        this.services
            .reconnect_sync()
            .await
            .map_err(BridgeError::internal)
    }

    /// Hand one operation from `BridgeSyncStatusSnapshot::blocked` back to the
    /// sync loop, which revalidates it and runs a cycle. `id` is the one carried
    /// on that operation. The operation leaves the list on the next status the
    /// UI receives; an operation whose cause still stands simply blocks again,
    /// and an id that names nothing blocked fails here.
    fn retry_blocked_sync_operation(id: String) -> () {
        this.services
            .retry_blocked_sync_operation(&id)
            .await
            .map_err(BridgeError::from)
    }

    /// The current cloud outbox processing snapshot.
    fn get_outbox_snapshot() -> crate::types::BridgeOutboxSnapshot {
        let snapshot = this
            .services
            .outbox_snapshot()
            .await
            .map_err(BridgeError::internal)?;
        Ok(crate::types::BridgeOutboxSnapshot::from_core(snapshot))
    }

    /// Retry failed uploads now: drain coven's upload queue immediately instead
    /// of waiting for the next sync cycle.
    fn retry_outbox() -> () {
        this.services
            .retry_outbox_now()
            .await
            .map_err(BridgeError::internal)
    }

    /// Cancel whatever transition a release is mid-flight — a pin (download), a
    /// remote upload, or a make-Local transfer — leaving it in its prior state. The UI
    /// calls this from the storage row and the queue pane without knowing which
    /// is running; a no-op if nothing is in progress.
    fn cancel_release_transition(release_id: String) -> () {
        this.services
            .cancel_release_transition(&release_id)
            .await
            .map_err(BridgeError::internal)
    }

    /// Pause or resume the cloud-upload pipeline. While paused, new enqueues
    /// still land in the outbox but the sync cycle won't drain them; the
    /// snapshot's pause phase changes so the UI can distinguish pausing from
    /// fully paused.
    fn set_sync_paused(paused: bool) -> () {
        this.services.set_sync_paused(paused).await;
        Ok(())
    }
} }
