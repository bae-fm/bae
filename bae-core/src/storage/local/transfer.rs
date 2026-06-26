//! Release storage transitions — moving a release between Local and Remote,
//! and pinning/unpinning a Remote release in coven's cache.
//!
//! coven owns the blob lifecycle; bae drives the user-facing transition (guards,
//! progress events) and calls into coven:
//!
//! - `make_release_remote`: `coven.make_remote` enqueues an upload per release
//!   file from its external (in-place) source, uploads each, and on the last
//!   flips `remote` true, drops the external refs, deletes the source files, and
//!   re-emits the subtree (the cover rides along). Completion fires the observer's
//!   `on_root_made_remote`, which emits `ReleaseUpdated`.
//! - `make_release_local`: `coven.make_local` materializes every blob back to a
//!   local file durability-first (release files to the chosen folder, the cover to
//!   coven's local store), then flips `remote` false, registers the external refs,
//!   and tombstones the cloud blobs — one atomic commit. A cancel before the
//!   commit rolls back the partial copies and leaves the release Remote.
//! - `pin_release_task` / `unpin_release`: pin/unpin a Remote release's blobs in
//!   coven's cache (`storage/pinned/` vs the evictable `storage/cache/`).
//!
//! Pinned-ness is coven cache state; bae stores no pin flag. A Remote release's
//! bytes live only in coven's cache — never a bae `storage/` path.

use crate::db::DbFile;
use crate::library::LibraryManager;
use tokio::sync::mpsc;
use tracing::{error, info};

/// Read one release file's whole plaintext through coven's locality-aware read:
/// the user's own file (a Local user-provided blob's external ref), coven's local
/// store (a Local host-provided blob), or coven's cache/cloud for a Remote blob.
/// Verifies `bytes.len() == file.file_size` so a short or zero read aborts the
/// caller before it trusts the bytes (defense-in-depth — coven validates an
/// external file's size and a torn cache file itself).
pub async fn read_release_file_bytes(
    file: &DbFile,
    mgr: &LibraryManager,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = mgr.read_release_blob(file).await?;
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

/// Progress updates emitted during a pin, unpin, make-Remote, or make-Local
/// operation.
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

/// Pin/unpin/make-Remote/make-Local service for releases.
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

    /// Make a Local release Remote: upload it to the cloud home, keeping its blobs
    /// pinned in coven's cache iff `pin`. Returns a receiver for progress updates;
    /// the upload progress flows through the outbox snapshot, so this emits Started
    /// then Complete (the uploads are now queued and draining via coven).
    pub fn make_release_remote(
        &self,
        release_id: String,
        pin: bool,
    ) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        tokio::spawn(async move {
            let result = do_make_remote(&release_id, pin, &library_manager, &tx).await;

            if let Err(e) = result {
                error!("Make-Remote failed for release {}: {}", release_id, e);
                let _ = tx.send(TransferProgress::Failed {
                    release_id,
                    error: e.to_string(),
                });
            }
        });

        rx
    }

    /// Make a Remote release Local: copy its files to `new_path` and drop the
    /// cloud copies (coven owns the durability-first ordering). Returns a receiver
    /// for progress updates.
    pub fn make_release_local(
        &self,
        release_id: String,
        new_path: String,
        cancel: crate::library::CancellationToken,
    ) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        tokio::spawn(async move {
            let result =
                do_make_local(&release_id, &new_path, &cancel, &library_manager, &tx).await;

            if let Err(e) = result {
                error!("Make-Local failed for release {}: {}", release_id, e);
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
    if !release.remote {
        return Err("Cannot pin a local release".into());
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
    if !release.remote {
        return Err("Cannot unpin a local release".into());
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

/// Make a Local release Remote: hand the transition to coven, which enqueues the
/// uploads (carrying the `pin` retain-pinned intent), uploads them, and on the
/// last one flips `remote` true, drops the external refs, deletes the source
/// files, and re-emits the subtree. An is-sync-ready gate stays here: coven's
/// upload drain runs from the sync loop, so the loop must be running, else the
/// uploads sit forever and the release stays Local with no error.
async fn do_make_remote(
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
    if release.remote {
        return Err("Release is already remote".into());
    }
    if mgr.get_cloud_home().is_none() {
        return Err("Cannot make a release remote without a cloud home".into());
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
        "Making release {} remote ({} files, pin={pin})",
        release_id,
        files.len()
    );

    // The release reaches `remote = true` only when coven's upload drain flips it
    // after the last upload lands. That drain runs from the sync loop, so the loop
    // must be running — otherwise the uploads sit forever and the release stays
    // Local with no error. A configured cloud home isn't enough; the loop must be
    // draining.
    if !mgr.is_sync_ready() {
        return Err(
            "Cannot make a release remote while sync isn't running — it would never finish \
             uploading and would stay local"
                .into(),
        );
    }

    // Hand the whole transition to coven: it verifies every external source,
    // enqueues the uploads, and kicks the loop. Per-file upload progress flows
    // through the outbox snapshot; the gate flip + source delete fire from coven's
    // drain, surfaced via the observer's `on_root_made_remote`.
    mgr.coven_make_remote(release_id, pin).await?;

    info!("Make-Remote queued for release {} (pin={pin})", release_id);
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}

/// Make a Remote release Local: hand the transition to coven, which materializes
/// every blob back to a local file durability-first (release files to
/// `new_path/{original_filename}`, the cover to coven's local store), then flips
/// `remote` false + registers the external refs + tombstones the cloud blobs in
/// one atomic commit. A cancel before the commit rolls back the partial copies and
/// leaves the release Remote (reported as a clean stop, not a failure).
async fn do_make_local(
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
    if !release.remote {
        return Err("Release is already local".into());
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
        "Making release {} local ({} files)",
        release_id,
        files.len()
    );

    tokio::fs::create_dir_all(std::path::Path::new(new_path)).await?;

    // coven materializes each blob durability-first, flips the gate false,
    // registers the external refs, and tombstones the cloud blobs in one atomic
    // commit; a cancel before the commit is rolled back and surfaced as Ok.
    mgr.coven_make_local(release_id, new_path, cancel).await?;

    info!("Make-Local complete for release {}", release_id);
    let _ = tx.send(TransferProgress::Complete {
        release_id: release_id.to_string(),
    });
    Ok(())
}
