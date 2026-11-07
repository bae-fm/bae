//! Release file attachment and offline pins.
//!
//! Every release's rows sync. Making a release Remote attaches all of its local
//! files in one write; coven then uploads their bytes. Making it local again
//! runs in the download queue (see `locality.rs`).
//!
//! Pins keep attached files available offline. Coven owns their cache and pin
//! state; bae owns the release-level progress and cancellation controls.

use std::future::Future;

use crate::{LibraryError, LibraryManager};
use bae_diagnostics::{LocalId, TelemetryEvent};
use bae_transfers::DownloadTransferProgress;
use bae_transfers::Finished;
use bae_transfers::RunningOp;
use bae_types::release_storage::ReleaseStorageAction;
use bae_util::task_failure::{caught, TaskFailed};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

type ProgressTx = mpsc::UnboundedSender<TransferProgress>;

/// Progress updates emitted during a pin. A
/// failure carries the typed error, so the caller reports why it failed —
/// the cloud out of reach, its credentials, or this device's own store.
#[derive(Debug)]
pub enum TransferProgress {
    Progress { progress: DownloadTransferProgress },
    Complete { release_id: String },
    Failed { release_id: String, error: LibraryError },
}

/// The progress one pin reports, until it says how
/// it ended, from the library manager that started it.
pub struct TransferProgressFeed(mpsc::UnboundedReceiver<TransferProgress>);

impl TransferProgressFeed {
    /// The transfer's next report, or `None` once it ended without one.
    pub async fn recv(&mut self) -> Option<TransferProgress> {
        self.0.recv().await
    }

    /// A feed over `progress`, for a test that reports a transfer itself.
    #[cfg(test)]
    pub(crate) fn of_test_channel(progress: mpsc::UnboundedReceiver<TransferProgress>) -> Self {
        Self(progress)
    }
}

impl LibraryManager {
    /// Pin a release and compose its progress drain, task completion, and abort
    /// control into the operation the serial queue owns.
    ///
    /// The pin ends `entry` in the write that pins the release's files, and
    /// the operation's outcome carries that proof.
    pub fn start_pin_transfer<Drive, DriveFuture>(
        &self,
        release_id: String,
        entry: bae_transfers::ActiveEntry<bae_transfers::DownloadRequest>,
        drive: Drive,
    ) -> RunningOp<impl Future<Output = Result<Finished, LibraryError>>>
    where
        Drive: FnOnce(TransferProgressFeed) -> DriveFuture,
        DriveFuture: Future<Output = Result<(), LibraryError>>,
    {
        let library_manager = self.clone();
        let pinned_release_id = release_id.clone();
        let (progress, task) =
            spawn_transfer(ReleaseTransfer::new(self.clone(), release_id), move |tx| async move {
                library_manager
                    .pin_release_files_with_progress(&pinned_release_id, entry, |progress| {
                        send_progress(&tx, TransferProgress::Progress { progress });
                    })
                    .await
            });
        let abort = task.abort_handle();
        let manager = self.clone();
        let outcome = async move {
            let drained = drive(TransferProgressFeed(progress)).await;
            match task.await {
                Ok(Some(finished)) => drained.map(|()| finished),
                // The transfer failed or was aborted, which the drain says.
                Ok(None) => drained.and_then(|()| {
                    Err(LibraryError::Storage(
                        "the pin ended without finishing its queue entry".to_string(),
                    ))
                }),
                Err(join_error) if join_error.is_cancelled() => drained
                    .and_then(|()| Err(LibraryError::Storage("the pin was cancelled".to_string()))),
                // The transfer catches its own work's panic, so this is a
                // panic in recording how it ended.
                Err(join_error) => {
                    Err(manager.bug(TaskFailed::of_join("pin a release", join_error)).into())
                }
            }
        };
        RunningOp::new(abort, outcome)
    }
}

/// One pin in flight: which release, and the library it is guarded and
/// recorded against.
///
/// Whoever runs a transfer holds one of these from the precondition check
/// through to the terminal record, so a completion or a failure can only ever
/// name the transfer that was started. Which release is pinning is its
/// download queue entry; this owns what the pin checks, logs, and reports.
///
/// A pin is handed to [`spawn_transfer`], which drives it on its own task and
/// reports on a progress channel.
pub(crate) struct ReleaseTransfer {
    library_manager: LibraryManager,
    release_id: String,
}

impl ReleaseTransfer {
    pub(crate) fn new(library_manager: LibraryManager, release_id: String) -> Self {
        Self { library_manager, release_id }
    }

    /// Guard the transfer's preconditions; returns the release's file count so
    /// the completion can report it.
    pub(crate) async fn start(&self) -> Result<u32, LibraryError> {
        self.library_manager.get_release_by_id(&self.release_id).await?.ok_or_else(|| {
            LibraryError::release_missing(&self.release_id, crate::manager::ReleaseWork::Transfer)
        })?;
        let files = self.library_manager.get_files_for_release(&self.release_id).await?;
        if files.is_empty() {
            return Err(LibraryError::Storage("Release has no files".to_string()));
        }

        // A pin acts on files stored in the cloud.
        let mut ready = true;
        for file in &files {
            let file = self.library_manager.release_file_ref(&file.id).await?;
            ready &= matches!(file.place(), coven::FilePlace::Cloud { .. });
        }
        if !ready {
            return Err(LibraryError::Storage("Cannot pin a local release".to_string()));
        }

        Ok(files.len() as u32)
    }

    pub(crate) fn complete(&self, file_count: u32) {
        info!(
            release_id = %self.release_id,
            "release transfer complete"
        );
        self.library_manager.record_telemetry(TelemetryEvent::StorageTransferCompleted {
            action: ReleaseStorageAction::Pin,
            release_id: LocalId(self.release_id.clone()),
            file_count,
        });
    }

    pub(crate) fn fail(&self, failure: &dyn std::fmt::Display) {
        error!(
            release_id = %self.release_id,
            failure = %failure,
            "release transfer failed"
        );
        self.library_manager.record_telemetry(TelemetryEvent::StorageTransferFailed {
            action: ReleaseStorageAction::Pin,
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
fn spawn_transfer<Run, Fut, Done>(
    transfer: ReleaseTransfer,
    run: Run,
) -> (mpsc::UnboundedReceiver<TransferProgress>, tokio::task::JoinHandle<Option<Done>>)
where
    Run: FnOnce(ProgressTx) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Done, LibraryError>> + Send + 'static,
    Done: Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let result = caught("pin a release", async {
            let file_count = transfer.start().await?;
            info!(
                release_id = %transfer.release_id,
                file_count,
                "release transfer started"
            );
            let done = run(tx.clone()).await?;
            transfer.complete(file_count);
            send_progress(
                &tx,
                TransferProgress::Complete { release_id: transfer.release_id.clone() },
            );
            Ok::<_, LibraryError>(done)
        })
        .await
        .unwrap_or_else(|bug| Err(transfer.library_manager.panicked(bug).into()));
        match result {
            Ok(done) => Some(done),
            Err(error) => {
                transfer.fail(&error);
                send_progress(
                    &tx,
                    TransferProgress::Failed { release_id: transfer.release_id.clone(), error },
                );
                None
            }
        }
    });

    (rx, task)
}

fn send_progress(tx: &ProgressTx, progress: TransferProgress) {
    if let Err(mpsc::error::SendError(progress)) = tx.send(progress) {
        debug!(?progress, "release transfer progress receiver dropped");
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
