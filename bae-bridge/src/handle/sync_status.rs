//! The `AppHandle` surface for cloud sync: the loop's status and the operations
//! it left waiting, everything sync is doing, and the upload outbox.

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

    /// Everything sync is doing, for the activity indicator and its list:
    /// the value now, then one after each change.
    pub fn subscribe_sync_activity(
        &self,
        callback: Box<dyn crate::types::SyncActivityCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.live_subscription(move |services| async move {
            let mut activity = services.subscribe_sync_activity();
            loop {
                match activity.next().await {
                    Ok(value) => {
                        callback.on_value(crate::types::BridgeSyncActivity::from_core(value))
                    }
                    Err(error) => callback.on_error(BridgeError::from_core(error)),
                }
            }
        })
    }

    /// Whether the person paused uploads now, then each time that changes.
    pub fn subscribe_uploads_paused(
        &self,
        callback: Box<dyn crate::types::UploadsPausedCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_query(
            move |services| services.subscribe_uploads_paused(),
            move |result| match result {
                Ok(paused) => callback.on_value(paused),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }
}

forward! { sync this => {
    fn trigger_sync() {
        this.services.trigger_sync();
    }

    /// Report that the device's network may be back. Apps call it on a
    /// transition, not continuously: when the platform's network monitor
    /// reports the network became available or changed, when the app returns
    /// to the foreground, and when the device wakes. While the network was
    /// cut off (Android blocks a locked phone's background network), every
    /// attempt failed without a provider answer and backed off up to five
    /// minutes; this ends those backoffs — the connection's, operations',
    /// uploads' and downloads' — and runs a pass at once. Backoffs after a
    /// provider's answer and provider cooldowns hold. It also requests a pass,
    /// so a caller need not also call `trigger_sync`.
    fn report_network_change() {
        this.services.network_changed();
    }

    fn is_sync_ready() -> bool {
        this.services.is_sync_ready()
    }

    fn get_sync_status() -> BridgeSyncStatusSnapshot {
        crate::types::BridgeSyncStatusSnapshot::from_core(this.services.get_sync_status())
    }


} }

forward! { async this => {
    /// The person asked to retry everything now: pulling to refresh, or
    /// Retry Now in the sync activity list, which the list shows only while
    /// `BridgeSyncActivity::retry_offered` says it runs a pass that can clear
    /// a failure.
    /// Ends every ordinary backoff, answered or not, and runs a pass;
    /// provider cooldowns and failures that need the person's action hold. Automatic triggers call `trigger_sync`.
    /// Returns when the first pass after the request ends, with what came of
    /// it; a pass held by a cooldown or such a failure answers at once.
    fn retry_now() -> crate::types::BridgeSyncRetryOutcome {
        Ok(crate::types::BridgeSyncRetryOutcome::from_core(
            this.services.retry_now().await,
        ))
    }

    /// End the release's failed move to the cloud, `id`, as its pending
    /// record offers (`BridgePendingAction::CancelUpload`): the release is
    /// local here again.
    fn cancel_failed_release_move(release_id: String, id: i64) -> () {
        Ok(this
            .services
            .cancel_failed_release_move(&release_id, coven::MoveId(id))
            .await?)
    }
    /// Retry sync with the provider this library already has configured: connect
    /// if a failed startup left no connection, then run a cycle now. The failure
    /// reaches the caller and is recorded as the sync-status error the failure
    /// banner reads.
    fn reconnect_sync() -> () {
        this.services
            .reconnect_sync()
            .await
            .map_err(BridgeError::from)
    }

    /// Hand one operation from `BridgeSyncStatusSnapshot::pending` back to the
    /// sync loop, which revalidates it and runs a cycle. `id` is the one carried
    /// on that operation. The operation leaves the list on the next status the
    /// UI receives; an operation whose cause still stands simply blocks again,
    /// and an id that names nothing blocked fails here.
    fn retry_pending_operation(id: i64) -> () {
        this.services
            .retry_pending_operation(coven::OperationId(id))
            .await
            .map_err(BridgeError::from)
    }

    /// Whether the person paused uploads, as it stands now.
    fn get_uploads_paused() -> bool {
        Ok(this.services.is_sync_paused().await?)
    }

    /// Retry failed uploads now: drain coven's upload queue immediately instead
    /// of waiting for the next sync cycle.
    fn retry_outbox() -> () {
        this.services
            .retry_outbox_now()
            .await
            .map_err(BridgeError::from)
    }

    /// Cancel whatever transition a release is mid-flight — a pin (download), a
    /// remote upload, or an audio detachment — leaving it in its prior state. The UI
    /// calls this from the storage row and the queue pane without knowing which
    /// is running; a no-op if nothing is in progress.
    fn cancel_release_transition(release_id: String) -> () {
        this.services
            .cancel_release_transition(&release_id)
            .await
            .map_err(BridgeError::from)
    }

    /// Set coven's persistent upload pause state. New files remain queued
    /// while paused; metadata sync continues.
    fn set_sync_paused(paused: bool) -> () {
        this.services.set_sync_paused(paused).await?;
        Ok(())
    }
} }
