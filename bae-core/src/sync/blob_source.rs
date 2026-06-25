//! bae's `BlobSource` + `BlobUploadObserver`.
//!
//! coven owns the cloud blob layout and encryption; bae decides which of its rows
//! carry blobs and where their local files live. `library_images` INSERTs carry
//! blobs that move with the changeset (deletes go through the cloud outbox, not
//! here). After a release's storage files finish uploading, the observer marks it
//! managed (cloud-only) and drops this device's local copy.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use coven::blob::{BlobRef, BlobScope, BlobSource, BlobSync, BlobUploadObserver, DrainControl};
use coven::changeset::{ChangeOp, RowChange};
use coven::library_dir::LibraryDir;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::library::LibraryEvent;

/// The `library_images` columns in DDL (schema) order — the single source of
/// truth this module reads `cloud_path` at, kept in lockstep with
/// `bae-core/migrations/001_initial.sql`. A coven changeset's `RowChange.columns`
/// holds values in this order, so the index of `cloud_path` here IS the index to
/// read it at from a changeset. A guard test inserts a real row, captures the
/// changeset, and asserts the value at `LIBRARY_IMAGES_CLOUD_PATH_INDEX` equals
/// the stored `cloud_path`, so a DDL reorder fails loudly.
const LIBRARY_IMAGES_COLUMNS: &[&str] = &[
    "id",
    "type",
    "content_type",
    "file_size",
    "width",
    "height",
    "source",
    "source_url",
    "cloud_path",
    "_updated_at",
    "created_at",
];

/// The changeset column index `cloud_path` lives at for `library_images`,
/// derived from [`LIBRARY_IMAGES_COLUMNS`] so it can never drift from that list.
const LIBRARY_IMAGES_CLOUD_PATH_INDEX: usize = {
    let mut i = 0;
    while i < LIBRARY_IMAGES_COLUMNS.len() {
        // `str::eq` isn't const, so compare bytes.
        if matches!(LIBRARY_IMAGES_COLUMNS[i].as_bytes(), b"cloud_path") {
            break;
        }
        i += 1;
    }
    i
};

/// Maps bae's blob-bearing rows to their cloud blobs. coven walks both a pushed
/// and an incoming changeset through the same `blobs_for_change`, so an INSERT's
/// blob is uploaded on push and downloaded on pull off one mapping.
pub struct BaeBlobSource {
    library_dir: LibraryDir,
}

impl BaeBlobSource {
    pub fn new(library_dir: LibraryDir) -> Self {
        Self { library_dir }
    }

    /// A `library_images` row's blob. Every image — cover or artist — encrypts
    /// with the library master key. `cloud_path` is the row's stored value:
    /// `None` on an opaque home (coven's `Hashed` scheme keys off the id), the
    /// readable key on a browsable one (coven's `Plain` scheme uses it verbatim).
    /// The LOCAL file always lives at the hashed `image_path(id)` regardless —
    /// only the cloud key becomes readable. Images are `Mirrored`: a pulling
    /// device downloads and keeps them, since cover/artist art is part of having
    /// the library (audio, by contrast, streams on demand and is never listed).
    fn image_ref(&self, id: &str, cloud_path: Option<String>) -> BlobRef {
        BlobRef {
            namespace: "images".to_string(),
            id: id.to_string(),
            local_path: crate::storage::local::image_path(&self.library_dir, id),
            scope: BlobScope::Master,
            cloud_path,
            sync: BlobSync::Mirrored,
        }
    }

    /// The blobs a single row-change references: at most one image blob for a
    /// `library_images` INSERT, empty for everything else. coven calls this over
    /// every change in both directions.
    fn refs_for_change(&self, change: &RowChange) -> Vec<BlobRef> {
        // Only INSERTs carry a new blob: image bytes never change on update, and
        // deletes are handled by the cloud outbox.
        if change.op != ChangeOp::Insert {
            return Vec::new();
        }
        // `library_images` is the only blob-bearing table.
        if change.table != "library_images" {
            return Vec::new();
        }
        let Some(id) = change.pk() else {
            warn!(
                "{} INSERT has no primary key; skipping its blob",
                change.table
            );
            return Vec::new();
        };
        // The row's stored `cloud_path` (readable key on a browsable home, absent
        // on an opaque one), read at its DDL column index.
        let cloud_path = change
            .col(LIBRARY_IMAGES_CLOUD_PATH_INDEX)
            .map(|s| s.to_string());
        vec![self.image_ref(id, cloud_path)]
    }
}

impl BlobSource for BaeBlobSource {
    fn blobs_for_change(&self, change: &RowChange) -> Vec<BlobRef> {
        self.refs_for_change(change)
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
            let mut stmt = conn.prepare("SELECT id, cloud_path FROM library_images")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            for row in rows {
                let (id, cloud_path) = row?;
                refs.push(self.image_ref(&id, cloud_path));
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
/// the end of an Unmanaged → Managed run emits a `ReleaseUpdated` event.
/// Every lifecycle callback re-emits the outbox snapshot as a
/// `LibraryEvent::OutboxChanged`.
pub struct ReleaseUploadObserver {
    db: Arc<Database>,
    library_dir: LibraryDir,
    in_flight: Arc<Mutex<HashMap<String, u64>>>,
    throughput: Arc<crate::library::UploadThroughput>,
    sync_paused: Arc<std::sync::atomic::AtomicBool>,
    /// Releases the user stopped uploading mid-`manage`. A file that lands for
    /// one of these must not flip it to managed; its blob is queued for delete
    /// instead so the release stays local-only.
    upload_cancelling: Arc<Mutex<HashSet<String>>>,
    events: broadcast::Sender<LibraryEvent>,
}

impl ReleaseUploadObserver {
    pub fn new(
        db: Arc<Database>,
        library_dir: LibraryDir,
        in_flight: Arc<Mutex<HashMap<String, u64>>>,
        throughput: Arc<crate::library::UploadThroughput>,
        sync_paused: Arc<std::sync::atomic::AtomicBool>,
        upload_cancelling: Arc<Mutex<HashSet<String>>>,
        events: broadcast::Sender<LibraryEvent>,
    ) -> Self {
        Self {
            db,
            library_dir,
            in_flight,
            throughput,
            sync_paused,
            upload_cancelling,
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
    /// safe to push to your other devices. The release's bytes now live in coven's
    /// cache (pinned or evictable, per the upload's retain-pinned intent), so this
    /// drops the in-place `release_unmanaged_source` row and deletes the now-
    /// redundant source files. Every managed path (managed import and the "Manage"
    /// action) lands the release `managed = false` and reaches `managed = true`
    /// here, so the synced row never references a blob the cloud doesn't hold yet.
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
        if self.upload_cancelling.lock().unwrap().contains(&release.id) {
            // The user stopped this release's upload, but this file's write
            // landed in the gap. Don't flip to managed — queue the now-orphaned
            // blob for deletion so the release stays local-only.
            debug!(
                "Release {} upload was cancelled; deleting orphaned blob for {file_id}",
                release.id
            );
            match self.db.find_file_by_id(file_id).await {
                Ok(Some(file)) => {
                    if let Err(e) = self.db.add_cloud_outbox_delete(&file.cloud_key()).await {
                        warn!("Failed to queue delete for cancelled upload {file_id}: {e}");
                    }
                }
                Ok(None) => {
                    warn!("Cancelled upload {file_id} has no file row; its blob can't be cleaned")
                }
                Err(e) => {
                    warn!("Failed to look up cancelled upload {file_id} to clean its blob: {e}")
                }
            }
            return false;
        }
        match self.db.has_pending_uploads_for_release(&release.id).await {
            Ok(true) => false, // More files still to upload.
            Ok(false) => {
                // Last upload landed: the cloud now holds a durable copy of every
                // file, so the release flips to managed and its in-place source is
                // redundant. Capture the source path BEFORE dropping its row.
                info!(
                    "All files uploaded for release {}, marking managed",
                    release.id
                );
                let source = self
                    .db
                    .get_release_unmanaged_source(&release.id)
                    .await
                    .unwrap_or_else(|e| {
                        warn!("Failed to load unmanaged source for {}: {e}", release.id);
                        None
                    });
                // Flip the gate on + drop the unmanaged-source row, atomically. The
                // cloud holds every blob, so `managed = true` never points at an
                // absent blob.
                if let Err(e) = self.db.set_release_managed(&release.id).await {
                    warn!("Failed to mark {} managed: {e}", release.id);
                    return false;
                }
                // Delete the now-redundant in-place source files. Best-effort: the
                // release is already managed with its bytes in the cloud + coven's
                // cache, so a leftover original is harmless garbage, not wrong state.
                if let Some(source) = source {
                    self.delete_managed_source_files(&release.id, &source.path)
                        .await;
                }
                // Emit ReleaseUpdated so the UI's cached summary picks up the new
                // storage_state (per-release upload counts come from the snapshot,
                // but the state field doesn't).
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
    /// Delete a just-managed release's in-place source files at
    /// `path/{original_filename}` — they are redundant now that the bytes are in
    /// the cloud + coven's cache. Best-effort: the release is already managed, so a
    /// file that can't be deleted is a harmless leftover, never wrong state.
    async fn delete_managed_source_files(&self, release_id: &str, path: &str) {
        let files = match self.db.get_files_for_release(release_id).await {
            Ok(files) => files,
            Err(e) => {
                warn!("Failed to load files to delete originals for {release_id}: {e}");
                return;
            }
        };

        for file in &files {
            let file_path = std::path::Path::new(path).join(&file.original_filename);
            match tokio::fs::remove_file(&file_path).await {
                Ok(()) => info!("Deleted managed-source original: {}", file_path.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    debug!("Source original already gone: {}", file_path.display());
                }
                Err(e) => warn!(
                    "Failed to delete source original {}: {e}",
                    file_path.display()
                ),
            }
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
        // drain_uploads via record_cloud_upload_failure; the snapshot we emit
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

    /// Seed a release with one file (carrying `cloud_path`) and return a
    /// `(db, observer)` whose `upload_cancelling` already holds the release, so
    /// `mark_managed_if_complete` exercises the cancelled-mid-upload branch.
    #[allow(clippy::type_complexity)]
    async fn observer_with_cancelled_release() -> (
        Arc<Database>,
        ReleaseUploadObserver,
        String,
        String,
        tempfile::TempDir,
    ) {
        use crate::db::{DbAlbum, DbArtist, DbFile, DbRelease, Pressing, ReleaseMetadataSource};
        use crate::util::content_type::ContentType;
        use chrono::Utc;

        let tmp = tempfile::TempDir::new().unwrap();
        let db = Arc::new(
            Database::new_test(
                tmp.path().join("test.db").to_str().unwrap(),
                Arc::new(crate::clock::SystemClock),
            )
            .await
            .unwrap(),
        );
        db.insert_artist(&DbArtist {
            id: "artist-1".into(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
        db.insert_album(&DbAlbum {
            id: "album-1".into(),
            title: "Album Title".into(),
            artist_id: "artist-1".into(),
            year: None,
            primary_release_id: None,
            is_compilation: false,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
        db.insert_release(&DbRelease {
            id: "rel-1".into(),
            album_id: "album-1".into(),
            release_name: None,
            pressing: Pressing {
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            managed: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
        db.insert_file(&DbFile {
            id: "file-1".into(),
            release_id: "rel-1".into(),
            original_filename: "track.flac".into(),
            file_size: 1000,
            content_type: ContentType::Flac,
            // Namespace-relative, as stored on the row; coven prepends `storage/`,
            // so the queued delete key is `storage/rel-1/track`.
            cloud_path: Some("rel-1/track".into()),
            created_at: Utc::now(),
        })
        .await
        .unwrap();

        let cancelling: Arc<Mutex<HashSet<String>>> =
            Arc::new(Mutex::new(HashSet::from(["rel-1".to_string()])));
        let (events, _rx) = broadcast::channel(16);
        let observer = ReleaseUploadObserver::new(
            db.clone(),
            LibraryDir::new(tmp.path().to_path_buf()),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(crate::library::UploadThroughput::new()),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancelling,
            events,
        );
        (
            db,
            observer,
            "file-1".to_string(),
            "storage/rel-1/track".to_string(),
            tmp,
        )
    }

    #[tokio::test]
    async fn cancelled_release_skips_managed_flip_and_queues_blob_delete() {
        let (db, observer, file_id, cloud_key, _tmp) = observer_with_cancelled_release().await;

        // The file's blob landed after the user stopped the upload.
        let flipped = observer.mark_managed_if_complete(&file_id).await;

        assert!(!flipped, "a cancelled release must not flip to managed");
        let release = db.find_release_for_file(&file_id).await.unwrap().unwrap();
        assert!(!release.managed, "release stays local-only");
        let deletes: Vec<String> = db
            .get_pending_cloud_deletes()
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.cloud_key)
            .collect();
        assert_eq!(
            deletes,
            vec![cloud_key],
            "the orphaned blob is queued for deletion"
        );
    }

    fn row(table: &str, op: ChangeOp, cols: &[&str]) -> RowChange {
        RowChange {
            table: table.to_string(),
            op,
            columns: cols.iter().map(|c| Some(c.to_string())).collect(),
        }
    }

    fn source() -> BaeBlobSource {
        BaeBlobSource::new(LibraryDir::new("/lib"))
    }

    /// A full `library_images` `RowChange` in DDL order, with `cloud_path`
    /// placed via the module's index constant so the source reads the same key
    /// the production walker would. NULL columns model an opaque-home row.
    fn image_row(op: ChangeOp, id: &str, image_type: &str, cloud_path: Option<&str>) -> RowChange {
        let mut columns: Vec<Option<String>> = vec![None; LIBRARY_IMAGES_COLUMNS.len()];
        columns[0] = Some(id.to_string());
        columns[1] = Some(image_type.to_string());
        columns[LIBRARY_IMAGES_CLOUD_PATH_INDEX] = cloud_path.map(|p| p.to_string());
        RowChange {
            table: "library_images".to_string(),
            op,
            columns,
        }
    }

    #[test]
    fn every_image_uses_master_scope() {
        // Covers and artist images both encrypt with the library master key.
        for image_type in ["cover", "artist"] {
            let refs =
                source().blobs_for_change(&image_row(ChangeOp::Insert, "img-1", image_type, None));
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].namespace, "images");
            assert_eq!(refs[0].id, "img-1");
            assert_eq!(refs[0].scope, BlobScope::Master);
        }
    }

    #[test]
    fn opaque_image_carries_no_cloud_path_browsable_carries_it() {
        // An opaque-home row (cloud_path NULL) → BlobRef.cloud_path None; a
        // browsable-home row → the stored readable key, read at its DDL index.
        let refs = source().blobs_for_change(&image_row(ChangeOp::Insert, "rel-1", "cover", None));
        assert_eq!(refs[0].cloud_path, None);

        let refs = source().blobs_for_change(&image_row(
            ChangeOp::Insert,
            "rel-1",
            "cover",
            Some("Artist Name/Album Title/cover.jpg"),
        ));
        assert_eq!(
            refs[0].cloud_path.as_deref(),
            Some("Artist Name/Album Title/cover.jpg")
        );
    }

    #[test]
    fn blobs_in_db_lists_every_image_row_with_scope_and_cloud_path() {
        // The snapshot-bootstrap enumeration must produce the same (id → scope,
        // cloud_path) mapping as the changeset path. (Audio lives in other
        // tables and is never listed — it streams on demand.)
        let conn = coven::rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE library_images (id TEXT PRIMARY KEY, type TEXT NOT NULL, cloud_path TEXT);\n\
             INSERT INTO library_images (id, type, cloud_path) VALUES ('rel-1', 'cover', 'Artist Name/Album Title/cover.jpg');\n\
             INSERT INTO library_images (id, type, cloud_path) VALUES ('img-2', 'artist', NULL);",
        )
        .unwrap();

        let refs = source().blobs_in_db(&conn).unwrap();
        assert_eq!(refs.len(), 2);

        let cover = refs.iter().find(|r| r.id == "rel-1").unwrap();
        assert_eq!(cover.namespace, "images");
        assert_eq!(cover.scope, BlobScope::Master);
        assert_eq!(
            cover.cloud_path.as_deref(),
            Some("Artist Name/Album Title/cover.jpg")
        );

        let artist = refs.iter().find(|r| r.id == "img-2").unwrap();
        assert_eq!(artist.namespace, "images");
        assert_eq!(artist.scope, BlobScope::Master);
        assert_eq!(artist.cloud_path, None);
    }

    #[test]
    fn updates_deletes_and_other_tables_carry_no_blobs() {
        // Image bytes never change on update; deletes go through the cloud
        // outbox; non-blob tables (albums, etc.) carry nothing. coven walks one
        // change at a time, so each must individually yield no blob.
        let changes = [
            image_row(ChangeOp::Update, "img-1", "cover", None),
            image_row(ChangeOp::Delete, "img-1", "cover", None),
            row("albums", ChangeOp::Insert, &["a-1"]),
        ];
        let source = source();
        for change in &changes {
            assert!(source.blobs_for_change(change).is_empty());
        }
    }

    /// Pins the DDL↔index coupling: insert a `library_images` row through the
    /// REAL migration schema, capture the emitted changeset with a real SQLite
    /// session, walk it with coven's production walker, and assert the value at
    /// `LIBRARY_IMAGES_CLOUD_PATH_INDEX` is the stored `cloud_path`. A column
    /// reorder in `001_initial.sql` (or in `LIBRARY_IMAGES_COLUMNS`) moves the
    /// real index out from under the constant and fails here loudly.
    #[test]
    fn changeset_cloud_path_index_matches_real_schema() {
        use coven::rusqlite::session::Session;

        let conn = coven::rusqlite::Connection::open_in_memory().unwrap();
        // The actual bae schema — the single source of truth for column order.
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();

        let mut session = Session::new(&conn).unwrap();
        session.attach(Some("library_images")).unwrap();

        let now = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO library_images \
             (id, type, content_type, file_size, source, cloud_path, _updated_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            coven::rusqlite::params![
                "rel-1",
                "cover",
                "image/jpeg",
                123,
                "local",
                "Artist Name/Album Title/cover.jpg",
                now,
                now,
            ],
        )
        .unwrap();

        let mut buf = Vec::new();
        session.changeset_strm(&mut buf).unwrap();
        let changes = coven::changeset::walk(&buf).unwrap();

        let row = changes
            .iter()
            .find(|c| c.table == "library_images")
            .expect("the insert is in the changeset");
        // The DDL column count must match the constant list, or the index is
        // meaningless.
        assert_eq!(row.columns.len(), LIBRARY_IMAGES_COLUMNS.len());
        assert_eq!(
            row.col(LIBRARY_IMAGES_CLOUD_PATH_INDEX),
            Some("Artist Name/Album Title/cover.jpg"),
            "cloud_path is at the index the constant claims",
        );
    }
}
