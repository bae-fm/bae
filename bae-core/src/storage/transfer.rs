//! Release file attachment, verified local copies, and offline pins.
//!
//! Every release's rows sync. Making a release Remote attaches all of its local
//! files in one write; coven then uploads their bytes. Making it Local first
//! writes and verifies every destination, then atomically clears the attachments
//! and records this device's paths. A failure removes the copies it created.
//! Covers remain attached throughout both transitions.
//!
//! Pins keep attached files available offline. Coven owns their cache and pin
//! state; bae owns the release-level progress and cancellation controls.

use std::future::Future;

use crate::album_detail::ReleaseStorageAction;
use crate::db::DbFile;
use crate::diagnostics::{LocalId, TelemetryEvent};
use crate::library::release_queue::RunningOp;
use crate::library::{DownloadTransferProgress, LibraryError, LibraryManager};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

type TransferResult = Result<(), LibraryError>;
type ProgressTx = mpsc::UnboundedSender<TransferProgress>;

/// Read a release file through its device-local source or coven attachment,
/// and require the byte count its row declares before handing it to a decoder.
pub async fn read_release_file_bytes(
    file: &DbFile,
    mgr: &LibraryManager,
) -> Result<Vec<u8>, LibraryError> {
    let bytes = mgr.read_release_file(file).await?;
    if bytes.len() as i64 != file.file_size {
        return Err(LibraryError::Storage(format!(
            "File {} short read: got {} bytes, expected {}",
            file.id,
            bytes.len(),
            file.file_size
        )));
    }
    Ok(bytes)
}

/// Progress updates emitted during a pin, unpin, or audio detachment operation. A
/// failure carries the typed error, so the caller reports why it failed —
/// the cloud out of reach, its credentials, or this device's own store.
#[derive(Debug)]
pub enum TransferProgress {
    Started,
    Progress {
        progress: DownloadTransferProgress,
    },
    Complete {
        release_id: String,
    },
    Failed {
        release_id: String,
        error: LibraryError,
    },
}

/// Pin, unpin, and audio detachment service for releases.
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
                    .pin_release_files_with_progress(&pinned_release_id, |progress| {
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

    /// Unpin a release: move its files from the pinned cache to the evictable
    /// cache. Returns a receiver for progress updates.
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
                    .unpin_release_files(&unpinned_release_id)
                    .await?;
                Ok(())
            },
        );
        rx
    }

    /// Make a Remote release Local: copy its files to `new_path` and drop the
    /// cloud copies (coven owns the durability-first ordering). Returns a receiver
    /// for progress updates.
    pub fn detach_release_audio(
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
                ReleaseStorageAction::DetachAudio,
            ),
            move |_tx| async move {
                // The copy operation owns every created file and directory;
                // it removes them if verification or the final write fails.
                library_manager
                    .detach_release_audio_into(&localized_release_id, &new_path, &cancel)
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
/// A pin, unpin, or audio detachment is handed to [`spawn_transfer`], which drives it
/// on its own task and reports on a progress channel. A upload has no
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
    pub(crate) async fn start(&self) -> Result<u32, LibraryError> {
        self.library_manager
            .get_release_by_id(&self.release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("Release not found: {}", self.release_id))
            })?;
        if matches!(self.action, ReleaseStorageAction::Upload)
            && !self.library_manager.is_sync_configured().await?
        {
            return Err(coven::SyncError::NoStorage.into());
        }

        let files = self
            .library_manager
            .get_files_for_release(&self.release_id)
            .await?;
        if files.is_empty() {
            return Err(LibraryError::Storage("Release has no files".to_string()));
        }

        if files.iter().all(|file| file.attached) != action_expects_remote(self.action) {
            return Err(LibraryError::Storage(
                wrong_state_error(self.action).to_string(),
            ));
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
            Ok::<_, LibraryError>(())
        }
        .await;
        if let Err(error) = result {
            transfer.fail(&error);
            send_progress(
                &tx,
                TransferProgress::Failed {
                    release_id: transfer.release_id.clone(),
                    error,
                },
            );
        }
    });

    (rx, task)
}

fn action_expects_remote(action: ReleaseStorageAction) -> bool {
    matches!(
        action,
        ReleaseStorageAction::Pin | ReleaseStorageAction::Unpin | ReleaseStorageAction::DetachAudio
    )
}

fn wrong_state_error(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Cannot pin a local release",
        ReleaseStorageAction::Unpin => "Cannot unpin a local release",
        ReleaseStorageAction::Upload => "Release is already remote",
        ReleaseStorageAction::DetachAudio => "Release is already local",
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
        let database =
            crate::db::Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
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
            Arc::new(coven::SequentialIds::new()),
            transport.clone(),
        )
        .expect("diagnostics starts");

        let config = crate::config::Config::with_defaults(
            database.store_id(),
            home.path().join("preferences"),
        );

        let manager = LibraryManager::new(
            database,
            crate::config::AppDir::under_home(home.path()).unwrap(),
            Arc::new(crate::config::ConfigHandle::new(config)),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidIds),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
            crate::import::cover_art::RemoteImageCache::for_test(
                crate::util::http::Http::for_test(),
            ),
            crate::providers::Providers::offline(),
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

        // No such release, so `ReleaseTransfer::start` errors before any file work.
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
    async fn a_failed_upload_batch_ships_storage_transfer_failed() {
        let (manager, diagnostics, transport, _home) = manager_with_recording_transport().await;

        let outcome = manager
            .upload_releases(&["missing-release".to_string()])
            .await
            .expect("a per-release refusal is returned as a batch outcome");
        let crate::library::UploadReleasesOutcome::Partial {
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
        assert_eq!(failed.fields["action"], serde_json::json!("upload"));
        assert_eq!(
            failed.fields["release_id"],
            serde_json::json!("missing-release")
        );
    }
}
