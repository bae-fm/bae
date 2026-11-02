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

use crate::library::queued_releases::RunningOp;
use crate::library::{DownloadTransferProgress, LibraryError, LibraryManager};
use bae_diagnostics::{LocalId, TelemetryEvent};
use bae_types::release_storage::ReleaseStorageAction;
use bae_util::task_failure::{caught, TaskFailed};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

type TransferResult = Result<(), LibraryError>;
type ProgressTx = mpsc::UnboundedSender<TransferProgress>;

/// Progress updates emitted during a pin, unpin, or audio detachment operation. A
/// failure carries the typed error, so the caller reports why it failed —
/// the cloud out of reach, its credentials, or this device's own store.
#[derive(Debug)]
pub enum TransferProgress {
    Progress { progress: DownloadTransferProgress },
    Complete { release_id: String },
    Failed { release_id: String, error: LibraryError },
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
        let manager = self.library_manager.clone();
        let outcome = async move {
            let drained = drive(progress).await;
            match task.await {
                Ok(()) => drained,
                Err(join_error) if join_error.is_cancelled() => drained,
                // The transfer catches its own work's panic, so this is a
                // panic in recording how it ended.
                Err(join_error) => {
                    Err(manager.bug(TaskFailed::of_join("pin a release", join_error)).into())
                }
            }
        };
        RunningOp::new(abort, outcome)
    }

    /// Unpin a release: move its files from the pinned cache to the evictable
    /// cache, as the release's transition. Returns a receiver for progress
    /// updates.
    pub fn unpin_release(&self, release_id: String) -> mpsc::UnboundedReceiver<TransferProgress> {
        let library_manager = self.library_manager.clone();
        let unpinned_release_id = release_id.clone();
        let (rx, _) = spawn_transfer(
            ReleaseTransfer::new(
                self.library_manager.clone(),
                release_id,
                ReleaseStorageAction::Unpin,
            ),
            move |_tx| async move { library_manager.unpin_release_files(&unpinned_release_id).await },
        );
        rx
    }

    /// Make a Remote release Local, as the release's transition: coven
    /// downloads its files into `new_path` and switches them to this device
    /// only in one write. `cancel` stops it; the transfer's task holds
    /// `registration`, which makes `cancel` reachable, until it ends.
    /// Returns a receiver for progress updates.
    pub(crate) fn detach_release_audio(
        &self,
        release_id: String,
        new_path: std::path::PathBuf,
        cancel: crate::library::CancellationToken,
        registration: crate::library::detach_cancels::DetachRegistration,
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
                let _registration = registration;
                // coven removes every copy and folder it made if the call
                // fails or is cancelled before its write.
                library_manager
                    .detach_release_audio_into(&localized_release_id, &new_path, &cancel)
                    .await
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
/// name the transfer that was started. Which release is in a transition at
/// all is its `release_storage_action` row, or for a pin its download queue
/// entry; this owns what one transfer checks, logs, and reports.
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
        Self { library_manager, release_id, action }
    }

    /// Guard the transfer's preconditions; returns the release's file count so
    /// the completion can report it.
    pub(crate) async fn start(&self) -> Result<u32, LibraryError> {
        self.library_manager.get_release_by_id(&self.release_id).await?.ok_or_else(|| {
            LibraryError::TrackMapping(format!("Release not found: {}", self.release_id))
        })?;
        if matches!(self.action, ReleaseStorageAction::Upload)
            && !self.library_manager.is_sync_configured().await?
        {
            return Err(coven::SyncError::NoStorage.into());
        }

        let files = self.library_manager.get_files_for_release(&self.release_id).await?;
        if files.is_empty() {
            return Err(LibraryError::Storage("Release has no files".to_string()));
        }

        // An upload moves files on this device only, none of them moving
        // already; pin, unpin and detach act on files stored in the cloud.
        let here = self.library_manager.device_id();
        let mut ready = match self.action {
            ReleaseStorageAction::Upload => {
                !self.library_manager.has_pending_uploads_for_release(&self.release_id).await?
            }
            ReleaseStorageAction::Pin
            | ReleaseStorageAction::Unpin
            | ReleaseStorageAction::DetachAudio => true,
        };
        for file in &files {
            let file = self.library_manager.release_file_ref(&file.id).await?;
            ready &= match (self.action, file.place()) {
                (ReleaseStorageAction::Upload, coven::FilePlace::DeviceOnly { device }) => {
                    device == here
                }
                (ReleaseStorageAction::Upload, coven::FilePlace::Cloud { .. }) => false,
                (
                    ReleaseStorageAction::Pin
                    | ReleaseStorageAction::Unpin
                    | ReleaseStorageAction::DetachAudio,
                    place,
                ) => matches!(place, coven::FilePlace::Cloud { .. }),
            };
        }
        if !ready {
            return Err(LibraryError::Storage(wrong_state_error(self.action).to_string()));
        }

        Ok(files.len() as u32)
    }

    pub(crate) fn complete(&self, file_count: u32) {
        info!(
            action = ?self.action,
            release_id = %self.release_id,
            "release transfer complete"
        );
        self.library_manager.record_telemetry(TelemetryEvent::StorageTransferCompleted {
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
        self.library_manager.record_telemetry(TelemetryEvent::StorageTransferFailed {
            action: self.action,
            release_id: LocalId(self.release_id.clone()),
        });
    }
}

/// Drive one release transfer on its own task: guard its preconditions, run
/// `run`, and record whichever terminal state it reaches. A
/// panic in the transfer is reported as a bug and ends it as failed, on the
/// progress channel like any failure, so a caller that drains the channel
/// alone still hears how it ended. Returns the progress channel to drain and
/// the task, so a caller that needs to join or abort the transfer can.
fn spawn_transfer<Run, Fut>(
    transfer: ReleaseTransfer,
    run: Run,
) -> (mpsc::UnboundedReceiver<TransferProgress>, tokio::task::JoinHandle<()>)
where
    Run: FnOnce(ProgressTx) -> Fut + Send + 'static,
    Fut: Future<Output = TransferResult> + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let result = caught(transfer_operation(transfer.action), async {
            let file_count = transfer.start().await?;
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
                TransferProgress::Complete { release_id: transfer.release_id.clone() },
            );
            Ok::<_, LibraryError>(())
        })
        .await
        .unwrap_or_else(|failed| Err(transfer.library_manager.bug(failed).into()));
        if let Err(error) = result {
            transfer.fail(&error);
            send_progress(
                &tx,
                TransferProgress::Failed { release_id: transfer.release_id.clone(), error },
            );
        }
    });

    (rx, task)
}

/// What a transfer of `action` is called where its panic is reported.
fn transfer_operation(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "pin a release",
        ReleaseStorageAction::Unpin => "unpin a release",
        ReleaseStorageAction::Upload => "upload a release",
        ReleaseStorageAction::DetachAudio => "make a release local",
    }
}

fn wrong_state_error(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Cannot pin a local release",
        ReleaseStorageAction::Unpin => "Cannot unpin a local release",
        ReleaseStorageAction::Upload => {
            "Release is not on this device alone, or is moving to the cloud already"
        }
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
    use bae_diagnostics::{
        AppDiagnosticMetadata, DatadogDiagnosticsConfig, DiagnosticEvent, Diagnostics,
        RecordingTransport,
    };
    use std::sync::Arc;
    use tempfile::TempDir;

    /// A `LibraryManager` (over an empty test DB) whose diagnostics ship to a
    /// recording transport, so an emitted event can be read back off the wire.
    async fn manager_with_recording_transport(
    ) -> (LibraryManager, Diagnostics, Arc<RecordingTransport>, TempDir) {
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
        let diagnostics = Diagnostics::recording_to(
            config,
            Arc::new(coven::SystemClock),
            Arc::new(coven::SequentialIds::new()),
            transport.clone(),
        )
        .expect("diagnostics starts");

        let library = crate::app::test_library::TestLibrary::open_with(
            database,
            crate::app_dir::AppDir::under_home(home.path()).unwrap(),
            bae_catalogs::providers::Providers::offline(),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidIds),
            diagnostics.clone(),
        )
        .await;
        let manager = LibraryManager::clone(&library);
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
        assert_eq!(failed.fields["release_id"], serde_json::json!("missing-release"));
    }

    #[tokio::test]
    async fn a_failed_upload_batch_ships_storage_transfer_failed() {
        let (manager, diagnostics, transport, _home) = manager_with_recording_transport().await;

        let outcome = manager
            .upload_releases(&["missing-release".to_string()])
            .await
            .expect("a per-release refusal is returned as a batch outcome");
        let crate::library::UploadReleasesOutcome::Partial { receipt: None, failure } = outcome
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
        assert_eq!(failed.fields["release_id"], serde_json::json!("missing-release"));
    }
}
