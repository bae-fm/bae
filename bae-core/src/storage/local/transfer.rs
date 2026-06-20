//! Release storage transitions — moving a release between Unmanaged, Pinned,
//! and CloudOnly.
//!
//! - `pin_release`: downloads from cloud (or copies from unmanaged path), writes
//!   to `storage/ab/cd/{file_id}`, sets `pinned_locally = true`.
//! - `unpin_release`: queues local copies for deferred deletion, sets
//!   `pinned_locally = false`. Rejected unless a cloud home exists and no
//!   upload is still pending (the cloud copy must be durable, not intended).
//! - `manage_release`: uploads an Unmanaged release to the cloud home, landing
//!   it at Pinned or CloudOnly.
//! - `unmanage_release`: copies a managed release back out to a user folder and
//!   drops the managed copies.
//!
//! Every transition obeys the SAFETY INVARIANT: a durable copy is verified at
//! the destination (`bytes.len() == file_size` on read and write) before any
//! delete (cloud-outbox or local pending-deletion) is queued. On any per-file
//! failure the transition aborts and queues nothing.

use crate::db::{DbFile, DbReleaseLocalCopy};
use crate::library::LibraryManager;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::cleanup::PendingDeletion;

/// Read one release file's bytes from wherever it currently lives: this
/// device's local copy (managed `storage/` copy or an unmanaged original),
/// the original source of a still-pending cloud upload, or otherwise a cloud
/// read through the home's at-rest cipher. Verifies `bytes.len() == file.file_size`
/// so a short or zero read aborts the caller before any delete is queued (SAFETY
/// INVARIANT).
///
/// The cipher applies only on the cloud path — a local read is always verbatim.
/// An opaque home decrypts the blob under the library master key; a browsable
/// home reads the verbatim bytes. The cipher is absent only for an opaque,
/// locked library — a broken invariant for a managed release (which always has
/// an unlocked library), surfaced as an error rather than masked.
pub async fn read_release_file_bytes(
    local_copy: Option<&DbReleaseLocalCopy>,
    file: &DbFile,
    mgr: &LibraryManager,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::library::manager::ReadableFileSource;

    let source = mgr.resolve_readable_file_source(local_copy, file).await?;
    let bytes = match source {
        ReadableFileSource::Local(local_path) => tokio::fs::read(&local_path).await?,
        ReadableFileSource::UploadPendingSourceMissing => {
            // A queued upload whose source file is gone: the cloud object may
            // not exist yet, so a cloud read would 404 with a raw storage key.
            // Report the in-flight upload instead.
            return Err(format!(
                "File {} is still uploading — its source is gone and the cloud copy isn't \
                 available yet",
                file.id
            )
            .into());
        }
        ReadableFileSource::CloudOnly => {
            let cloud_home = mgr.get_cloud_home().ok_or_else(|| {
                format!(
                    "File {} is cloud-only but no cloud home is configured",
                    file.id
                )
            })?;
            let cipher = mgr
                .cloud_blob_cipher()
                .ok_or_else(|| format!("no blob cipher for managed release file {}", file.id))?;
            // Read the whole object through the home's cipher: one ranged read
            // that decrypts under the master key on an opaque home, or returns
            // the verbatim bytes on a browsable one. Every managed blob is
            // master-scoped (see `BaeBlobPlan`). The object key is the row's
            // stored `cloud_path`; a NULL value means the hashed-by-id layout
            // (`storage_path`), the documented default — not a masked error.
            let source_size = file.file_size as u64;
            let reader = crate::storage::BlobRangeReader::new(
                cloud_home,
                &cipher,
                coven::blob::ResolvedScope::Master,
                file.cloud_key(),
                source_size,
            );
            reader.read(0, source_size).await?
        }
        ReadableFileSource::Unreachable => {
            return Err(format!("File {} has no readable location", file.id).into());
        }
    };

    if bytes.len() as i64 != file.file_size {
        return Err(format!(
            "File {} short read: got {} bytes, expected {}",
            file.id,
            bytes.len(),
            file.file_size
        )
        .into());
    }

    Ok(bytes)
}

/// Flush a just-written destination file and its directory entry to disk with
/// `fsync(2)` (`File::sync_all`), so a crash right after the source/cloud copy
/// is deleted can't lose a destination copy that was still only in the OS page
/// cache. Fail-closed: any fsync error propagates, so the caller aborts before
/// queuing a delete. (On macOS this reaches the drive's cache, not the platter
/// — that needs `F_FULLFSYNC` — but it closes the page-cache window these
/// transitions care about.)
async fn fsync_file_and_dir(path: &std::path::Path) -> std::io::Result<()> {
    tokio::fs::File::open(path).await?.sync_all().await?;
    if let Some(parent) = path.parent() {
        tokio::fs::File::open(parent).await?.sync_all().await?;
    }
    Ok(())
}

/// Progress updates emitted during a pin or unpin operation
#[derive(Debug, Clone)]
pub enum TransferProgress {
    /// Operation started
    Started {
        release_id: String,
        total_files: usize,
    },
    /// A file is being processed
    FileProgress {
        release_id: String,
        file_index: usize,
        total_files: usize,
        filename: String,
        percent: u8,
    },
    /// Operation completed
    Complete { release_id: String },
    /// Operation failed
    Failed { release_id: String, error: String },
}

/// Send one `FileProgress` update. Shared by the per-file loop's own progress
/// callbacks and the byte-level write callbacks `store_from_path` /
/// `store_bytes` invoke, so the payload is built in exactly one place.
fn send_file_progress(
    tx: &mpsc::UnboundedSender<TransferProgress>,
    release_id: &str,
    file_index: usize,
    total_files: usize,
    filename: &str,
    percent: u8,
) {
    let _ = tx.send(TransferProgress::FileProgress {
        release_id: release_id.to_string(),
        file_index,
        total_files,
        filename: filename.to_string(),
        percent,
    });
}

/// Pin/unpin service for managing local copies of releases
pub struct TransferService {
    library_manager: LibraryManager,
}

impl TransferService {
    pub fn new(library_manager: LibraryManager) -> Self {
        Self { library_manager }
    }

    /// Pin a release: download/copy its files to managed local storage on a
    /// spawned task. Returns a receiver for progress updates plus the task's
    /// handle, so the download queue worker can abort the actual download (not
    /// just stop draining its progress) when the user cancels. Aborting the task
    /// mid-download leaves a `<dest>.part` temp file that never renames into
    /// place, so the release stays cloud-only.
    pub fn pin_release_task(
        &self,
        release_id: String,
    ) -> (
        mpsc::UnboundedReceiver<TransferProgress>,
        tokio::task::JoinHandle<()>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        let task = tokio::spawn(async move {
            let result = do_pin(&release_id, &library_manager, &tx).await;

            if let Err(e) = result {
                error!("Pin failed for release {}: {}", release_id, e);
                let _ = tx.send(TransferProgress::Failed {
                    release_id,
                    error: e.to_string(),
                });
            }
        });

        (rx, task)
    }

    /// Unpin a release: delete local copies, mark as cloud-only.
    ///
    /// Returns a receiver for progress updates.
    pub fn unpin_release(&self, release_id: String) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        tokio::spawn(async move {
            let result = do_unpin(&release_id, &library_manager, &tx).await;

            if let Err(e) = result {
                error!("Unpin failed for release {}: {}", release_id, e);
                let _ = tx.send(TransferProgress::Failed {
                    release_id,
                    error: e.to_string(),
                });
            }
        });

        rx
    }

    /// Manage an unmanaged release: upload to the cloud home, landing at
    /// Pinned (`pin = true`) or CloudOnly (`pin = false`).
    ///
    /// Returns a receiver for progress updates.
    pub fn manage_release(
        &self,
        release_id: String,
        pin: bool,
        delete_unmanaged_source: bool,
    ) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        tokio::spawn(async move {
            let result = do_manage(
                &release_id,
                pin,
                delete_unmanaged_source,
                &library_manager,
                &tx,
            )
            .await;

            if let Err(e) = result {
                error!("Manage failed for release {}: {}", release_id, e);
                let _ = tx.send(TransferProgress::Failed {
                    release_id,
                    error: e.to_string(),
                });
            }
        });

        rx
    }

    /// Unmanage a managed release: copy its files to `new_path` and drop the
    /// managed copies.
    ///
    /// Returns a receiver for progress updates.
    pub fn unmanage_release(
        &self,
        release_id: String,
        new_path: String,
        cancel: crate::library::CancellationToken,
    ) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        tokio::spawn(async move {
            let result = do_unmanage(&release_id, &new_path, &cancel, &library_manager, &tx).await;

            if let Err(e) = result {
                error!("Unmanage failed for release {}: {}", release_id, e);
                let _ = tx.send(TransferProgress::Failed {
                    release_id,
                    error: e.to_string(),
                });
            }
        });

        rx
    }
}

async fn do_pin(
    release_id: &str,
    library_manager: &LibraryManager,
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager;

    let release = mgr
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;
    let local_copy = mgr.get_release_local_copy(release_id).await?;

    if !release.managed {
        return Err("Cannot pin a local-library release".into());
    }

    if local_copy.as_ref().is_some_and(|c| c.pinned_locally) {
        return Err("Release is already pinned locally".into());
    }

    let files = mgr.get_files_for_release(release_id).await?;

    if files.is_empty() {
        return Err("Release has no files".into());
    }

    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: files.len(),
    });

    info!("Pinning release {} ({} files)", release_id, files.len());

    let storage = mgr.create_release_storage();
    let total_files = files.len();

    for (i, file) in files.iter().enumerate() {
        use crate::library::manager::ReadableFileSource;

        let report_progress = |percent: u8| {
            send_file_progress(
                tx,
                release_id,
                i,
                total_files,
                &file.original_filename,
                percent,
            )
        };
        report_progress(0);

        match mgr
            .resolve_readable_file_source(local_copy.as_ref(), file)
            .await?
        {
            ReadableFileSource::Local(source_path) => {
                // A local copy already holds the bytes; stream it from disk into
                // storage (length-verified below) without buffering in memory.
                let tx_clone = tx.clone();
                let rid = release_id.to_string();
                let fname = file.original_filename.clone();
                storage
                    .store_from_path(
                        &file.id,
                        &source_path,
                        Box::new(move |bytes_written, total_bytes| {
                            let percent = if total_bytes > 0 {
                                ((bytes_written as f64 / total_bytes as f64) * 100.0) as u8
                            } else {
                                100
                            };
                            send_file_progress(&tx_clone, &rid, i, total_files, &fname, percent);
                        }),
                    )
                    .await?;
                // A short copy must abort the pin before it's marked durable.
                let stored = mgr.local_storage_path_for_file(file);
                let stored_len = tokio::fs::metadata(&stored).await?.len();
                if stored_len as i64 != file.file_size {
                    return Err(format!(
                        "Pinned local copy of {} is {} bytes, expected {}",
                        file.id, stored_len, file.file_size
                    )
                    .into());
                }
            }
            ReadableFileSource::UploadPendingSourceMissing => {
                return Err(format!(
                    "File {} has a queued upload whose source is gone — its cloud object may not \
                     exist yet",
                    file.id
                )
                .into());
            }
            ReadableFileSource::CloudOnly => {
                let cloud_home = mgr
                    .get_cloud_home()
                    .ok_or("Cannot pin a cloud-only release without a cloud home")?;
                // The cloud download reads through the home's cipher: it decrypts
                // on an opaque home, reads verbatim on a browsable one. The cipher
                // is absent only for an opaque, locked library — a broken
                // invariant for a managed release, surfaced as an error.
                let cipher = mgr
                    .cloud_blob_cipher()
                    .ok_or_else(|| "no blob cipher for managed release".to_string())?;
                let dest = mgr.local_storage_path_for_file(file);
                download_cloud_file_chunked(cloud_home, cipher, file, &dest, &report_progress)
                    .await?;
            }
            ReadableFileSource::Unreachable => {
                return Err(format!(
                    "Cannot pin file {}: its source is gone and it was never uploaded",
                    file.id
                )
                .into());
            }
        }

        report_progress(100);
    }

    mgr.pin_release_locally(release_id).await?;

    info!("Pin complete for release {}", release_id);

    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });

    Ok(())
}

/// Plaintext window size for a chunked cloud pin download: one window is read
/// (decrypted on an opaque home, verbatim on a browsable one) and appended to
/// the temp file before the next is fetched, so peak memory is one window
/// regardless of file size.
const PIN_DOWNLOAD_WINDOW: u64 = 1_048_576;

/// Download a single managed cloud file to `dest` through the home's `cipher`,
/// one `PIN_DOWNLOAD_WINDOW`-sized plaintext window at a time — decrypting on an
/// opaque home, reading verbatim on a browsable one. Each window is retried so a
/// transient provider stall doesn't kill the whole pin. The bytes land in a
/// `<dest>.part` temp file that is length-verified, fsync'd, and renamed into
/// place; any failure removes the temp file so a partial download never
/// masquerades as a pinned copy.
async fn download_cloud_file_chunked(
    cloud_home: std::sync::Arc<dyn crate::storage::cloud::CloudHome>,
    cipher: coven::sync::cloud_storage::CloudCipher,
    file: &DbFile,
    dest: &std::path::Path,
    on_progress: &(dyn Fn(u8) + Send + Sync),
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let source_size = file.file_size as u64;
    // Every managed blob is master-scoped (see `BaeBlobPlan`). The object key is
    // the file's effective cloud key (its stored `cloud_path`, else the
    // hashed-by-id default).
    let reader = crate::storage::BlobRangeReader::new(
        cloud_home,
        &cipher,
        coven::blob::ResolvedScope::Master,
        file.cloud_key(),
        source_size,
    );

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part_path = part_path(dest);

    let result = stream_windows_to_part(&reader, &part_path, source_size, on_progress).await;
    if let Err(e) = result {
        // A failed download leaves no `.part` behind.
        remove_part_file(&part_path).await;
        return Err(e);
    }

    // Verify the on-disk length, flush, then publish atomically by rename.
    let written = tokio::fs::metadata(&part_path).await?.len();
    if written != source_size {
        remove_part_file(&part_path).await;
        return Err(format!(
            "Pinned download of {} is {} bytes, expected {}",
            file.id, written, source_size
        )
        .into());
    }
    fsync_file_and_dir(&part_path).await?;
    tokio::fs::rename(&part_path, dest).await?;
    Ok(())
}

/// Stream every plaintext window of `source_size` from `reader` into the
/// `.part` file at `part_path`, retrying each window. Returns once the last
/// window is written; the caller verifies length and renames.
async fn stream_windows_to_part(
    reader: &crate::storage::BlobRangeReader,
    part_path: &std::path::Path,
    source_size: u64,
    on_progress: &(dyn Fn(u8) + Send + Sync),
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncWriteExt;

    let part = tokio::fs::File::create(part_path).await?;
    let mut writer = tokio::io::BufWriter::new(part);

    let mut offset = 0u64;
    while offset < source_size {
        let len = PIN_DOWNLOAD_WINDOW.min(source_size - offset);
        let window =
            crate::retry::retry_with_backoff(3, "pin window download", || reader.read(offset, len))
                .await
                .map_err(|e| format!("Pin download window at offset {offset} failed: {e}"))?;
        if window.len() as u64 != len {
            return Err(format!(
                "Pin download window at offset {offset} returned {} bytes, expected {len}",
                window.len()
            )
            .into());
        }
        writer.write_all(&window).await?;
        offset += len;
        let percent = ((offset as f64 / source_size as f64) * 100.0) as u8;
        on_progress(percent);
    }
    writer.flush().await?;
    Ok(())
}

/// The `<dest>.part` temp path a chunked pin download writes to before renaming
/// into place.
fn part_path(dest: &std::path::Path) -> std::path::PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".part");
    std::path::PathBuf::from(name)
}

/// Remove a failed download's `.part` file, ignoring an already-absent file
/// (a download that failed before opening it leaves nothing behind) and
/// logging any other removal failure rather than swallowing it.
async fn remove_part_file(part_path: &std::path::Path) {
    if let Err(rm) = tokio::fs::remove_file(part_path).await {
        if rm.kind() == std::io::ErrorKind::NotFound {
            debug!(
                "No partial pin download to remove at {} (download failed before opening it)",
                part_path.display()
            );
        } else {
            warn!(
                "Failed to remove partial pin download {}: {rm}",
                part_path.display()
            );
        }
    }
}

async fn do_unpin(
    release_id: &str,
    library_manager: &LibraryManager,
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager;

    let release = mgr
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;
    let local_copy = mgr.get_release_local_copy(release_id).await?;

    if !release.managed {
        return Err("Cannot unpin a local-library release".into());
    }

    if !local_copy.as_ref().is_some_and(|c| c.pinned_locally) {
        return Err("Release is not pinned locally".into());
    }

    // Dropping the local copy is safe only when the cloud holds a durable
    // copy: a cloud home must exist AND no upload may still be pending (a
    // pending upload means the cloud copy is merely intended, not confirmed).
    if mgr.get_cloud_home().is_none() {
        return Err("Cannot unpin without a cloud home".into());
    }
    if mgr.count_pending_uploads_for_release(release_id).await? != 0 {
        return Err("Cannot unpin while an upload is still pending".into());
    }

    let files = mgr.get_files_for_release(release_id).await?;

    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: files.len(),
    });

    info!("Unpinning release {} ({} files)", release_id, files.len());

    // Queue local copies for deferred deletion
    let pending: Vec<PendingDeletion> = files
        .iter()
        .map(|f| PendingDeletion::Local {
            path: mgr.local_storage_path_for_file(f).display().to_string(),
        })
        .collect();

    if !pending.is_empty() {
        if let Err(e) = mgr.append_pending_deletions(&pending).await {
            warn!("Failed to queue deferred deletions: {}", e);
        }
    }

    mgr.unpin_release_locally(release_id).await?;

    info!("Unpin complete for release {}", release_id);

    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });

    Ok(())
}

/// Manage an Unmanaged release: upload its files to the cloud home, landing it
/// at Pinned (`pin = true`) or CloudOnly (`pin = false`).
///
/// File bytes upload only through the background cloud outbox (mirroring the
/// import managed-upload path), never via a direct `CloudHome::write` — the
/// coven changeset doesn't carry the `files` table, so a direct write would
/// skip the only bookkeeping that records uploads.
async fn do_manage(
    release_id: &str,
    pin: bool,
    delete_unmanaged_source: bool,
    library_manager: &LibraryManager,
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager;

    let release = mgr
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;
    let local_copy = mgr.get_release_local_copy(release_id).await?;

    if release.managed {
        return Err("Release is already managed".into());
    }
    let unmanaged_path = local_copy
        .as_ref()
        .and_then(|c| c.unmanaged_path.clone())
        .ok_or("Unmanaged release has no local copy on this device")?;

    if mgr.get_cloud_home().is_none() {
        return Err("Cannot manage without a cloud home".into());
    }

    let files = mgr.get_files_for_release(release_id).await?;
    if files.is_empty() {
        return Err("Release has no files".into());
    }

    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: files.len(),
    });

    info!(
        "Managing release {} ({} files, pin={pin})",
        release_id,
        files.len()
    );

    // Both transitions reach `managed = true` only when the upload observer marks
    // the release after its last upload lands (the pin path keeps this device's
    // local copy, the cloud-only path drops it). That observer fires from the
    // sync loop's outbox drain, so the loop must actually be running — otherwise
    // the uploads sit forever, the release stays unmanaged, and no error
    // surfaces. A configured cloud home isn't enough; the loop must be draining.
    if !mgr.is_sync_ready() {
        return Err(
            "Cannot manage a release while sync isn't running — it would never finish \
             uploading and would stay unmanaged"
                .into(),
        );
    }

    if pin {
        // Pin path: stage a verified copy in `storage/`, enqueue uploads that
        // read from there, then (only after the storage/ copy is durable) the
        // originals may be deleted immediately.
        let storage = mgr.create_release_storage();
        for (i, file) in files.iter().enumerate() {
            send_file_progress(tx, release_id, i, files.len(), &file.original_filename, 0);

            // Verified read from the unmanaged original.
            let data = read_release_file_bytes(local_copy.as_ref(), file, mgr).await?;

            let tx_clone = tx.clone();
            let rid = release_id.to_string();
            let fname = file.original_filename.clone();
            let total_files = files.len();
            storage
                .store_bytes(
                    &file.id,
                    &data,
                    Box::new(move |bytes_written, total_bytes| {
                        let percent = if total_bytes > 0 {
                            ((bytes_written as f64 / total_bytes as f64) * 100.0) as u8
                        } else {
                            100
                        };
                        send_file_progress(&tx_clone, &rid, i, total_files, &fname, percent);
                    }),
                )
                .await?;

            // Re-stat the staged copy: a short write must abort before any
            // delete or upload of this release is considered durable.
            let stored_path = mgr.local_storage_path_for_file(file);
            let stored_len = tokio::fs::metadata(&stored_path).await?.len();
            if stored_len as i64 != file.file_size {
                return Err(format!(
                    "Staged copy of {} is {} bytes, expected {}",
                    file.id, stored_len, file.file_size
                )
                .into());
            }

            // Flush the staged copy before any original is deleted below.
            fsync_file_and_dir(&stored_path).await?;
        }

        // The verified `storage/` copies are this device's durable local copy.
        // Pin it and clear `unmanaged_path` BEFORE enqueueing uploads, so when
        // the upload observer fires on the last upload it sees a pinned copy and
        // flips the release to managed-pinned (keeping the copy) rather than
        // cloud-only. `managed` stays false until then.
        mgr.pin_release_locally(release_id).await?;

        // Now enqueue uploads that read the staged `storage/` copies. The key
        // is the readable path on a browsable home (computed + stored on the
        // file row here) or the hashed `storage_path` on an opaque one.
        for file in &files {
            let cloud_key = mgr.cloud_key_for_managed_file(file).await?;
            mgr.add_cloud_outbox_upload(&file.id, &cloud_key, None)
                .await?;
        }
        mgr.trigger_sync();

        if delete_unmanaged_source {
            // Safe now — `storage/` holds a verified copy of every file.
            for file in &files {
                let original = std::path::Path::new(&unmanaged_path).join(&file.original_filename);
                match tokio::fs::remove_file(&original).await {
                    Ok(()) => info!("Deleted managed-source original: {}", original.display()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => warn!(
                        "Failed to delete source original {}: {e}",
                        original.display()
                    ),
                }
            }
        }
    } else {
        // CloudOnly path: upload directly from the originals; do NOT stage a
        // local copy. The originals stay until the observer confirms the last
        // upload landed, which is also when the release flips to managed
        // (cloud-only, dropping no local copy since there is none).
        //
        // Verify EVERY source is intact (on-disk length == recorded
        // file_size) before enqueueing anything. The delete of the original is
        // deferred to the upload observer and CloudOnly keeps no other local
        // copy, so a truncated/corrupt source must abort the whole transition
        // here rather than upload short bytes and then have its only full copy
        // deleted. Verifying all sources first keeps Manage all-or-nothing.
        for file in &files {
            let source = std::path::Path::new(&unmanaged_path).join(&file.original_filename);
            let on_disk = tokio::fs::metadata(&source)
                .await
                .map_err(|e| format!("Cannot read manage source {}: {e}", source.display()))?
                .len();
            if on_disk != file.file_size as u64 {
                return Err(format!(
                    "Manage source {} is {on_disk} bytes but the release records {} — \
                     refusing to upload a truncated original whose only copy would then be deleted",
                    source.display(),
                    file.file_size
                )
                .into());
            }
        }

        for (i, file) in files.iter().enumerate() {
            send_file_progress(tx, release_id, i, files.len(), &file.original_filename, 0);

            let source = std::path::Path::new(&unmanaged_path).join(&file.original_filename);
            let source_str = source.to_string_lossy().to_string();
            // Readable key on a browsable home (computed + stored on the row),
            // hashed `storage_path` on an opaque one.
            let cloud_key = mgr.cloud_key_for_managed_file(file).await?;
            mgr.add_cloud_outbox_upload(&file.id, &cloud_key, Some(&source_str))
                .await?;
        }

        // The originals ARE the upload source, so a delete now would lose the
        // only copy on upload failure. Persist the intent; the upload observer
        // deletes them after the last upload lands, then clears `unmanaged_path`.
        if delete_unmanaged_source {
            mgr.set_release_delete_unmanaged_source_on_upload(release_id, true)
                .await?;
        }

        mgr.trigger_sync();
    }

    info!("Manage complete for release {} (pin={pin})", release_id);

    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });

    Ok(())
}

/// Unmanage a managed release (Pinned or CloudOnly): copy every file back out
/// to `new_path`, then drop the managed copies.
///
/// Durability-first: every file is read (local or cloud), verified, written to
/// `new_path/{original_filename}`, and re-stat-verified BEFORE any delete is
/// queued. On any per-file failure the release stays managed and every
/// cloud/storage copy is intact (the SAFETY INVARIANT). Only after all files
/// are durable does it set `unmanaged_path` and queue the managed-copy deletes.
async fn do_unmanage(
    release_id: &str,
    new_path: &str,
    cancel: &crate::library::CancellationToken,
    library_manager: &LibraryManager,
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager;

    let release = mgr
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;
    let local_copy = mgr.get_release_local_copy(release_id).await?;

    if !release.managed {
        return Err("Release is already unmanaged".into());
    }

    let files = mgr.get_files_for_release(release_id).await?;
    if files.is_empty() {
        return Err("Release has no files".into());
    }

    // Capture the pre-transition state for the later managed-copy deletes: the
    // cloud keys and (if pinned) the `storage/` paths are precomputed from the
    // still-managed release so they stay correct after the release flips to
    // unmanaged.
    let was_pinned = local_copy.as_ref().is_some_and(|c| c.pinned_locally);

    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: files.len(),
    });

    info!("Unmanaging release {} ({} files)", release_id, files.len());

    let dest_dir = std::path::Path::new(new_path);
    tokio::fs::create_dir_all(dest_dir).await?;

    // Copies written so far at `new_path`. If the user cancels mid-transfer we
    // delete these so a cancelled unmanage leaves no orphans — the managed cloud
    // and `storage/` copies are never touched until the irreversible flip below,
    // so the release stays fully managed and intact.
    let mut written: Vec<std::path::PathBuf> = Vec::new();

    // Write + verify every file at the new location before queueing any delete.
    for (i, file) in files.iter().enumerate() {
        // Cancellation is checked per file (the read/write of one file is the
        // slow unit); the current file finishes, then we stop and roll back.
        if cancel.is_cancelled() {
            return cancel_unmanage(release_id, &written, tx).await;
        }

        send_file_progress(tx, release_id, i, files.len(), &file.original_filename, 0);

        // Verified read: `storage/` if Pinned, else cloud download + decrypt.
        // A missing or short blob aborts here, before any delete.
        let data = read_release_file_bytes(local_copy.as_ref(), file, mgr).await?;

        let dest = dest_dir.join(&file.original_filename);
        tokio::fs::write(&dest, &data).await?;

        // Re-stat the written file: a short write aborts the whole transition.
        let written_len = tokio::fs::metadata(&dest).await?.len();
        if written_len as i64 != file.file_size {
            return Err(format!(
                "Wrote {} bytes to {}, expected {}",
                written_len,
                dest.display(),
                file.file_size
            )
            .into());
        }

        // Flush to disk before any managed copy is deleted (the cloud delete
        // fires next sync with no grace window, so the new copy must be durable
        // first, not merely in the page cache).
        fsync_file_and_dir(&dest).await?;
        written.push(dest);

        send_file_progress(tx, release_id, i, files.len(), &file.original_filename, 100);
    }

    // A cancel landing between the last write and the irreversible flip still
    // rolls back cleanly.
    if cancel.is_cancelled() {
        return cancel_unmanage(release_id, &written, tx).await;
    }

    // Every file is now durable at `new_path`. Flip the release to Unmanaged,
    // THEN queue the managed-copy deletes (cloud outbox + cancel pending uploads
    // + local `storage/` deletions) using the precomputed file set.
    mgr.set_release_unmanaged_path(release_id, new_path).await?;
    mgr.queue_storage_deletions(&files, was_pinned).await;

    info!("Unmanage complete for release {}", release_id);

    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });

    Ok(())
}

/// Roll back a cancelled unmanage: delete the partial copies written at the new
/// path (the managed cloud/`storage/` copies were never touched) and end the
/// transfer cleanly so the release stays managed. Reported as `Complete` because
/// the transfer is over — not as `Failed`, since the user asked for the stop.
async fn cancel_unmanage(
    release_id: &str,
    written: &[std::path::PathBuf],
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "Unmanage cancelled for {release_id}; removing {} partial copies",
        written.len()
    );
    for path in written {
        if let Err(e) = tokio::fs::remove_file(path).await {
            warn!(
                "Failed to remove partial unmanage copy {}: {e}",
                path.display()
            );
        }
    }
    // Best-effort like every other transfer-progress send: a gone receiver means
    // the driving transfer was already abandoned (view dismissed), which is
    // expected, not an error to surface.
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}
