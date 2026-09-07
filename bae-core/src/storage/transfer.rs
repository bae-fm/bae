//! Release storage transitions — moving a release between Local and Remote,
//! and pinning/unpinning a Remote release in coven's cache.
//!
//! coven owns the blob lifecycle; bae drives the user-facing transition (guards,
//! progress events) and calls into coven:
//!
//! - `make_releases_remote`: each selected release is independently admitted to
//!   coven's durable queue, so one release may start uploading while later
//!   releases are still being admitted and one refusal does not block its
//!   siblings. coven uploads each blob from its external (in-place) source, and
//!   on the last publishes `remote` true and drops the external refs.
//!   User-provided source files stay in place; host-provided local-store copies
//!   follow coven's cache retention policy. The cover rides along. The commit
//!   wakes subscribed release projections.
//! - `make_release_local`: `coven.make_local` materializes every blob back to a
//!   local file durability-first (release files to the chosen folder, the cover to
//!   coven's local store), then flips `remote` false, registers the external refs,
//!   and tombstones the cloud blobs — one atomic commit. A cancel before the
//!   commit rolls back the partial copies and leaves the release Remote.
//! - `pin_release` / `unpin_release`: pin/unpin a Remote release's blobs in
//!   coven's cache (`storage/pinned/` vs the evictable `storage/cache/`).
//!
//! Pinned-ness is coven cache state; bae stores no pin flag. A Remote release's
//! bytes live only in coven's cache — never a bae `storage/` path.

use std::future::Future;

use crate::album_detail::ReleaseStorageAction;
use crate::db::DbFile;
use crate::diagnostics::{LocalId, TelemetryEvent};
use crate::library::release_queue::RunningOp;
use crate::library::{DownloadTransferProgress, LibraryError, LibraryManager};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

type TransferResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
type ProgressTx = mpsc::UnboundedSender<TransferProgress>;

/// Read one release file's whole plaintext through coven's locality-aware read:
/// the user's own file (a Local user-provided blob's external ref), coven's local
/// store (a Local host-provided blob), or coven's cache/cloud for a Remote blob.
/// Checks `bytes.len() == file.file_size` so a short or zero read aborts the
/// caller before it trusts the bytes — a second check over coven's own, which
/// validates an external file's size and a torn cache file.
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

/// Progress updates emitted during a pin, unpin, or make-Local operation.
#[derive(Debug, Clone)]
pub enum TransferProgress {
    Started,
    Progress { progress: DownloadTransferProgress },
    Complete { release_id: String },
    Failed { release_id: String, error: String },
}

/// Pin, unpin, and make-Local service for releases.
pub struct TransferService {
    library_manager: LibraryManager,
}

impl TransferService {
    pub fn new(library_manager: LibraryManager) -> Self {
        Self { library_manager }
    }

    /// Pin a release and compose its progress drain, task completion, and abort
    /// control into the operation the serial queue owns.
    pub fn pin_release<Drive, DriveFuture, DriveOutput>(
        &self,
        release_id: String,
        drive: Drive,
    ) -> RunningOp<impl Future<Output = Result<DriveOutput, LibraryError>>>
    where
        Drive: FnOnce(mpsc::UnboundedReceiver<TransferProgress>) -> DriveFuture,
        DriveFuture: Future<Output = Result<DriveOutput, LibraryError>>,
    {
        let library_manager = self.library_manager.clone();
        let pinned_release_id = release_id.clone();
        let (progress, task) = spawn_transfer(
            ReleaseTransfer::new(
                self.library_manager.clone(),
                release_id,
                ReleaseStorageAction::Pin,
            ),
            move |tx| async move {
                library_manager
                    .pin_release_blobs_with_progress(&pinned_release_id, |progress| {
                        send_progress(&tx, TransferProgress::Progress { progress });
                    })
                    .await?;
                Ok(())
            },
        );
        let abort = task.abort_handle();
        let outcome = async move {
            let drained = drive(progress).await;
            match task.await {
                Ok(()) => drained,
                Err(join_error) if join_error.is_cancelled() => drained,
                Err(join_error) => Err(LibraryError::Storage(format!(
                    "pin task panicked: {join_error}"
                ))),
            }
        };
        RunningOp::new(abort, outcome)
    }

    /// Unpin a release: move its blobs from `storage/pinned/` to the evictable
    /// `storage/cache/`. Returns a receiver for progress updates.
    pub fn unpin_release(&self, release_id: String) -> mpsc::UnboundedReceiver<TransferProgress> {
        let library_manager = self.library_manager.clone();
        let unpinned_release_id = release_id.clone();
        let (rx, _) = spawn_transfer(
            ReleaseTransfer::new(
                self.library_manager.clone(),
                release_id,
                ReleaseStorageAction::Unpin,
            ),
            move |_tx| async move {
                library_manager
                    .unpin_release_blobs(&unpinned_release_id)
                    .await?;
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
        let library_manager = self.library_manager.clone();
        let localized_release_id = release_id.clone();
        let (rx, _) = spawn_transfer(
            ReleaseTransfer::new(
                self.library_manager.clone(),
                release_id,
                ReleaseStorageAction::MakeLocal,
            ),
            move |_tx| async move {
                tokio::fs::create_dir_all(std::path::Path::new(&new_path)).await?;

                // coven materializes each blob durability-first, flips the gate false,
                // registers the external refs, and tombstones the cloud blobs in one
                // atomic commit; a cancel before the commit is rolled back and surfaced
                // as Ok.
                library_manager
                    .coven_make_local(&localized_release_id, &new_path, &cancel)
                    .await?;
                Ok(())
            },
        );
        rx
    }
}

/// One release transfer in flight: which release, what is being done to it, and
/// the library it is guarded and recorded against.
///
/// Whoever runs a transfer holds one of these from the precondition check
/// through to the terminal record, so a completion or a failure can only ever
/// name the transfer that was started. Sits beside
/// [`crate::library::storage_transitions::StorageTransitions`], which tracks
/// which release is in a transition at all and how to cancel it; this owns what
/// one transfer checks, logs, and reports.
///
/// A pin, unpin, or make-Local is handed to [`spawn_transfer`], which drives it
/// on its own task and reports on a progress channel. A make-Remote has no
/// channel of its own — coven's outbox reports it — so its caller drives it and
/// calls [`Self::start`], [`Self::complete`], and [`Self::fail`] directly.
pub(crate) struct ReleaseTransfer {
    library_manager: LibraryManager,
    release_id: String,
    action: ReleaseStorageAction,
}

impl ReleaseTransfer {
    pub(crate) fn new(
        library_manager: LibraryManager,
        release_id: String,
        action: ReleaseStorageAction,
    ) -> Self {
        Self {
            library_manager,
            release_id,
            action,
        }
    }

    /// Guard the transfer's preconditions; returns the release's file count so
    /// the completion can report it.
    pub(crate) async fn start(&self) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        let release = self
            .library_manager
            .get_release_by_id(&self.release_id)
            .await?
            .ok_or("Release not found")?;
        if release.remote != action_expects_remote(self.action) {
            return Err(wrong_state_error(self.action).into());
        }
        if matches!(self.action, ReleaseStorageAction::MakeRemote)
            && !self.library_manager.has_cloud_home()
        {
            return Err("Cannot make a release remote without a cloud home".into());
        }

        let files = self
            .library_manager
            .get_files_for_release(&self.release_id)
            .await?;
        if files.is_empty() {
            return Err("Release has no files".into());
        }

        Ok(files.len() as u32)
    }

    pub(crate) fn complete(&self, file_count: u32) {
        info!(
            action = ?self.action,
            release_id = %self.release_id,
            "release transfer complete"
        );
        self.library_manager
            .record_telemetry(TelemetryEvent::StorageTransferCompleted {
                action: self.action,
                release_id: LocalId(self.release_id.clone()),
                file_count,
            });
    }

    pub(crate) fn fail(&self, failure: &dyn std::fmt::Display) {
        error!(
            action = ?self.action,
            release_id = %self.release_id,
            failure = %failure,
            "release transfer failed"
        );
        self.library_manager
            .record_telemetry(TelemetryEvent::StorageTransferFailed {
                action: self.action,
                release_id: LocalId(self.release_id.clone()),
            });
    }
}

/// Drive one release transfer on its own task: guard its preconditions, announce
/// its start, run `run`, and record whichever terminal state it reaches. Returns
/// the progress channel to drain and the task, so a caller that needs to join or
/// abort the transfer can.
fn spawn_transfer<Run, Fut>(
    transfer: ReleaseTransfer,
    run: Run,
) -> (
    mpsc::UnboundedReceiver<TransferProgress>,
    tokio::task::JoinHandle<()>,
)
where
    Run: FnOnce(ProgressTx) -> Fut + Send + 'static,
    Fut: Future<Output = TransferResult> + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let result = async {
            let file_count = transfer.start().await?;
            send_progress(&tx, TransferProgress::Started);
            info!(
                action = ?transfer.action,
                release_id = %transfer.release_id,
                file_count,
                "release transfer started"
            );
            run(tx.clone()).await?;
            transfer.complete(file_count);
            send_progress(
                &tx,
                TransferProgress::Complete {
                    release_id: transfer.release_id.clone(),
                },
            );
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;
        if let Err(e) = result {
            transfer.fail(&e);
            send_progress(
                &tx,
                TransferProgress::Failed {
                    release_id: transfer.release_id.clone(),
                    error: e.to_string(),
                },
            );
        }
    });

    (rx, task)
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

fn send_progress(tx: &ProgressTx, progress: TransferProgress) {
    if let Err(mpsc::error::SendError(progress)) = tx.send(progress) {
        debug!(?progress, "release transfer progress receiver dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{
        AppDiagnosticMetadata, DatadogDiagnosticsConfig, DiagnosticEvent, Diagnostics,
        RecordingTransport,
    };
    use std::sync::Arc;
    use tempfile::TempDir;

    /// A `LibraryManager` (over an empty test DB) whose diagnostics ship to a
    /// recording transport, so an emitted event can be read back off the wire.
    async fn manager_with_recording_transport() -> (
        LibraryManager,
        Diagnostics,
        Arc<RecordingTransport>,
        TempDir,
    ) {
        let home = TempDir::new().unwrap();
        let db_path = home.path().join("transfer-test.db");
        let database = crate::db::Database::new_test(
            db_path.to_str().unwrap(),
            Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();

        let transport = Arc::new(RecordingTransport::new(vec![]));
        let config = DatadogDiagnosticsConfig {
            datadog_site: "datadoghq.com".to_string(),
            client_token: "client-token".to_string(),
            source: "test".to_string(),
            app: AppDiagnosticMetadata {
                service: "bae".to_string(),
                environment: "test".to_string(),
                app_version: "1.2.3".to_string(),
                edition: "bae".to_string(),
                git_commit: "abc123".to_string(),
            },
        };
        let diagnostics = Diagnostics::with_transport(
            config,
            Arc::new(coven::SystemClock),
            Arc::new(coven::SequentialIdProvider::new("request-id")),
            transport.clone(),
        )
        .expect("diagnostics starts");

        let library_id = "transfer-test".to_string();
        let config = crate::config::Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            coven::StoreDir::new(home.path().join("library")),
            "Test Library".to_string(),
        );
        crate::config::install_test_keyring();
        let manager = LibraryManager::new(
            database,
            Arc::new(crate::config::ConfigHandle::new(config)),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(),
        );
        (manager, diagnostics, transport, home)
    }

    async fn shipped_events(
        diagnostics: &Diagnostics,
        transport: &RecordingTransport,
    ) -> Vec<DiagnosticEvent> {
        diagnostics.flush().await.expect("flush succeeds");
        transport
            .requests()
            .iter()
            .flat_map(|request| {
                serde_json::from_slice::<Vec<DiagnosticEvent>>(&request.body).unwrap()
            })
            .collect()
    }

    /// Pinning a release that isn't in the library fails the transfer's
    /// precondition check, which ships `storage_transfer_failed` — an Error-level
    /// event — through the real diagnostics pipeline to the wire.
    #[tokio::test]
    async fn a_failed_transfer_ships_storage_transfer_failed() {
        let (manager, diagnostics, transport, _home) = manager_with_recording_transport().await;
        let transfer = TransferService::new(manager);

        // No such release, so `ReleaseTransfer::start` errors before any blob work.
        let running =
            transfer.pin_release("missing-release".to_string(), |mut progress| async move {
                while progress.recv().await.is_some() {}
                Ok(())
            });
        running.finish().await.expect("transfer task completes");

        let events = shipped_events(&diagnostics, &transport).await;
        let failed = events
            .iter()
            .find(|e| e.name == "storage_transfer_failed")
            .expect("a failed transfer ships storage_transfer_failed");
        assert_eq!(failed.fields["action"], serde_json::json!("pin"));
        assert_eq!(
            failed.fields["release_id"],
            serde_json::json!("missing-release")
        );
    }

    #[tokio::test]
    async fn a_failed_move_to_cloud_batch_ships_storage_transfer_failed() {
        let (manager, diagnostics, transport, _home) = manager_with_recording_transport().await;

        let outcome = manager
            .make_releases_remote(&["missing-release".to_string()], false)
            .await
            .expect("a per-release refusal is returned as a batch outcome");
        let crate::library::MakeReleasesRemoteOutcome::Partial {
            receipt: None,
            failure,
        } = outcome
        else {
            panic!("the missing release must be refused without a receipt");
        };
        assert_eq!(failure.release_ids, vec!["missing-release"]);

        let events = shipped_events(&diagnostics, &transport).await;
        let failed = events
            .iter()
            .find(|event| event.name == "storage_transfer_failed")
            .expect("a failed batch ships storage_transfer_failed");
        assert_eq!(failed.fields["action"], serde_json::json!("make_remote"));
        assert_eq!(
            failed.fields["release_id"],
            serde_json::json!("missing-release")
        );
    }
}
