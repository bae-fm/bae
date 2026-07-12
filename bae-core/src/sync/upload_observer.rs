//! bae's `BlobTransitionObserver` — UI bookkeeping only.
//!
//! coven owns the whole blob lifecycle: the upload drain, the make-Remote gate
//! flip + source-file delete, and the make-Local materialize + retract. This
//! observer only *reports* what coven did, so the UI stays current:
//!
//! - the upload callbacks drive the `in_flight` map and the rolling-window
//!   `throughput`, and re-emit the outbox snapshot as a `LibraryEvent`;
//! - `on_root_made_remote` / `on_root_made_local` fire when coven completes a
//!   transition, including one resumed after a restart — so the `ReleaseUpdated`
//!   event survives a restart rather than dying with an in-memory flag.
//!
//! `should_skip_uploads` lets the host pause the upload pipeline without touching
//! the queue.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use coven::{BlobTransitionObserver, CovenHandle};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::db::Database;
use crate::library::LibraryEvent;

/// Reports coven's blob transitions to the UI: the outbox/throughput state while
/// a make-Remote uploads, and a `ReleaseUpdated` whenever coven completes a
/// transition.
///
/// `in_flight` maps each uploading `file_id` to the encrypted bytes that have
/// reached the cloud for it, shared with the `LibraryManager` so its outbox
/// snapshot reports the "uploading" state and drives the per-file bar. `sessions`
/// tallies each completed upload under its release, so the snapshot keeps finished
/// files in the cumulative progress and never re-derives a completed file as
/// queued while coven's post-upload commit hasn't removed its row yet.
/// `throughput` records the byte deltas as they transfer, for a rolling-window
/// rate. `handle` is what `find_release_detail_with` answers the pin-state query
/// through when rebuilding a `ReleaseDetail` for a `ReleaseUpdated` event; it is
/// filled by [`set_handle`](Self::set_handle) right after the handle is built,
/// since the handle owns this observer and so can't be passed at construction.
pub struct ReleaseUploadObserver {
    db: OnceLock<Arc<Database>>,
    handle: OnceLock<CovenHandle>,
    in_flight: Arc<Mutex<HashMap<String, u64>>>,
    sessions: Arc<crate::library::UploadSessions>,
    throughput: Arc<crate::library::UploadThroughput>,
    sync_paused: Arc<std::sync::atomic::AtomicBool>,
    events: broadcast::Sender<LibraryEvent>,
}

impl ReleaseUploadObserver {
    pub fn new(
        in_flight: Arc<Mutex<HashMap<String, u64>>>,
        sessions: Arc<crate::library::UploadSessions>,
        throughput: Arc<crate::library::UploadThroughput>,
        sync_paused: Arc<std::sync::atomic::AtomicBool>,
        events: broadcast::Sender<LibraryEvent>,
    ) -> Self {
        Self {
            db: OnceLock::new(),
            handle: OnceLock::new(),
            in_flight,
            sessions,
            throughput,
            sync_paused,
            events,
        }
    }

    /// Install the bae database wrapper after coven has opened the handle it owns.
    pub fn set_database(&self, db: Arc<Database>) {
        if self.db.set(db).is_err() {
            panic!("ReleaseUploadObserver database already set — the observer was wired twice");
        }
    }

    /// Install the handle the observer answers pin-state through. Called once,
    /// right after the `CovenHandle` (which owns this observer) is built, so the
    /// observer can rebuild `ReleaseDetail` payloads when a transition completes.
    pub fn set_handle(&self, handle: CovenHandle) {
        if self.handle.set(handle).is_err() {
            panic!("ReleaseUploadObserver handle already set — the observer was wired twice");
        }
    }

    /// The installed handle. Set right after construction and before any
    /// transition can fire, so its absence is a wiring bug, not a runtime state.
    fn handle(&self) -> &CovenHandle {
        self.handle
            .get()
            .expect("observer handle set before any blob transition fires")
    }

    /// The installed bae database wrapper. Set before sync can start.
    fn db(&self) -> &Database {
        self.db
            .get()
            .expect("observer database set before any blob transition fires")
            .as_ref()
    }

    /// Emit a `ReleaseUpdated` event after coven completes a transition, so the
    /// UI's cached summary picks up the new `storage_state`.
    async fn emit_release_updated(&self, release_id: &str) {
        let album_id = match self.db().find_release_by_id(release_id).await {
            Ok(Some(release)) => release.album_id,
            Ok(None) => {
                warn!("emit_release_updated: release {release_id} not found");
                return;
            }
            Err(e) => {
                warn!("emit_release_updated: {e}");
                return;
            }
        };
        match crate::library::manager::find_release_detail_with(
            self.db(),
            self.handle(),
            true,
            release_id,
        )
        .await
        {
            Ok(Some(release)) => {
                // A `send` error means no UI is subscribed right now — harmless.
                let _ = self
                    .events
                    .send(LibraryEvent::ReleaseUpdated { album_id, release });
            }
            Ok(None) => warn!("emit_release_updated: release {release_id} not found"),
            Err(e) => warn!("emit_release_updated: {e}"),
        }
    }

    /// Rebuild the outbox snapshot and broadcast it.
    async fn emit_outbox_changed(&self) {
        let in_flight = { self.in_flight.lock().unwrap().clone() };
        let paused = self.sync_paused.load(std::sync::atomic::Ordering::SeqCst);
        match crate::library::outbox_snapshot::build_outbox_snapshot(
            self.db(),
            &in_flight,
            &self.sessions,
            &self.throughput,
            paused,
        )
        .await
        {
            Ok(snapshot) => {
                // A `send` error means no UI is subscribed right now — harmless.
                let _ = self.events.send(LibraryEvent::OutboxChanged { snapshot });
            }
            Err(e) => warn!("Failed to build outbox snapshot: {e}"),
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
        self.emit_outbox_changed().await;
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
        self.emit_outbox_changed().await;
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
        // Tally the completion under its release, so the snapshot derives this
        // file as done (its outbox row lingers until coven's post-upload commit
        // removes it) and keeps its bytes in the cumulative progress.
        match self.db().find_file_by_id(file_id).await {
            Ok(Some(file)) => {
                let remaining = (file.file_size as u64).saturating_sub(already_counted);
                if remaining > 0 {
                    self.throughput.record(remaining);
                }
                self.sessions.record_done(
                    Some(file.release_id),
                    crate::library::upload_sessions::DoneFile {
                        file_id: file_id.to_string(),
                        display_name: file.original_filename,
                        bytes: file.file_size as u64,
                    },
                );
            }
            // No release-file row backs this blob: tally it in the
            // unattributed bucket, labelled by its id, with the encrypted byte
            // count the transfer reported.
            Ok(None) => {
                warn!("on_blob_uploaded: no file row for {file_id}; tallying unattributed");
                self.sessions.record_done(
                    None,
                    crate::library::upload_sessions::DoneFile {
                        file_id: file_id.to_string(),
                        display_name: file_id.to_string(),
                        bytes: already_counted,
                    },
                );
            }
            // DB error: the tally and throughput credit are UI bookkeeping, so
            // a miss only degrades the display — log it rather than swallow it.
            Err(e) => warn!("on_blob_uploaded: looking up {file_id}: {e}"),
        }
        self.emit_outbox_changed().await;
    }

    async fn on_blob_upload_failed(&self, file_id: &str, _error: &str) {
        {
            self.in_flight.lock().unwrap().remove(file_id);
        }
        // The failure (attempt_count / last_error) is persisted by coven's
        // drain_uploads via record_cloud_upload_failure; the snapshot we emit
        // here reflects it.
        self.emit_outbox_changed().await;
    }

    fn should_skip_uploads(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// coven finished making a root Remote (every blob uploaded, gate flipped,
    /// source files dropped). For a release, emit `ReleaseUpdated` so the UI's
    /// storage state flips, and drop its completed-upload tally — its queue rows
    /// are gone, so nothing renders for it. For *every* root, refresh the outbox
    /// snapshot: a covers / artist-images root often commits last in a burst, and
    /// skipping its emission would leave the queue pane frozen on the previous
    /// snapshot instead of clearing.
    async fn on_root_made_remote(&self, root_table: &str, root_id: &str) {
        if root_table == "releases" {
            if let Err(e) = self.db().complete_import_for_release(root_id).await {
                warn!("on_root_made_remote: marking import complete for {root_id}: {e}");
            }
            self.emit_release_updated(root_id).await;
            self.sessions.clear_group(Some(root_id));
        } else {
            debug!("on_root_made_remote for non-release root {root_table:?}/{root_id}");
        }
        self.emit_outbox_changed().await;
    }

    /// coven finished making a root Local (blobs materialized to local files,
    /// gate retracted, cloud blobs queued for tombstoning): emit
    /// `ReleaseUpdated` for a release, and refresh the outbox snapshot for
    /// every root — the retraction changed the queue either way.
    async fn on_root_made_local(&self, root_table: &str, root_id: &str) {
        if root_table == "releases" {
            self.emit_release_updated(root_id).await;
        } else {
            debug!("on_root_made_local for non-release root {root_table:?}/{root_id}");
        }
        self.emit_outbox_changed().await;
    }
}

/// The `BlobTransitionObserver` coven actually holds, keeping only a [`Weak`]
/// reference to the real [`ReleaseUploadObserver`].
///
/// coven's builder takes the observer as a strong `Arc`, and the observer holds
/// the `CovenHandle` back (to answer pin-state when rebuilding a `ReleaseDetail`).
/// Registering the observer directly would close a strong cycle
/// coven → observer → handle → coven that outlives every `LibraryManager`, so
/// coven's exclusive store-open lock would never release and an in-process
/// reopen of the same store fails with "store is already open". Registering this
/// weak adapter instead leaves the observer owned solely by the `LibraryManager`
/// (see `LibraryManager::open`): when the last manager clone drops, the observer
/// drops, its handle clone drops, and the lock is released.
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
