//! Release file attachment and offline pins.
//!
//! Every release's rows sync. Making a release Remote attaches all of its local
//! files in one write; coven then uploads their bytes. Making it local again
//! runs in the download queue (see `locality.rs`).
//!
//! Pins keep attached files available offline. Coven owns their cache and pin
//! state; bae owns the release-level progress and cancellation controls.

use std::future::Future;

use crate::manager::coven_files::total_bytes;
use crate::{LibraryError, LibraryManager};
use bae_diagnostics::{LocalId, TelemetryEvent, Unfinished};
use bae_records::DbFile;
use bae_transfers::DownloadTransferProgress;
use bae_transfers::Finished;
use bae_transfers::RunningOp;
use bae_types::release_storage::ReleaseStorageAction;
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
    /// control into the operation the serial queue owns, with the progress the
    /// pin starts from: none of the total it counts down.
    ///
    /// The pin reads the release's files once, here, for its preconditions,
    /// its progress and its work. A release that fails the preconditions
    /// starts no pin: its failure is recorded and returned.
    ///
    /// The pin ends `entry` in the write that pins the release's files, and
    /// the operation's outcome carries that proof.
    pub async fn start_pin_transfer<Drive, DriveFuture>(
        &self,
        release_id: String,
        entry: bae_transfers::ActiveEntry<bae_transfers::DownloadRequest>,
        drive: Drive,
    ) -> Result<
        (DownloadTransferProgress, RunningOp<impl Future<Output = Result<Finished, LibraryError>>>),
        LibraryError,
    >
    where
        Drive: FnOnce(TransferProgressFeed) -> DriveFuture,
        DriveFuture: Future<Output = Result<(), LibraryError>>,
    {
        let files = self.find_files_to_pin(&release_id).await?;
        let transfer = ReleaseTransfer::new(self.clone(), release_id.clone());
        let plan = transfer.start(files).inspect_err(|error| transfer.fail(error))?;
        let initial = plan.initial.clone();
        let library_manager = self.clone();
        let pinned_release_id = release_id.clone();
        let file_count = u32::try_from(plan.files.len()).expect("a release's files fit a u32");
        let (progress, running) = spawn_transfer(transfer, file_count, move |tx| async move {
            library_manager
                .pin_release_files_with_progress(&pinned_release_id, &plan, entry, |progress| {
                    send_progress(&tx, TransferProgress::Progress { progress })
                })
                .await
        });
        let running = running.then(|ended| async move {
            let drained = drive(TransferProgressFeed(progress)).await;
            match ended.await {
                Ok(Some(finished)) => drained.map(|()| finished),
                // The transfer failed, which the drain says.
                Ok(None) => drained.and_then(|()| {
                    Err(LibraryError::Storage(
                        "the pin ended without finishing its queue entry".to_string(),
                    ))
                }),
                Err(Unfinished::Stopped) => drained
                    .and_then(|()| Err(LibraryError::Storage("the pin was cancelled".to_string()))),
                // A panic in recording how the transfer ended, reported as it
                // happened.
                Err(Unfinished::Panicked(bug)) => Err(bug.into()),
            }
        });
        Ok((initial, running))
    }
}

/// What a pin that passed its preconditions works from.
pub(crate) struct PinPlan {
    /// The release's files, which the pin keeps together.
    pub(crate) files: Vec<DbFile>,
    /// The progress the pin starts from: none of the bytes of its files.
    pub(crate) initial: DownloadTransferProgress,
}

/// One pin in flight: which release, and the library it is recorded
/// against.
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

    /// Guard the transfer's preconditions over the release's `files`, `None`
    /// when the library holds no such release; returns the pin they plan.
    pub(crate) fn start(&self, files: Option<Vec<DbFile>>) -> Result<PinPlan, LibraryError> {
        let files = files.ok_or_else(|| {
            LibraryError::release_missing(&self.release_id, crate::manager::ReleaseWork::Transfer)
        })?;
        if files.is_empty() {
            return Err(LibraryError::Storage("Release has no files".to_string()));
        }
        // coven's `prepare_pin` refuses a file that is not in the cloud.
        let total = total_bytes(&self.release_id, &files)?;
        Ok(PinPlan { initial: DownloadTransferProgress::new(&self.release_id, 0, total)?, files })
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

/// Drive one release transfer of `file_count` files, whose preconditions
/// passed, on its own task: run `run` and record whichever terminal state it
/// reaches. A
/// panic in the transfer is reported as a bug and ends it as failed, on the
/// progress channel like any failure, so a caller that drains the channel
/// alone still hears how it ended. Returns the progress channel to drain and
/// the running transfer, whose outcome is how it ended.
fn spawn_transfer<Run, Fut, Done>(
    transfer: ReleaseTransfer,
    file_count: u32,
    run: Run,
) -> (
    mpsc::UnboundedReceiver<TransferProgress>,
    RunningOp<impl Future<Output = Result<Option<Done>, Unfinished>> + Send + 'static>,
)
where
    Run: FnOnce(ProgressTx) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Done, LibraryError>> + Send + 'static,
    Done: Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    // The task runs the transfer and `end` records how it ended: one
    // transfer, held by both.
    let transfer = std::sync::Arc::new(transfer);
    let ended_transfer = transfer.clone();
    let manager = transfer.library_manager.clone();
    let progress = tx.clone();
    let running = manager.spawn_ending(
        "pin a release",
        async move {
            info!(
                release_id = %transfer.release_id,
                file_count,
                "release transfer started"
            );
            let done = run(progress.clone()).await?;
            transfer.complete(file_count);
            send_progress(
                &progress,
                TransferProgress::Complete { release_id: transfer.release_id.clone() },
            );
            Ok::<_, LibraryError>(done)
        },
        move |ended| {
            let transfer = ended_transfer;
            match ended.unwrap_or_else(|bug| Err(bug.into())) {
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
        },
    );

    (rx, running)
}

fn send_progress(tx: &ProgressTx, progress: TransferProgress) {
    if let Err(mpsc::error::SendError(progress)) = tx.send(progress) {
        debug!(?progress, "release transfer progress receiver dropped");
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
