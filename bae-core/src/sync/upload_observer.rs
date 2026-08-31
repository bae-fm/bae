//! bae's `BlobTransitionObserver` — UI bookkeeping only.
//!
//! coven owns the whole blob lifecycle: the upload drain, the make-Remote gate
//! flip + local ownership cleanup, and the make-Local materialize + retract.
//! User-provided source files remain untouched; coven drops their external
//! references once the release is Remote. This observer only *reports* what
//! coven did, so the UI stays current:
//!
//! - preparation and upload callbacks drive the transient map and the rolling-window
//!   `throughput`, then tell the database-owning sync controller which UI
//!   projection changed;
//! - durable queue changes, including terminal publication, arrive through
//!   coven's cloud-outbox live query rather than lifecycle callbacks.
//!
//! The pause watch lets coven suspend active preparation and provider futures
//! without touching the durable queue or discarding open upload sessions.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};

use coven::BlobTransitionObserver;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

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
/// `transient` maps each exact blob to preparation or provider-transfer bytes,
/// shared with the `LibraryManager` so its outbox snapshot reports the active
/// state and drives the per-file bar.
/// `throughput` records byte deltas while preparation or provider transfer is
/// active, resetting the blob's measurement at their boundary.
/// Database-backed projection work is sent to the sync controller rather than
/// retaining or exposing its database.
pub struct ReleaseUploadObserver {
    transient: Arc<
        Mutex<
            HashMap<
                crate::library::outbox_snapshot::UploadBlobKey,
                crate::library::outbox_snapshot::TransientUploadState,
            >,
        >,
    >,
    throughput: Arc<crate::library::UploadThroughput>,
    sync_paused: tokio::sync::watch::Sender<bool>,
    events: mpsc::UnboundedSender<UploadObserverMessage>,
}

impl ReleaseUploadObserver {
    pub(crate) fn new(
        transient: Arc<
            Mutex<
                HashMap<
                    crate::library::outbox_snapshot::UploadBlobKey,
                    crate::library::outbox_snapshot::TransientUploadState,
                >,
            >,
        >,
        throughput: Arc<crate::library::UploadThroughput>,
        sync_paused: tokio::sync::watch::Sender<bool>,
    ) -> (Self, UploadObserverEvents) {
        let (events, receiver) = mpsc::unbounded_channel();
        (
            Self {
                transient,
                throughput,
                sync_paused,
                events,
            },
            UploadObserverEvents { receiver },
        )
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

fn upload_blob_key(upload: &coven::RowBlobRef) -> crate::library::outbox_snapshot::UploadBlobKey {
    crate::library::outbox_snapshot::UploadBlobKey::from_row(upload)
}

async fn wait_for_upload_pause_state(pause_state: &tokio::sync::watch::Sender<bool>, target: bool) {
    let mut pause_state = pause_state.subscribe();
    loop {
        if *pause_state.borrow_and_update() == target {
            return;
        }
        pause_state
            .changed()
            .await
            .expect("release upload observer owns the pause sender");
    }
}

#[async_trait::async_trait]
impl coven::BlobTransitionObserver for ReleaseUploadObserver {
    async fn on_blob_preparation_started(&self, upload: &coven::RowBlobRef) {
        let blob_key = upload_blob_key(upload);
        {
            use std::collections::hash_map::Entry;
            let mut transient = self.transient.lock().unwrap();
            match transient.entry(blob_key.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(
                        crate::library::outbox_snapshot::TransientUploadState::Preparing {
                            bytes_done: 0,
                            bytes_total: upload.plaintext_size(),
                        },
                    );
                }
                Entry::Occupied(entry) => panic!(
                    "preparation started while the same blob already had transient state for {}:{}; state: {:?}",
                    upload.table(),
                    upload.row_id(),
                    entry.get()
                ),
            }
        }
        self.throughput.begin_preparation(blob_key);
        self.report().await;
    }

    async fn on_blob_preparation_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        let blob_key = upload_blob_key(upload);
        let delta = {
            let mut transient = self.transient.lock().unwrap();
            match transient.get_mut(&blob_key) {
                Some(crate::library::outbox_snapshot::TransientUploadState::Preparing {
                    bytes_done: previous,
                    bytes_total: exact_total,
                }) => {
                    if bytes_total != *exact_total || bytes_total != upload.plaintext_size() {
                        panic!(
                            "preparation progress changed its exact plaintext total for {}:{}: \
                             expected {}, received {bytes_total}",
                            upload.table(),
                            upload.row_id(),
                            *exact_total
                        );
                    }
                    if bytes_done < *previous || bytes_done > bytes_total {
                        panic!(
                            "preparation progress regressed or exceeded its exact total for {}:{}: \
                             previous {}, received {bytes_done} of {bytes_total}",
                            upload.table(),
                            upload.row_id(),
                            *previous
                        );
                    }
                    let delta = bytes_done - *previous;
                    *previous = bytes_done;
                    delta
                }
                state => panic!(
                    "preparation progress arrived without a preparation-start state for {}:{}; \
                     transient state: {state:?}",
                    upload.table(),
                    upload.row_id()
                ),
            }
        };
        if delta > 0 {
            self.throughput.record_preparation(&blob_key, delta);
        }
        self.report().await;
    }

    async fn on_blob_upload_started(&self, upload: &coven::RowBlobRef) {
        let blob_key = upload_blob_key(upload);
        let uploading = crate::library::outbox_snapshot::TransientUploadState::UploadStarted;
        {
            use std::collections::hash_map::Entry;
            let mut transient = self.transient.lock().unwrap();
            match transient.entry(blob_key.clone()) {
                Entry::Vacant(entry) => {
                    // A restart can resume directly from coven's durable
                    // Prepared state, so no preparation callback is required in
                    // this process.
                    entry.insert(uploading);
                }
                Entry::Occupied(mut entry) => match entry.get() {
                    crate::library::outbox_snapshot::TransientUploadState::Preparing {
                        bytes_done,
                        bytes_total,
                    } if bytes_done == bytes_total => {
                        entry.insert(uploading);
                    }
                    state => panic!(
                        "upload started while the same blob already had transient state for {}:{}; state: {state:?}",
                        upload.table(),
                        upload.row_id()
                    ),
                },
            }
        }
        self.throughput.begin_upload(blob_key);
        self.report().await;
    }

    async fn on_blob_upload_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        if bytes_total == 0 || bytes_done > bytes_total {
            panic!(
                "provider progress regressed or changed its exact total for {}:{}: \
                 received {bytes_done} of {bytes_total}",
                upload.table(),
                upload.row_id()
            );
        }
        let blob_key = upload_blob_key(upload);
        // Feed the throughput tracker only the bytes new since the last report.
        // coven coalesces these calls to a tick, so each is already throttled —
        // emit the snapshot on every one to move the bar. The counts are
        // cumulative within an attempt; invalid ordering fails loudly below.
        let delta = {
            let mut map = self.transient.lock().unwrap();
            match map.get_mut(&blob_key) {
                Some(
                    state @ crate::library::outbox_snapshot::TransientUploadState::UploadStarted,
                ) => {
                    *state = crate::library::outbox_snapshot::TransientUploadState::Uploading {
                        bytes_done,
                        bytes_total,
                    };
                    bytes_done
                }
                Some(crate::library::outbox_snapshot::TransientUploadState::Uploading {
                    bytes_done: previous,
                    bytes_total: previous_total,
                }) => {
                    if bytes_total == 0
                        || bytes_done < *previous
                        || bytes_done > bytes_total
                        || bytes_total != *previous_total
                    {
                        panic!(
                            "provider progress regressed or changed its exact total for {}:{}: \
                             previous {} of {}, received {bytes_done} of {bytes_total}",
                            upload.table(),
                            upload.row_id(),
                            *previous,
                            *previous_total
                        );
                    }
                    let delta = bytes_done - *previous;
                    *previous = bytes_done;
                    *previous_total = bytes_total;
                    delta
                }
                state => panic!(
                    "upload progress arrived without an upload-start state for {}:{}; \
                     transient state: {state:?}",
                    upload.table(),
                    upload.row_id()
                ),
            }
        };
        if delta > 0 {
            self.throughput.record_upload(&blob_key, delta);
        }
        self.report().await;
    }

    async fn on_blob_uploaded(&self, upload: &coven::RowBlobRef) {
        // Coven emits a final provider-progress report with the exact encrypted
        // total before completion. Requiring that report keeps provider bytes
        // distinct from plaintext source bytes; substituting the source size
        // here would make both the file row and throughput false.
        let blob_key = upload_blob_key(upload);
        match self.transient.lock().unwrap().remove(&blob_key) {
            Some(crate::library::outbox_snapshot::TransientUploadState::Uploading {
                bytes_done,
                bytes_total,
            }) if bytes_total > 0 && bytes_done == bytes_total => {}
            state => panic!(
                "upload completion arrived without exact provider byte progress for {}:{}; \
                 transient state: {state:?}",
                upload.table(),
                upload.row_id()
            ),
        }
        // Coven committed this exact row journal as Created before calling us;
        // the durable outbox now owns its Uploaded state. Keep no transient
        // terminal copy that could survive or disagree with that commit.
        self.throughput.end(&blob_key);
        self.report().await;
    }

    async fn on_blob_upload_failed(&self, upload: &coven::RowBlobRef, _error: &str) {
        let blob_key = upload_blob_key(upload);
        let removed = self.transient.lock().unwrap().remove(&blob_key);
        if removed.is_some() {
            self.throughput.end(&blob_key);
        }
        // coven's drain records the attempt count and the error on its own
        // queue entry; the snapshot we emit here reads them back.
        self.report().await;
    }

    fn should_skip_uploads(&self) -> bool {
        *self.sync_paused.borrow()
    }

    async fn wait_until_uploads_paused(&self) {
        wait_for_upload_pause_state(&self.sync_paused, true).await;
    }

    async fn wait_until_uploads_resumed(&self) {
        wait_for_upload_pause_state(&self.sync_paused, false).await;
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

    type TransientMap = Arc<
        Mutex<
            HashMap<
                crate::library::outbox_snapshot::UploadBlobKey,
                crate::library::outbox_snapshot::TransientUploadState,
            >,
        >,
    >;

    fn observer() -> (
        ReleaseUploadObserver,
        UploadObserverEvents,
        TransientMap,
        Arc<crate::library::UploadThroughput>,
    ) {
        let transient = Arc::new(Mutex::new(HashMap::new()));
        let throughput = Arc::new(crate::library::UploadThroughput::new());
        let (paused, _) = tokio::sync::watch::channel(false);
        let (observer, events) =
            ReleaseUploadObserver::new(transient.clone(), throughput.clone(), paused);
        (observer, events, transient, throughput)
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
        let transient = Arc::new(Mutex::new(HashMap::new()));
        let throughput = Arc::new(crate::library::UploadThroughput::new());
        let (pause_state, _) = tokio::sync::watch::channel(false);
        let (observer, _events) =
            ReleaseUploadObserver::new(transient, throughput, pause_state.clone());
        let observer = Arc::new(observer);

        let waiting_for_pause = tokio::spawn({
            let observer = observer.clone();
            async move { observer.wait_until_uploads_paused().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting_for_pause.is_finished());
        pause_state.send_replace(true);
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
        pause_state.send_replace(false);
        tokio::time::timeout(std::time::Duration::from_secs(1), waiting_for_resume)
            .await
            .expect("resume notification")
            .expect("resume waiter task");
    }

    #[tokio::test]
    #[should_panic(expected = "upload started while the same blob already had transient state")]
    async fn one_blob_cannot_start_two_provider_transfers() {
        let (observer, events, _, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_started(&blob).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    async fn provider_callbacks_advance_exact_transient_bytes_and_end_throughput() {
        let (observer, events, transient, throughput) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();
        let key = upload_blob_key(&blob);

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        assert_eq!(
            transient.lock().unwrap().get(&key).copied(),
            Some(
                crate::library::outbox_snapshot::TransientUploadState::Uploading {
                    bytes_done: 600,
                    bytes_total: 1016,
                }
            )
        );
        assert!(throughput.bytes_per_sec() > 0);

        observer.on_blob_upload_progress(&blob, 1016, 1016).await;
        observer.on_blob_uploaded(&blob).await;
        assert_eq!(transient.lock().unwrap().get(&key).copied(), None);
        assert_eq!(throughput.bytes_per_sec(), 0);

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    async fn completed_preparation_can_enter_provider_upload() {
        let (observer, events, transient, throughput) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();
        let key = upload_blob_key(&blob);

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 1000, 1000)
            .await;
        assert!(throughput.bytes_per_sec() > 0);
        assert_eq!(throughput.rates().provider_bps, 0);
        observer.on_blob_upload_started(&blob).await;

        assert_eq!(
            transient.lock().unwrap().get(&key).copied(),
            Some(crate::library::outbox_snapshot::TransientUploadState::UploadStarted)
        );
        assert_eq!(throughput.bytes_per_sec(), 0);

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    async fn failed_preparation_ends_its_throughput_measurement() {
        let (observer, events, transient, throughput) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();
        let key = upload_blob_key(&blob);

        observer.on_blob_preparation_started(&blob).await;
        observer
            .on_blob_preparation_progress(&blob, 500, 1000)
            .await;
        assert!(throughput.bytes_per_sec() > 0);

        observer
            .on_blob_upload_failed(&blob, "preparation failed")
            .await;
        assert_eq!(transient.lock().unwrap().get(&key).copied(), None);
        assert_eq!(throughput.bytes_per_sec(), 0);

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    #[should_panic(expected = "upload progress arrived without an upload-start state")]
    async fn provider_progress_requires_upload_start() {
        let (observer, events, _, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_progress(&blob, 600, 1016).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }

    #[tokio::test]
    #[should_panic(expected = "upload completion arrived without exact provider byte progress")]
    async fn provider_completion_requires_the_exact_final_progress() {
        let (observer, events, _, _) = observer();
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
        let (observer, events, _, _) = observer();
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
        let (observer, events, _, _) = observer();
        let event_task = tokio::spawn(events.run(|| async {}));
        let blob = test_blob();

        observer.on_blob_upload_started(&blob).await;
        observer.on_blob_upload_progress(&blob, 600, 1016).await;
        observer.on_blob_upload_progress(&blob, 500, 1016).await;

        drop(observer);
        event_task.await.expect("observer event task");
    }
}
