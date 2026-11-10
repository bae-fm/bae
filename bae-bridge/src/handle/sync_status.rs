//! The `AppHandle` surface for cloud sync: the loop's status and the operations
//! it left waiting, everything sync is doing, and the upload outbox.

use super::*;

#[uniffi::export]
impl AppHandle {
    /// What sync is doing and what it waits on, now and after each change.
    pub fn subscribe_sync_status(&self) -> std::sync::Arc<crate::SyncStatusSubscription> {
        crate::screens::sync_status(self.services.subscribe_sync_status(), self.runtime.clone())
    }

    /// Everything sync is doing, for the activity indicator and its list:
    /// the value now, then one after each change.
    pub fn subscribe_sync_activity(&self) -> std::sync::Arc<crate::SyncActivitySubscription> {
        crate::screens::sync_activity(self.services.subscribe_sync_activity(), self.runtime.clone())
    }

    /// Whether the person paused uploads now, then each time that changes.
    pub fn subscribe_uploads_paused(&self) -> std::sync::Arc<crate::UploadsPausedSubscription> {
        crate::screens::uploads_paused(
            self.services.subscribe_uploads_paused(),
            self.runtime.clone(),
        )
    }

    /// Whether files move only on unmetered networks now, then each time
    /// that changes.
    pub fn subscribe_unmetered_only(&self) -> std::sync::Arc<crate::UnmeteredOnlySubscription> {
        crate::screens::unmetered_only(
            self.services.subscribe_unmetered_only(),
            self.runtime.clone(),
        )
    }
}

forward! { sync this => {
    fn trigger_sync() {
        this.services.trigger_sync();
    }

    /// Cancel the storage setup running on this library. Before its point
    /// of no return it is undone, and its call fails as cancelled; past it,
    /// from its `CreatingStore` or `RecordingAccess` step on, it finishes,
    /// and Cancel no longer applies. Leaving a setup's screen while it runs
    /// cancels it this way: dropping the call alone leaves it running to
    /// its end.
    fn cancel_storage_setup() -> crate::types::BridgeSetupCancellation {
        crate::types::BridgeSetupCancellation::from_core(this.services.cancel_storage_setup())
    }
} }

forward! { async this => {
    /// The person asked to retry everything now: pulling to refresh, or
    /// Retry now on the Sync page and the status line.
    /// Ends every ordinary backoff, answered or not, and runs a pass;
    /// provider cooldowns and failures that need the person's action hold. Automatic triggers call `trigger_sync`.
    /// Returns when the first pass after the request ends, with what came of
    /// it; a pass held by a cooldown or such a failure answers at once.
    fn retry_now() -> crate::types::BridgeSyncRetryOutcome {
        Ok(crate::types::BridgeSyncRetryOutcome::from_core(
            this.services.retry_now().await?.await,
        ))
    }

    /// End a release's failed move to the cloud, `id`, as its pending
    /// record offers (`BridgePendingAction::CancelUpload`): the release stays
    /// on this device only, as it was.
    fn cancel_failed_release_move(id: i64) -> () {
        Ok(this
            .services
            .cancel_failed_release_move(coven::MoveId(id))
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

    /// Dismiss `BridgeSyncStatusSnapshot::undone_setup`, the storage setup
    /// `operation` that ended undone: the status no longer shows it.
    fn dismiss_undone_storage_setup(operation: i64) -> () {
        this.services
            .dismiss_undone_storage_setup(coven::OperationId(operation))
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

    /// Whether files move only on unmetered networks, as it stands now.
    fn get_unmetered_only() -> bool {
        Ok(this.services.is_unmetered_only().await?)
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

    /// Have uploads and downloads of release files wait on a metered network,
    /// or move on any. Saved by coven, so it lasts across restarts.
    fn set_unmetered_only(unmetered_only: bool) -> () {
        this.services.set_unmetered_only(unmetered_only).await?;
        Ok(())
    }
} }
