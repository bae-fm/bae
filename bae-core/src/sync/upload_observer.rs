//! bae's `BlobTransitionObserver` — UI bookkeeping only.
//!
//! coven owns the whole blob lifecycle: the upload drain, the make-Remote gate
//! flip + source-file delete, and the make-Local materialize + retract. This
//! observer only *reports* what coven did, so the UI stays current:
//!
//! - the upload callbacks drive the `in_flight` map and the rolling-window
//!   `throughput`, then tell the database-owning sync controller which UI
//!   projection changed;
//! - `on_root_made_remote` / `on_root_made_local` fire when coven completes a
//!   transition, including one resumed after a restart, so the outbox stream
//!   reflects the completed work.
//!
//! `should_skip_uploads` lets the host pause the upload pipeline without touching
//! the queue.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, Weak};

use coven::BlobTransitionObserver;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

#[derive(Debug)]
pub(crate) enum UploadObserverEvent {
    OutboxChanged,
    BlobUploaded {
        file_id: String,
        already_counted: u64,
    },
    ReleaseMadeRemote {
        release_id: String,
    },
    ReleaseMadeLocal,
}

impl UploadObserverEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::OutboxChanged => "OutboxChanged",
            Self::BlobUploaded { .. } => "BlobUploaded",
            Self::ReleaseMadeRemote { .. } => "ReleaseMadeRemote",
            Self::ReleaseMadeLocal => "ReleaseMadeLocal",
        }
    }
}

struct UploadObserverMessage {
    event: UploadObserverEvent,
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
        F: FnMut(UploadObserverEvent) -> Fut,
        Fut: Future<Output = ()>,
    {
        while let Some(message) = self.receiver.recv().await {
            process(message.event).await;
            if message.completed.send(()).is_err() {
                debug!("upload observer callback ended before its UI projection completed");
            }
        }
    }
}

/// Reports coven's blob transitions to the outbox value stream while a
/// make-Remote upload runs and when a transition completes.
///
/// `in_flight` maps each uploading `file_id` to the encrypted bytes that have
/// reached the cloud for it, shared with the `LibraryManager` so its outbox
/// snapshot reports the "uploading" state and drives the per-file bar. `sessions`
/// tallies each completed upload under its release, so the snapshot keeps finished
/// files in the cumulative progress and never re-derives a completed file as
/// queued while coven's post-upload commit hasn't removed its row yet.
/// `throughput` records the byte deltas as they transfer, for a rolling-window
/// rate. Database-backed projection work is sent to the sync controller rather
/// than retaining or exposing its database.
pub struct ReleaseUploadObserver {
    in_flight: Arc<Mutex<HashMap<String, u64>>>,
    throughput: Arc<crate::library::UploadThroughput>,
    sync_paused: Arc<std::sync::atomic::AtomicBool>,
    events: mpsc::UnboundedSender<UploadObserverMessage>,
}

impl ReleaseUploadObserver {
    pub(crate) fn new(
        in_flight: Arc<Mutex<HashMap<String, u64>>>,
        throughput: Arc<crate::library::UploadThroughput>,
        sync_paused: Arc<std::sync::atomic::AtomicBool>,
    ) -> (Self, UploadObserverEvents) {
        let (events, receiver) = mpsc::unbounded_channel();
        (
            Self {
                in_flight,
                throughput,
                sync_paused,
                events,
            },
            UploadObserverEvents { receiver },
        )
    }

    async fn report(&self, event: UploadObserverEvent) {
        let event_name = event.name();
        let (completed, completion) = oneshot::channel();
        if self
            .events
            .send(UploadObserverMessage { event, completed })
            .is_err()
        {
            warn!("upload observer event processor stopped before {event_name}");
            return;
        }
        if completion.await.is_err() {
            warn!("upload observer event processor dropped {event_name}");
        }
    }
}

#[async_trait::async_trait]
impl coven::BlobTransitionObserver for ReleaseUploadObserver {
    async fn on_blob_upload_started(&self, file_id: &str) {
        {
            self.in_flight
                .lock()
                .unwrap()
                .insert(file_id.to_string(), 0);
        }
        self.report(UploadObserverEvent::OutboxChanged).await;
    }

    async fn on_blob_upload_progress(&self, file_id: &str, bytes_done: u64, _bytes_total: u64) {
        // Feed the throughput tracker only the bytes new since the last report.
        // coven coalesces these calls to a tick, so each is already throttled —
        // emit the snapshot on every one to move the bar. The counts are
        // cumulative and monotonic within an attempt; `saturating_sub` guards a
        // late or duplicate report.
        let delta = {
            let mut map = self.in_flight.lock().unwrap();
            match map.get_mut(file_id) {
                Some(prev) => {
                    let delta = bytes_done.saturating_sub(*prev);
                    *prev = bytes_done;
                    delta
                }
                // A progress report after the file's terminal callback removed its
                // in-flight entry: ignore it, but name the file so a stray report
                // isn't fully invisible.
                None => {
                    debug!(
                        "upload progress for {file_id} after it left the in-flight set; ignoring"
                    );
                    return;
                }
            }
        };
        if delta > 0 {
            self.throughput.record(delta);
        }
        self.report(UploadObserverEvent::OutboxChanged).await;
    }

    async fn on_blob_uploaded(&self, file_id: &str) {
        // Credit any bytes no progress report counted (a small file that uploaded
        // between coalescing ticks, or the tail past the last report) so the
        // throughput tracker sees the whole file. coven's counts are of the
        // encrypted payload, a few bytes larger than `file_size`, but the rolling
        // rate is approximate so the discrepancy doesn't matter. coven, not bae,
        // flips the gate — this is a notification only.
        let already_counted = match self.in_flight.lock().unwrap().remove(file_id) {
            Some(counted) => counted,
            // The file completed without an in-flight entry (no `started`/progress
            // seen — e.g. a tiny file). Credit its whole size below; note it so the
            // missing lifecycle isn't silent.
            None => {
                debug!(
                    "upload of {file_id} completed with no in-flight entry; crediting full size"
                );
                0
            }
        };
        self.report(UploadObserverEvent::BlobUploaded {
            file_id: file_id.to_string(),
            already_counted,
        })
        .await;
    }

    async fn on_blob_upload_failed(&self, file_id: &str, _error: &str) {
        {
            self.in_flight.lock().unwrap().remove(file_id);
        }
        // coven's drain records the attempt count and the error on its own
        // queue entry; the snapshot we emit here reads them back.
        self.report(UploadObserverEvent::OutboxChanged).await;
    }

    fn should_skip_uploads(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// coven finished making a root Remote (every blob uploaded, gate flipped,
    /// source files dropped). For a release, drop its completed-upload tally — its queue rows
    /// are gone, so nothing renders for it. For *every* root, refresh the outbox
    /// snapshot: a covers / artist-images root often commits last in a burst, and
    /// skipping its emission would leave the queue pane frozen on the previous
    /// snapshot instead of clearing.
    async fn on_root_made_remote(&self, root_table: &str, root_id: &str) {
        if root_table == "releases" {
            self.report(UploadObserverEvent::ReleaseMadeRemote {
                release_id: root_id.to_string(),
            })
            .await;
        } else {
            debug!("on_root_made_remote for non-release root {root_table:?}/{root_id}");
            self.report(UploadObserverEvent::OutboxChanged).await;
        }
    }

    /// coven finished making a root Local (blobs materialized to local files,
    /// gate retracted, cloud blobs queued for tombstoning): refresh the outbox
    /// snapshot for every root because the retraction changed the queue.
    async fn on_root_made_local(&self, root_table: &str, root_id: &str) {
        if root_table == "releases" {
            self.report(UploadObserverEvent::ReleaseMadeLocal).await;
        } else {
            debug!("on_root_made_local for non-release root {root_table:?}/{root_id}");
            self.report(UploadObserverEvent::OutboxChanged).await;
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
    async fn on_blob_upload_started(&self, file_id: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_upload_started(file_id).await;
        }
    }

    async fn on_blob_upload_progress(&self, file_id: &str, bytes_done: u64, bytes_total: u64) {
        if let Some(observer) = self.inner.upgrade() {
            observer
                .on_blob_upload_progress(file_id, bytes_done, bytes_total)
                .await;
        }
    }

    async fn on_blob_uploaded(&self, file_id: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_uploaded(file_id).await;
        }
    }

    async fn on_blob_upload_failed(&self, file_id: &str, error: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_blob_upload_failed(file_id, error).await;
        }
    }

    fn should_skip_uploads(&self) -> bool {
        self.inner
            .upgrade()
            .is_some_and(|observer| observer.should_skip_uploads())
    }

    async fn on_root_made_remote(&self, root_table: &str, root_id: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_root_made_remote(root_table, root_id).await;
        }
    }

    async fn on_root_made_local(&self, root_table: &str, root_id: &str) {
        if let Some(observer) = self.inner.upgrade() {
            observer.on_root_made_local(root_table, root_id).await;
        }
    }
}
