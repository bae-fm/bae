//! Release storage transitions — moving a release between Unmanaged and Managed,
//! and pinning/unpinning a managed release in coven's cache.
//!
//! - `manage_release`: upload an Unmanaged release to the cloud home (the same
//!   transition the import's managed path runs, via
//!   `LibraryManager::enqueue_managed_uploads`). The upload observer flips
//!   `managed` true and deletes the in-place source once the last upload lands.
//! - `unmanage_release`: copy a managed release's bytes back out to a user folder
//!   (read through coven's cache), flip it Unmanaged, then delete the cloud blobs.
//! - `pin_release_task` / `unpin_release`: pin/unpin a managed release in coven's
//!   cache (`storage/pinned/` vs the evictable `storage/cache/`).
//!
//! Pinned-ness is coven cache state; bae stores no pin flag. A managed release's
//! bytes live only in coven's cache — never a bae `storage/` path. The
//! durability invariant still holds for the copy-out: `unmanage` writes and
//! verifies every file at the new location before any cloud delete is queued.

use crate::db::{DbFile, DbReleaseUnmanagedSource};
use crate::library::LibraryManager;
use tokio::sync::mpsc;
use tracing::{error, info};

/// Read one release file's bytes from wherever it lives on this device: an
/// unmanaged release's in-place source file, or — for a managed release — through
/// coven's cache (served from `storage/pinned/` or `storage/cache/` on a hit,
/// fetched from the cloud on a miss). Verifies `bytes.len() == file.file_size` so
/// a short or zero read aborts the caller before any delete is queued (the SAFETY
/// INVARIANT).
pub async fn read_release_file_bytes(
    unmanaged_source: Option<&DbReleaseUnmanagedSource>,
    file: &DbFile,
    mgr: &LibraryManager,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::library::manager::ReadableFileSource;

    let source = mgr
        .resolve_readable_file_source(unmanaged_source, file)
        .await?;
    let bytes = match source {
        ReadableFileSource::Local(local_path) => tokio::fs::read(&local_path).await?,
        ReadableFileSource::UploadPendingSourceMissing => {
            // A queued upload whose source file is gone: the cloud object may not
            // exist yet, so a cache read would miss into a 404. Report the
            // in-flight upload instead.
            return Err(format!(
                "File {} is still uploading — its source is gone and the cloud copy isn't \
                 available yet",
                file.id
            )
            .into());
        }
        ReadableFileSource::Managed => {
            // Read the whole blob through coven's cache (cache-or-cloud,
            // transparent). Served from the local pinned/cache file on a hit,
            // fetched + decrypted from the cloud on a miss.
            mgr.read_managed_file(file).await?
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
/// `fsync(2)` (`File::sync_all`), so a crash right after the cloud copy is deleted
/// can't lose a destination copy that was still only in the OS page cache.
/// Fail-closed: any fsync error propagates, so the caller aborts before queuing a
/// delete. (On macOS this reaches the drive's cache, not the platter — that needs
/// `F_FULLFSYNC` — but it closes the page-cache window these transitions care
/// about.)
async fn fsync_file_and_dir(path: &std::path::Path) -> std::io::Result<()> {
    tokio::fs::File::open(path).await?.sync_all().await?;
    if let Some(parent) = path.parent() {
        tokio::fs::File::open(parent).await?.sync_all().await?;
    }
    Ok(())
}

/// Progress updates emitted during a pin, unpin, manage, or unmanage operation.
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

/// Send one `FileProgress` update.
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

/// Pin/unpin/manage/unmanage service for managed releases.
pub struct TransferService {
    library_manager: LibraryManager,
}

impl TransferService {
    pub fn new(library_manager: LibraryManager) -> Self {
        Self { library_manager }
    }

    /// Pin a release: have coven fetch its blobs into `storage/pinned/` on a
    /// spawned task. Returns a receiver for progress updates plus the task's
    /// handle, so the download queue worker can abort the fetch when the user
    /// cancels — a half-finished pin just leaves some blobs in `pinned/` and the
    /// release reads as not-yet-pinned, so re-pinning resumes idempotently.
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

    /// Unpin a release: move its blobs from `storage/pinned/` to the evictable
    /// `storage/cache/`. Returns a receiver for progress updates.
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

    /// Manage an unmanaged release: upload it to the cloud home, keeping its blobs
    /// pinned in coven's cache iff `pin`. Returns a receiver for progress updates;
    /// the actual upload progress flows through the outbox snapshot, so this emits
    /// Started then Complete (the uploads are now queued and draining).
    pub fn manage_release(
        &self,
        release_id: String,
        pin: bool,
    ) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        tokio::spawn(async move {
            let result = do_manage(&release_id, pin, &library_manager, &tx).await;

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
    /// cloud copies. Returns a receiver for progress updates.
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
    if !release.managed {
        return Err("Cannot pin an unmanaged release".into());
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

    // coven fetches every blob into `storage/pinned/` (from the evictable cache
    // if already there, else the cloud). Pinned-ness is coven cache state.
    mgr.pin_release_blobs(release_id).await?;

    info!("Pin complete for release {}", release_id);
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
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
    if !release.managed {
        return Err("Cannot unpin an unmanaged release".into());
    }

    let files = mgr.get_files_for_release(release_id).await?;
    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: files.len(),
    });
    info!("Unpinning release {} ({} files)", release_id, files.len());

    // coven moves each blob from `storage/pinned/` to the evictable
    // `storage/cache/` (still readable, now droppable). No cloud read.
    mgr.unpin_release_blobs(release_id).await?;

    info!("Unpin complete for release {}", release_id);
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}

/// Manage an unmanaged release: enqueue its cloud uploads (carrying the `pin`
/// retain-pinned intent) so the sync loop drains them and the upload observer
/// flips it managed + deletes the in-place source. The same transition the import
/// runs, plus an is-sync-ready gate: the observer fires from the sync loop's
/// outbox drain, so the loop must actually be running — otherwise the uploads sit
/// forever, the release stays unmanaged, and no error surfaces.
async fn do_manage(
    release_id: &str,
    pin: bool,
    library_manager: &LibraryManager,
    tx: &mpsc::UnboundedSender<TransferProgress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = library_manager;

    let release = mgr
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;
    if release.managed {
        return Err("Release is already managed".into());
    }
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

    // The release reaches `managed = true` only when the upload observer marks it
    // after its last upload lands. That observer fires from the sync loop's outbox
    // drain, so the loop must be running — otherwise the uploads sit forever and
    // the release stays unmanaged with no error. A configured cloud home isn't
    // enough; the loop must be draining.
    if !mgr.is_sync_ready() {
        return Err(
            "Cannot manage a release while sync isn't running — it would never finish \
             uploading and would stay unmanaged"
                .into(),
        );
    }

    // Enqueue the uploads (verifies every source intact first) and kick the loop.
    // The observer flips `managed` true and deletes the in-place source on the
    // last upload; per-file upload progress flows through the outbox snapshot.
    mgr.enqueue_managed_uploads(release_id, pin).await?;

    info!("Manage queued for release {} (pin={pin})", release_id);
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}

/// Unmanage a managed release: copy every file back out to `new_path`, then drop
/// the cloud copies.
///
/// Durability-first: every file is read (through coven's cache), verified, written
/// to `new_path/{original_filename}`, and re-stat-verified BEFORE any delete is
/// queued. On any per-file failure the release stays managed and every cloud copy
/// is intact (the SAFETY INVARIANT). Only after all files are durable does it flip
/// the release to Unmanaged and queue the cloud-blob deletes.
///
/// NOTE: under the current coven gate a `managed` true→false flip is a freeze, not
/// a retract, so peers keep the row while this device deletes the cloud blob — the
/// known down-direction limitation deferred to the gate-retract follow-up. This
/// function is otherwise correct as-written.
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
    if !release.managed {
        return Err("Release is already unmanaged".into());
    }
    let files = mgr.get_files_for_release(release_id).await?;
    if files.is_empty() {
        return Err("Release has no files".into());
    }

    let _ = tx.send(TransferProgress::Started {
        release_id: release_id.to_string(),
        total_files: files.len(),
    });
    info!("Unmanaging release {} ({} files)", release_id, files.len());

    let dest_dir = std::path::Path::new(new_path);
    tokio::fs::create_dir_all(dest_dir).await?;

    // Copies written so far at `new_path`. If the user cancels mid-transfer we
    // delete these so a cancelled unmanage leaves no orphans — the cloud copies
    // are never touched until the irreversible flip below, so the release stays
    // fully managed and intact.
    let mut written: Vec<std::path::PathBuf> = Vec::new();

    for (i, file) in files.iter().enumerate() {
        if cancel.is_cancelled() {
            return cancel_unmanage(release_id, &written, tx).await;
        }

        send_file_progress(tx, release_id, i, files.len(), &file.original_filename, 0);

        // Verified read through coven's cache (the release is still managed, so it
        // has no in-place source — pass `None`). A missing/short blob aborts here,
        // before any delete.
        let data = read_release_file_bytes(None, file, mgr).await?;

        let dest = dest_dir.join(&file.original_filename);
        tokio::fs::write(&dest, &data).await?;

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

        // Flush to disk before any cloud copy is deleted (the cloud delete fires
        // next sync, so the new copy must be durable first, not merely in the page
        // cache).
        fsync_file_and_dir(&dest).await?;
        written.push(dest);

        send_file_progress(tx, release_id, i, files.len(), &file.original_filename, 100);
    }

    if cancel.is_cancelled() {
        return cancel_unmanage(release_id, &written, tx).await;
    }

    // Every file is now durable at `new_path`. Flip the release to Unmanaged
    // (recording `new_path` as its source), THEN queue the cloud-blob deletes and
    // drop coven's cache copies.
    mgr.set_release_unmanaged_path(release_id, new_path).await?;
    mgr.queue_storage_deletions(&files).await;

    info!("Unmanage complete for release {}", release_id);
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}

/// Roll back a cancelled unmanage: delete the partial copies written at the new
/// path (the cloud copies were never touched) and end the transfer cleanly so the
/// release stays managed. Reported as `Complete` because the transfer is over —
/// not `Failed`, since the user asked for the stop.
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
            tracing::warn!(
                "Failed to remove partial unmanage copy {}: {e}",
                path.display()
            );
        }
    }
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}
