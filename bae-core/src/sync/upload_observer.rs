//! bae's `BlobTransitionObserver` — UI bookkeeping only.
//!
//! coven owns the whole blob lifecycle: the upload drain, the make-Remote gate
//! flip + source-file delete, and the make-Local materialize + retract. This
//! observer only *reports* what coven did, so the UI stays current:
//!
//! - the upload callbacks drive the `in_flight` map and the rolling-window
//!   `throughput`, and re-emit the outbox snapshot as a `LibraryEvent`;
//! - `on_root_made_remote` / `on_root_made_local` fire when coven completes a
//!   transition (including one resumed after a restart), so bae's
//!   `ReleaseUpdated` event survives a restart rather than being lost with an
//!   in-memory flag.
//!
//! `should_skip_uploads` lets the host pause the upload pipeline without touching
//! the queue.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use coven::library_dir::LibraryDir;
use tokio::sync::broadcast;
use tracing::warn;

use crate::db::Database;
use crate::library::LibraryEvent;

/// Reports coven's blob transitions to the UI: the outbox/throughput state while
/// a make-Remote uploads, and a `ReleaseUpdated` whenever coven completes a
/// transition.
///
/// `in_flight` maps each currently-uploading `file_id` to the live count of
/// encrypted bytes that have reached the cloud for it, shared with the
/// `LibraryManager` so its outbox snapshot reports the "uploading" state and
/// drives the per-file bar. `throughput` records the byte deltas as they transfer
/// so the snapshot can surface a rolling-window rate. `library_dir` lets the
/// observer rebuild `ReleaseDetail` payloads (via `find_release_detail_with`) so
/// a completed transition emits a `ReleaseUpdated` event.
pub struct ReleaseUploadObserver {
    db: Arc<Database>,
    library_dir: LibraryDir,
    in_flight: Arc<Mutex<HashMap<String, u64>>>,
    throughput: Arc<crate::library::UploadThroughput>,
    sync_paused: Arc<std::sync::atomic::AtomicBool>,
    events: broadcast::Sender<LibraryEvent>,
}

impl ReleaseUploadObserver {
    pub fn new(
        db: Arc<Database>,
        library_dir: LibraryDir,
        in_flight: Arc<Mutex<HashMap<String, u64>>>,
        throughput: Arc<crate::library::UploadThroughput>,
        sync_paused: Arc<std::sync::atomic::AtomicBool>,
        events: broadcast::Sender<LibraryEvent>,
    ) -> Self {
        Self {
            db,
            library_dir,
            in_flight,
            throughput,
            sync_paused,
            events,
        }
    }

    /// Emit a `ReleaseUpdated` event after coven completes a transition, so the
    /// UI's cached summary picks up the new `storage_state`.
    async fn emit_release_updated(&self, release_id: &str) {
        let album_id = match self.db.find_release_by_id(release_id).await {
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
            &self.db,
            &self.library_dir,
            true,
            release_id,
        )
        .await
        {
            Ok(Some(release)) => {
                let _ = self
                    .events
                    .send(LibraryEvent::ReleaseUpdated { album_id, release });
            }
            Ok(None) => warn!("emit_release_updated: release {release_id} not found"),
            Err(e) => warn!("emit_release_updated: {e}"),
        }
    }

    /// Rebuild the outbox snapshot and broadcast it. A send error just means no
    /// UI is subscribed right now, which is fine.
    async fn emit_outbox_changed(&self) {
        let in_flight = { self.in_flight.lock().unwrap().clone() };
        let paused = self.sync_paused.load(std::sync::atomic::Ordering::SeqCst);
        match crate::library::outbox_snapshot::build_outbox_snapshot(
            &self.db,
            &in_flight,
            &self.throughput,
            paused,
        )
        .await
        {
            Ok(snapshot) => {
                let _ = self.events.send(LibraryEvent::OutboxChanged { snapshot });
            }
            Err(e) => warn!("Failed to build outbox snapshot: {e}"),
        }
    }
}

#[async_trait::async_trait]
impl coven::blob::BlobTransitionObserver for ReleaseUploadObserver {
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
        // Advance the file's live byte count and feed only the new bytes since
        // the last report to the rolling-window throughput tracker. coven
        // coalesces these calls to a tick, so each is already throttled — emit
        // the snapshot on every one to move the bar. The byte counts are
        // cumulative and monotonic within one attempt, so the delta is
        // non-negative; `saturating_sub` guards against a late/duplicate report.
        let delta = {
            let mut map = self.in_flight.lock().unwrap();
            match map.get_mut(file_id) {
                Some(prev) => {
                    let delta = bytes_done.saturating_sub(*prev);
                    *prev = bytes_done;
                    delta
                }
                None => return,
            }
        };
        if delta > 0 {
            self.throughput.record(delta);
        }
        self.emit_outbox_changed().await;
    }

    async fn on_blob_uploaded(&self, file_id: &str) {
        // Credit any bytes not yet counted by a progress report (e.g. a small
        // file that uploaded between coalescing ticks, or the tail past the last
        // report) so the rolling throughput tracker sees the whole file. The byte
        // counts coven reports are of the encrypted payload, a few bytes larger
        // than `file_size`; the rolling rate is approximate, so the small
        // discrepancy is immaterial. coven, not bae, flips the gate — this is a
        // notification only.
        let already_counted = self.in_flight.lock().unwrap().remove(file_id).unwrap_or(0);
        if let Ok(Some(file)) = self.db.find_file_by_id(file_id).await {
            let remaining = (file.file_size as u64).saturating_sub(already_counted);
            if remaining > 0 {
                self.throughput.record(remaining);
            }
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

    /// coven finished making a release Remote (every blob uploaded, gate flipped,
    /// source files dropped): emit `ReleaseUpdated` so the UI's storage state
    /// flips, and refresh the outbox snapshot now that the queue is empty.
    async fn on_root_made_remote(&self, root_table: &str, root_id: &str) {
        if root_table != "releases" {
            return;
        }
        self.emit_release_updated(root_id).await;
        self.emit_outbox_changed().await;
    }

    /// coven finished making a release Local (blobs materialized to local files,
    /// gate retracted, cloud blobs queued for tombstoning): emit `ReleaseUpdated`.
    async fn on_root_made_local(&self, root_table: &str, root_id: &str) {
        if root_table != "releases" {
            return;
        }
        self.emit_release_updated(root_id).await;
        self.emit_outbox_changed().await;
    }
}
