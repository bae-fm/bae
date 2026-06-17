//! bae's `BlobPlan` + `BlobUploadObserver`.
//!
//! coven owns the cloud blob layout and encryption; bae decides which of its rows
//! carry blobs and where their local files live. `library_images` INSERTs carry
//! blobs that move with the changeset (deletes go through the cloud outbox, not
//! here). After a release's storage files finish uploading, the observer marks it
//! managed (cloud-only) and drops this device's local copy.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use coven::blob::{BlobPlan, BlobRef, BlobScope, BlobUploadObserver, DrainControl};
use coven::changeset::{ChangeOp, RowChange};
use coven::library_dir::LibraryDir;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::library::LibraryEvent;

/// Maps bae's blob-bearing rows to their cloud blobs. Push and pull move the
/// same set (an INSERT's blob is uploaded on push, downloaded on pull).
pub struct BaeBlobPlan {
    library_dir: LibraryDir,
}

impl BaeBlobPlan {
    pub fn new(library_dir: LibraryDir) -> Self {
        Self { library_dir }
    }

    /// A `library_images` row's blob. Every image — cover or artist — encrypts
    /// with the library master key.
    fn image_ref(&self, id: &str) -> BlobRef {
        BlobRef {
            namespace: "images".to_string(),
            id: id.to_string(),
            local_path: self.library_dir.image_path(id),
            scope: BlobScope::Master,
            // bae homes use the hashed (obfuscated) blob layout, which keys off
            // the id and ignores a readable cloud path.
            cloud_path: None,
        }
    }

    fn refs(&self, changes: &[RowChange]) -> Vec<BlobRef> {
        let mut refs = Vec::new();
        for change in changes {
            // Only INSERTs carry a new blob: image bytes never change on update,
            // and deletes are handled by the cloud outbox.
            if change.op != ChangeOp::Insert {
                continue;
            }
            let Some(id) = change.pk() else {
                warn!(
                    "{} INSERT has no primary key; skipping its blob",
                    change.table
                );
                continue;
            };
            // `library_images` is the only blob-bearing table.
            if change.table == "library_images" {
                refs.push(self.image_ref(id));
            }
        }
        refs
    }
}

impl BlobPlan for BaeBlobPlan {
    fn blobs_to_push(&self, changes: &[RowChange]) -> Vec<BlobRef> {
        self.refs(changes)
    }

    fn blobs_to_pull(&self, changes: &[RowChange]) -> Vec<BlobRef> {
        self.refs(changes)
    }

    /// Every `library_images` row currently in the DB references a blob that
    /// must be local. A device that bootstraps from a snapshot has these rows but
    /// not their files — the snapshot carries no blobs, and the incremental pull
    /// starts past the INSERTs that first carried them — so coven downloads the
    /// missing ones from this list. Scopes each row's blob to the master key
    /// exactly as the changeset path does, so a row's blob is described one way
    /// whether it arrives as a changeset or in a snapshot. Audio is excluded: it
    /// is streamed on demand, never pulled to disk.
    fn blobs_in_db(
        &self,
        conn: &coven::rusqlite::Connection,
    ) -> coven::rusqlite::Result<Vec<BlobRef>> {
        let mut refs = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT id FROM library_images")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                refs.push(self.image_ref(&row?));
            }
        }
        Ok(refs)
    }
}

/// Tracks the outbox upload lifecycle for the UI and clears a release's
/// `unmanaged_path` once all its storage files have uploaded (transitioning it
/// from "importing from an external path" to fully managed).
///
/// `in_flight` maps each currently-uploading `file_id` to the live count of
/// encrypted bytes that have reached the cloud for it, shared with the
/// `LibraryManager` so its outbox snapshot reports the "uploading" state and
/// drives the per-file bar. `throughput` records the byte deltas as they
/// transfer so the snapshot can surface a rolling-window rate. `library_dir`
/// lets the observer rebuild `ReleaseDetail` payloads (via
/// `find_release_detail_with`) so the storage-state transition that lands at
/// the end of an Unmanaged → CloudOnly run emits a `ReleaseUpdated` event.
/// Every lifecycle callback re-emits the outbox snapshot as a
/// `LibraryEvent::OutboxChanged`.
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

    /// Emit a `ReleaseUpdated` event after a storage-state transition.
    /// The observer fires inside an active sync cycle, so `has_cloud_home`
    /// is always true at this point.
    async fn emit_release_updated(&self, album_id: &str, release_id: &str) {
        match crate::library::manager::find_release_detail_with(
            &self.db,
            &self.library_dir,
            true,
            release_id,
        )
        .await
        {
            Ok(Some(release)) => {
                let _ = self.events.send(LibraryEvent::ReleaseUpdated {
                    album_id: album_id.to_string(),
                    release,
                });
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

    /// Flip a release to managed once its last storage file uploads — the moment
    /// the cloud holds a durable copy of every file and the release's metadata is
    /// safe to push to your other devices. A pinned release keeps this device's verified
    /// local copy; a cloud-only one drops it. Every managed-content path (managed
    /// import and Manage → Pinned/CloudOnly) imports the release `managed = false`
    /// and reaches `managed = true` here, so the synced row never references a
    /// blob the cloud doesn't hold yet.
    ///
    /// Returns whether this call flipped the release, so the caller can break the
    /// outbox drain and publish the now-synced rows.
    async fn mark_managed_if_complete(&self, file_id: &str) -> bool {
        let release = match self.db.find_release_for_file(file_id).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                debug!("No release owns uploaded file {file_id}; nothing to mark");
                return false;
            }
            Err(e) => {
                warn!("Failed to look up release for uploaded file {file_id}: {e}");
                return false;
            }
        };
        if release.managed {
            // Its uploads already completed and flipped it; nothing to do.
            debug!("Release {} is already managed; nothing to mark", release.id);
            return false;
        }
        let local_copy = match self.db.get_release_local_copy(&release.id).await {
            Ok(copy) => copy,
            Err(e) => {
                warn!("Failed to load local copy for {}: {e}", release.id);
                return false;
            }
        };
        match self.db.has_pending_uploads_for_release(&release.id).await {
            Ok(true) => false, // More files still to upload.
            Ok(false) => {
                // Last upload landed: the cloud now holds a durable copy of every
                // file, so the release can flip to managed. A pin keeps this
                // device's verified `storage/` copy (flag-only flip); a cloud-only
                // release drops it — and first deletes the originals if a
                // Manage → CloudOnly asked to (they were the upload source, unsafe
                // to delete until now).
                let pinned = local_copy.as_ref().is_some_and(|c| c.pinned_locally);
                let kind = if pinned { "pinned" } else { "cloud-only" };
                info!(
                    "All files uploaded for release {}, marking managed ({kind})",
                    release.id
                );
                let flip = if pinned {
                    self.db.set_release_managed_pinned(&release.id).await
                } else {
                    self.delete_unmanaged_source_if_requested(&release, local_copy.as_ref())
                        .await;
                    self.db.set_release_managed_cloud_only(&release.id).await
                };
                if let Err(e) = flip {
                    warn!("Failed to mark {} managed ({kind}): {e}", release.id);
                    return false;
                }
                // The release just became managed. Emit ReleaseUpdated so the UI's
                // cached summary picks up the new storage_state (per-release upload
                // counts come from the snapshot, but the state field doesn't).
                self.emit_release_updated(&release.album_id, &release.id)
                    .await;
                true
            }
            Err(e) => {
                warn!("Failed to check pending uploads for {}: {e}", release.id);
                false
            }
        }
    }
}

impl ReleaseUploadObserver {
    /// If a Manage → CloudOnly transition asked to delete the originals,
    /// remove the source files at `unmanaged_path/{original_filename}` now that
    /// the cloud holds a durable copy, then clear the intent flag. Called only
    /// after the release's last upload completes and only while this device's
    /// local copy still records the unmanaged source path.
    async fn delete_unmanaged_source_if_requested(
        &self,
        release: &crate::db::DbRelease,
        local_copy: Option<&crate::db::DbReleaseLocalCopy>,
    ) {
        let Some(unmanaged_path) = local_copy.and_then(|c| c.unmanaged_path.as_deref()) else {
            return;
        };
        match self
            .db
            .get_release_delete_unmanaged_source_on_upload(&release.id)
            .await
        {
            Ok(false) => return,
            Ok(true) => {}
            Err(e) => {
                warn!(
                    "Failed to read delete-source intent for {}: {e}",
                    release.id
                );
                return;
            }
        }

        let files = match self.db.get_files_for_release(&release.id).await {
            Ok(files) => files,
            Err(e) => {
                warn!(
                    "Failed to load files to delete originals for {}: {e}",
                    release.id
                );
                return;
            }
        };

        for file in &files {
            let path = std::path::Path::new(unmanaged_path).join(&file.original_filename);
            match tokio::fs::remove_file(&path).await {
                Ok(()) => info!("Deleted managed-source original: {}", path.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    debug!("Source original already gone: {}", path.display());
                }
                Err(e) => warn!("Failed to delete source original {}: {e}", path.display()),
            }
        }

        if let Err(e) = self
            .db
            .set_release_delete_unmanaged_source_on_upload(&release.id, false)
            .await
        {
            warn!(
                "Failed to clear delete-source intent for {}: {e}",
                release.id
            );
        }
    }
}

#[async_trait::async_trait]
impl BlobUploadObserver for ReleaseUploadObserver {
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
            // Only an in-flight entry advances; a report after the terminal
            // callback (entry already removed) is ignored.
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

    async fn on_blob_uploaded(&self, file_id: &str) -> DrainControl {
        // Credit any bytes not yet counted by a progress report (e.g. a small
        // file that uploaded between coalescing ticks, or the tail past the
        // last report) so the rolling throughput tracker sees the whole file.
        // The byte counts coven reports are of the encrypted payload, a few
        // bytes larger than `file_size`; the rolling rate is approximate, so the
        // small discrepancy is immaterial.
        let already_counted = self.in_flight.lock().unwrap().remove(file_id).unwrap_or(0);
        if let Ok(Some(file)) = self.db.find_file_by_id(file_id).await {
            let remaining = (file.file_size as u64).saturating_sub(already_counted);
            if remaining > 0 {
                self.throughput.record(remaining);
            }
        }
        let flipped = self.mark_managed_if_complete(file_id).await;
        self.emit_outbox_changed().await;
        // When this upload completed a release (its last blob landed and it
        // flipped to managed), break the drain so the cycle publishes that
        // release's now-synced rows before draining the rest of the queue.
        if flipped {
            DrainControl::Publish
        } else {
            DrainControl::Continue
        }
    }

    async fn on_blob_upload_failed(&self, file_id: &str, _error: &str) {
        {
            self.in_flight.lock().unwrap().remove(file_id);
        }
        // The failure (attempt_count / last_error) is persisted by coven's
        // process_uploads via record_cloud_upload_failure; the snapshot we emit
        // here reflects it.
        self.emit_outbox_changed().await;
    }

    fn should_skip_uploads(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coven::changeset::ChangeOp;

    fn row(table: &str, op: ChangeOp, cols: &[&str]) -> RowChange {
        RowChange {
            table: table.to_string(),
            op,
            columns: cols.iter().map(|c| Some(c.to_string())).collect(),
        }
    }

    fn plan() -> BaeBlobPlan {
        BaeBlobPlan::new(LibraryDir::new("/lib"))
    }

    #[test]
    fn every_image_uses_master_scope() {
        // Covers and artist images both encrypt with the library master key.
        for image_type in ["cover", "artist"] {
            let refs = plan().blobs_to_push(&[row(
                "library_images",
                ChangeOp::Insert,
                &["img-1", image_type],
            )]);
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].namespace, "images");
            assert_eq!(refs[0].id, "img-1");
            assert_eq!(refs[0].scope, BlobScope::Master);
        }
    }

    #[test]
    fn blobs_in_db_lists_every_image_row_with_master_scope() {
        // The snapshot-bootstrap enumeration must produce the same (id → scope)
        // mapping as the changeset path: every image → master. (Audio lives in
        // other tables and is never listed — it streams on demand.)
        let conn = coven::rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE library_images (id TEXT PRIMARY KEY, type TEXT NOT NULL);\n\
             INSERT INTO library_images (id, type) VALUES ('rel-1', 'cover');\n\
             INSERT INTO library_images (id, type) VALUES ('img-2', 'artist');",
        )
        .unwrap();

        let refs = plan().blobs_in_db(&conn).unwrap();
        assert_eq!(refs.len(), 2);

        let cover = refs.iter().find(|r| r.id == "rel-1").unwrap();
        assert_eq!(cover.namespace, "images");
        assert_eq!(cover.scope, BlobScope::Master);

        let artist = refs.iter().find(|r| r.id == "img-2").unwrap();
        assert_eq!(artist.namespace, "images");
        assert_eq!(artist.scope, BlobScope::Master);
    }

    #[test]
    fn updates_deletes_and_other_tables_carry_no_blobs() {
        // Image bytes never change on update; deletes go through the cloud
        // outbox; non-blob tables (albums, etc.) carry nothing.
        let changes = [
            row("library_images", ChangeOp::Update, &["img-1", "cover"]),
            row("library_images", ChangeOp::Delete, &["img-1", "cover"]),
            row("albums", ChangeOp::Insert, &["a-1"]),
        ];
        assert!(plan().blobs_to_push(&changes).is_empty());
        assert!(plan().blobs_to_pull(&changes).is_empty());
    }
}
