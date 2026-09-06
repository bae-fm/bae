//! bae's `BlobTransitionObserver` — UI bookkeeping only.
//!
//! coven owns the whole blob lifecycle: the upload drain, the make-Remote gate
//! flip + local ownership cleanup, and the make-Local materialize + retract.
//! User-provided source files remain untouched; coven drops their external
//! references once the release is Remote. This observer only *reports* what
//! coven did, so the UI stays current:
//!
//! - preparation and upload callbacks record what moved into the shared
//!   [`LiveUploads`], then tell the database-owning sync controller which UI
//!   projection changed;
//! - durable queue changes, including terminal publication, arrive through
//!   coven's cloud-outbox live query rather than lifecycle callbacks.
//!
//! The pause state `LiveUploads` carries lets coven suspend active preparation
//! and provider futures without touching the durable queue or discarding open
//! upload sessions.
use std::future::Future;
use std::sync::Weak;

use coven::BlobTransitionObserver;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::library::live_uploads::LiveUploads;

struct UploadObserverMessage {
    completed: oneshot::Sender<()>,
}

/// Receives the observer's facts and keeps each coven callback waiting until
/// the database-owning consumer has rebuilt the corresponding UI projection.
pub(crate) struct UploadObserverEvents {
    receiver: mpsc::UnboundedReceiver<UploadObserverMessage>,
}

impl UploadObserverEvents {
    pub(crate) async fn run<F, Fut>(mut self, mut process: F)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = ()>,
    {
        while let Some(message) = self.receiver.recv().await {
            process().await;
            if message.completed.send(()).is_err() {
                debug!("upload observer callback ended before its UI projection completed");
            }
        }
    }
}

/// Reports coven's blob transitions to the outbox value stream while a
/// make-Remote upload runs and when a transition completes.
///
/// Every fact a callback carries lands in `uploads`, the live upload state this
/// observer shares with the `LibraryManager`'s sync controller, so the outbox
/// snapshot reports the active state and drives the per-file bar.
/// Database-backed projection work is sent to the sync controller rather than
/// retaining or exposing its database.
pub struct ReleaseUploadObserver {
    uploads: LiveUploads,
    events: mpsc::UnboundedSender<UploadObserverMessage>,
}

impl ReleaseUploadObserver {
    pub(crate) fn new(uploads: LiveUploads) -> (Self, UploadObserverEvents) {
        let (events, receiver) = mpsc::unbounded_channel();
        (Self { uploads, events }, UploadObserverEvents { receiver })
    }

    async fn report(&self) {
        let (completed, completion) = oneshot::channel();
        if self
            .events
            .send(UploadObserverMessage { completed })
            .is_err()
        {
            warn!("upload observer event processor stopped");
            return;
        }
        if completion.await.is_err() {
            warn!("upload observer event processor dropped a lifecycle update");
        }
    }
}

#[async_trait::async_trait]
impl coven::BlobTransitionObserver for ReleaseUploadObserver {
    async fn on_blob_preparation_started(&self, upload: &coven::RowBlobRef) {
        self.uploads.preparation_started(upload);
        self.report().await;
    }

    async fn on_blob_preparation_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        self.uploads
            .preparation_progress(upload, bytes_done, bytes_total);
        self.report().await;
    }

    async fn on_blob_upload_started(&self, upload: &coven::RowBlobRef) {
        self.uploads.upload_started(upload);
        self.report().await;
    }

    async fn on_blob_upload_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        // coven coalesces these calls to a tick, so each is already throttled —
        // emit the snapshot on every one to move the bar.
        self.uploads
            .upload_progress(upload, bytes_done, bytes_total);
        self.report().await;
    }

    async fn on_blob_uploaded(&self, upload: &coven::RowBlobRef) {
        self.uploads.upload_finished(upload);
        self.report().await;
    }

    async fn on_blob_upload_failed(&self, upload: &coven::RowBlobRef, _error: &str) {
        self.uploads.upload_failed(upload);
        // coven's drain records the attempt count and the error on its own
        // queue entry; the snapshot we emit here reads them back.
        self.report().await;
    }

    fn should_skip_uploads(&self) -> bool {
        self.uploads.is_paused()
    }

    async fn wait_until_uploads_paused(&self) {
        self.uploads.wait_until_paused().await;
    }

    async fn wait_until_uploads_resumed(&self) {
        self.uploads.wait_until_resumed().await;
    }

    /// coven finished making a root Local (blobs materialized to local files,
    /// gate retracted, cloud blobs queued for tombstoning): refresh the outbox
    /// snapshot for every root because the retraction changed the queue.
    async fn on_root_made_local(&self, root_table: &str, root_id: &str) {
        if root_table == "releases" {
            self.report().await;
        } else {
            debug!("on_root_made_local for non-release root {root_table:?}/{root_id}");
            self.report().await;
        }
    }
}

/// The `BlobTransitionObserver` coven actually holds, keeping only a [`Weak`]
/// reference to the real [`ReleaseUploadObserver`].
///
/// coven's builder takes the observer as a strong `Arc`. The observer's event
/// receiver is driven by a task that owns the sync controller, whose database
/// retains the Coven handle. Registering the observer directly would therefore
/// close a strong cycle that outlives every `LibraryManager`, so
/// coven's exclusive store-open lock would never release and an in-process
/// reopen of the same store fails with "store is already open". Registering this
/// weak adapter instead leaves the observer owned solely by the `LibraryManager`
/// (see `LibraryManager::open`): when the last manager clone drops, the observer
/// drops, its event sender closes, the task exits, and the lock is released.
///
/// A callback that arrives after the observer is gone — teardown racing a late
/// transition — upgrades to `None` and is a no-op: there is no UI left to notify.
pub(crate) struct WeakUploadObserver {
    inner: Weak<ReleaseUploadObserver>,
}

impl WeakUploadObserver {
    pub(crate) fn new(inner: Weak<ReleaseUploadObserver>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl BlobTransitionObserver for WeakUploadObserver {
    async fn on_blob_preparation_started(&self, blob: &coven::RowBlobRef) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_preparation_started(blob).await;
        }
    }

    async fn on_blob_preparation_progress(
        &self,
        blob: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        if let Some(observer) = self.inner.upgrade() {
            observer
                .on_blob_preparation_progress(blob, bytes_done, bytes_total)
                .await;
        }
    }

    async fn on_blob_upload_started(&self, blob: &coven::RowBlobRef) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_upload_started(blob).await;
        }
    }

    async fn on_blob_upload_progress(
        &self,
        blob: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        if let Some(observer) = self.inner.upgrade() {
            observer
                .on_blob_upload_progress(blob, bytes_done, bytes_total)
                .await;
        }
    }

    async fn on_blob_uploaded(&self, blob: &coven::RowBlobRef) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_uploaded(blob).await;
        }
    }

    async fn on_blob_upload_failed(&self, blob: &coven::RowBlobRef, error: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_upload_failed(blob, error).await;
        }
    }

    fn should_skip_uploads(&self) -> bool {
        self.inner
            .upgrade()
            .is_some_and(|observer| observer.should_skip_uploads())
    }

    async fn wait_until_uploads_paused(&self) {
        match self.inner.upgrade() {
            Some(observer) => observer.wait_until_uploads_paused().await,
            None => std::future::pending::<()>().await,
        }
    }

    async fn wait_until_uploads_resumed(&self) {
        if let Some(observer) = self.inner.upgrade() {
            observer.wait_until_uploads_resumed().await;
        }
    }

    async fn on_root_made_local(&self, root_table: &str, root_id: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_root_made_local(root_table, root_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn observer() -> (ReleaseUploadObserver, UploadObserverEvents, LiveUploads) {
        let uploads = LiveUploads::new();
        let (observer, events) = ReleaseUploadObserver::new(uploads.clone());
        (observer, events, uploads)
    }

    fn test_blob() -> coven::RowBlobRef {
        coven::RowBlobRef::new(
            crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
            "00415c7f-b363-4ed9-8aad-422b93e974e9".to_string(),
            "0000000001000-0000-device-a".to_string(),
            "blob_id".to_string(),
            coven::BlobRef {
                namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
                id: "357d9eb4-a021-4555-8713-0bc652d83c65".to_string(),
                scope: coven::BlobScope::Master,
                cloud_path: None,
                provenance: coven::Provenance::HostProvided,
                fill: coven::CacheFill::CacheEager,
            },
            1000,
            coven::ObjectHash::digest(b"upload-observer-test"),
            coven::RowBlobAuthority::Local,
            None,
        )
        .expect("valid observer test blob")
    }

    #[tokio::test]
    async fn pause_waiters_wake_on_each_absolute_state_change() {
        let (observer, _events, uploads) = observer();
        let observer = Arc::new(observer);

        let waiting_for_pause = tokio::spawn({
            let observer = observer.clone();
            async move { observer.wait_until_uploads_paused().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting_for_pause.is_finished());
        uploads.set_paused(true);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting_for_pause)
            .await
            .expect("pause notification")
            .expect("pause waiter task");

        let waiting_for_resume = tokio::spawn({
            let observer = observer.clone();
            async move { observer.wait_until_uploads_resumed().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting_for_resume.is_finished());
        uploads.set_paused(false);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting_for_resume)
            .await
            .expect("resume notification")
            .expect("resume waiter task");
    }

    #[tokio::test]
    #[should_panic(expected = "upload started while the same blob already had transient state")]
    async fn one_blob_cannot_start_two_provider_transfers() {
        let (observer, events, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_started(&blob).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    async fn provider_callbacks_advance_exact_transient_bytes_and_end_throughput() {
        let (observer, events, uploads) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(
                crate::library::outbox_snapshot::TransientUploadState::Uploading {
                    bytes_done: 600,
                    bytes_total: 1016,
                }
            )
        );
        assert!(uploads.rates_for_test().aggregate_bps > 0);

        observer.on_blob_upload_progress(&blob, 1016, 1016).await;
        observer.on_blob_uploaded(&blob).await;
        assert_eq!(uploads.transient_state_for_test(&blob), None);
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    async fn completed_preparation_can_enter_provider_upload() {
        let (observer, events, uploads) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 1000, 1000)
            .await;
        let rates = uploads.rates_for_test();
        assert!(rates.aggregate_bps > 0);
        assert_eq!(rates.provider_bps, 0);
        observer.on_blob_upload_started(&blob).await;

        assert_eq!(
            uploads.transient_state_for_test(&blob),
            Some(crate::library::outbox_snapshot::TransientUploadState::UploadStarted)
        );
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    async fn failed_preparation_ends_its_throughput_measurement() {
        let (observer, events, uploads) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 500, 1000)
            .await;
        assert!(uploads.rates_for_test().aggregate_bps > 0);

        observer
            .on_blob_upload_failed(&blob, "preparation failed")
            .await;
        assert_eq!(uploads.transient_state_for_test(&blob), None);
        assert_eq!(uploads.rates_for_test().aggregate_bps, 0);

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    #[should_panic(expected = "upload progress arrived without an upload-start state")]
    async fn provider_progress_requires_upload_start() {
        let (observer, events, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_progress(&blob, 600, 1016).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    #[should_panic(expected = "upload completion arrived without exact provider byte progress")]
    async fn provider_completion_requires_the_exact_final_progress() {
        let (observer, events, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_uploaded(&blob).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    #[should_panic(expected = "provider progress regressed or changed its exact total")]
    async fn first_provider_progress_cannot_exceed_its_total() {
        let (observer, events, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 1_017, 1_016).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    #[should_panic(expected = "provider progress regressed or changed its exact total")]
    async fn provider_progress_is_monotonic_with_one_exact_total() {
        let (observer, events, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        observer.on_blob_upload_progress(&blob, 500, 1016).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }
}
