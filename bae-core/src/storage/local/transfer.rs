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

use std::future::Future;

use crate::album_detail::ReleaseStorageAction;
use crate::db::DbFile;
use crate::library::{DownloadTransferProgress, LibraryManager};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

type TransferResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type ProgressTx = mpsc::UnboundedSender<TransferProgress>;

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
    Started,
    /// Operation reported determinate transfer progress.
    Progress { progress: DownloadTransferProgress },
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
    /// cancels.
    pub fn pin_release_task(
        &self,
        release_id: String,
    ) -> (
        mpsc::UnboundedReceiver<TransferProgress>,
        tokio::task::JoinHandle<()>,
    ) {
        self.spawn_transfer(
            release_id,
            ReleaseStorageAction::Pin,
            |release_id, library_manager, tx| async move {
                library_manager
                    .pin_release_blobs_with_progress(&release_id, |progress| {
                        send_progress(&tx, TransferProgress::Progress { progress });
                    })
                    .await?;
                Ok(())
            },
        )
    }

    /// Unpin a release: move its blobs from `storage/pinned/` to the evictable
    /// `storage/cache/`. Returns a receiver for progress updates.
    pub fn unpin_release(&self, release_id: String) -> mpsc::UnboundedReceiver<TransferProgress> {
        let (rx, _) = self.spawn_transfer(
            release_id,
            ReleaseStorageAction::Unpin,
            |release_id, library_manager, _tx| async move {
                library_manager.unpin_release_blobs(&release_id).await?;
                Ok(())
            },
        );
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
        let (rx, _) = self.spawn_transfer(
            release_id,
            ReleaseStorageAction::MakeRemote,
            move |release_id, library_manager, _tx| async move {
                // The release reaches `remote = true` only when coven's upload drain
                // flips it after the last upload lands. That drain runs from the sync
                // loop, so the loop must be running — otherwise the uploads sit forever
                // and the release stays Local with no error. A configured cloud home
                // isn't enough; the loop must be draining.
                if !library_manager.is_sync_ready() {
                    return Err(
                        "Cannot make a release remote while sync isn't running — it would never finish \
                                 uploading and would stay local"
                            .into(),
                    );
                }

                // Hand the whole transition to coven: it verifies every external source,
                // enqueues the uploads, and kicks the loop. Per-file upload progress flows
                // through the outbox snapshot; the gate flip + source delete fire from
                // coven's drain, surfaced via the observer's `on_root_made_remote`.
                library_manager.coven_make_remote(&release_id, pin).await?;
                Ok(())
            },
        );
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
        let (rx, _) = self.spawn_transfer(
            release_id,
            ReleaseStorageAction::MakeLocal,
            move |release_id, library_manager, _tx| async move {
                tokio::fs::create_dir_all(std::path::Path::new(&new_path)).await?;

                // coven materializes each blob durability-first, flips the gate false,
                // registers the external refs, and tombstones the cloud blobs in one
                // atomic commit; a cancel before the commit is rolled back and surfaced
                // as Ok.
                library_manager
                    .coven_make_local(&release_id, &new_path, &cancel)
                    .await?;
                Ok(())
            },
        );
        rx
    }

    fn spawn_transfer<Run, Fut>(
        &self,
        release_id: String,
        action: ReleaseStorageAction,
        run: Run,
    ) -> (
        mpsc::UnboundedReceiver<TransferProgress>,
        tokio::task::JoinHandle<()>,
    )
    where
        Run: FnOnce(String, LibraryManager, ProgressTx) -> Fut + Send + 'static,
        Fut: Future<Output = TransferResult> + Send + 'static,
    {
        let (tx, rx) = mpsc::unbounded_channel();
        let library_manager = self.library_manager.clone();

        let task = tokio::spawn(async move {
            let label = transfer_label(action);
            let result =
                run_transfer(release_id.clone(), action, library_manager, tx.clone(), run).await;
            if let Err(e) = result {
                error!("{} failed for release {}: {}", label, release_id, e);
                send_progress(
                    &tx,
                    TransferProgress::Failed {
                        release_id,
                        error: e.to_string(),
                    },
                );
            }
        });

        (rx, task)
    }
}

async fn run_transfer<Fut>(
    release_id: String,
    action: ReleaseStorageAction,
    library_manager: LibraryManager,
    tx: ProgressTx,
    run: impl FnOnce(String, LibraryManager, ProgressTx) -> Fut,
) -> TransferResult
where
    Fut: Future<Output = TransferResult> + Send,
{
    start_transfer(&release_id, action, &library_manager, &tx).await?;
    run(release_id.clone(), library_manager, tx.clone()).await?;
    info!(
        action = ?action,
        release_id = %release_id,
        "release transfer complete"
    );
    send_progress(&tx, TransferProgress::Complete { release_id });
    Ok(())
}

async fn start_transfer(
    release_id: &str,
    action: ReleaseStorageAction,
    library_manager: &LibraryManager,
    tx: &ProgressTx,
) -> TransferResult {
    let release = library_manager
        .get_release_by_id(release_id)
        .await?
        .ok_or("Release not found")?;
    if release.remote != action_expects_remote(action) {
        return Err(wrong_state_error(action).into());
    }
    if matches!(action, ReleaseStorageAction::MakeRemote) && !library_manager.has_cloud_home() {
        return Err("Cannot make a release remote without a cloud home".into());
    }

    let files = library_manager.get_files_for_release(release_id).await?;
    if files.is_empty() {
        return Err("Release has no files".into());
    }

    send_progress(tx, TransferProgress::Started);
    info!(
        action = ?action,
        release_id = %release_id,
        file_count = files.len(),
        "release transfer started"
    );
    Ok(())
}

fn action_expects_remote(action: ReleaseStorageAction) -> bool {
    matches!(
        action,
        ReleaseStorageAction::Pin | ReleaseStorageAction::Unpin | ReleaseStorageAction::MakeLocal
    )
}

fn wrong_state_error(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Cannot pin a local release",
        ReleaseStorageAction::Unpin => "Cannot unpin a local release",
        ReleaseStorageAction::MakeRemote => "Release is already remote",
        ReleaseStorageAction::MakeLocal => "Release is already local",
    }
}

fn transfer_label(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "pin",
        ReleaseStorageAction::Unpin => "unpin",
        ReleaseStorageAction::MakeRemote => "make-remote",
        ReleaseStorageAction::MakeLocal => "make-local",
    }
}

fn send_progress(tx: &ProgressTx, progress: TransferProgress) {
    if let Err(mpsc::error::SendError(progress)) = tx.send(progress) {
        debug!(?progress, "release transfer progress receiver dropped");
    }
}
