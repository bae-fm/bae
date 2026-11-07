//! The download (pin) queue.
//!
//! Pinning routes through a serial queue rather than an ephemeral
//! per-release task: one release downloads at a time, the rest wait, and the
//! user can pause/cancel/retry. The queue is the `download_queue` table, which
//! the library keeps as it opens, with its pause: a release that wasn't fully
//! pinned stays as it was, since a pin flips a release to pinned only once
//! every file lands, and its entry runs again.

use super::*;

impl LibraryManager {
    /// Enqueue releases to pin for offline, in one write that passes over ids
    /// already waiting or downloading, missing from the library (deleted
    /// meanwhile), local, or already pinned, and queues one whose download
    /// failed again in its place. A write that fails queues nothing
    /// and is reported as a bug: the library's own rows failed to read. The
    /// entry names the release by id only; the Downloads pane reads its title,
    /// file count, and size from the library as it shows the row. Wakes the
    /// parked worker.
    pub async fn enqueue_pins(&self, release_ids: Vec<String>) -> Result<(), LibraryError> {
        let enqueued_at = bae_store::utc_now(self.clock.as_ref()).timestamp_millis();
        match self.downloads.enqueue_pinnable(release_ids, enqueued_at).await {
            Ok(true) => self.downloads.queued(),
            Ok(false) => debug!("enqueue_pins: nothing to pin"),
            Err(error) => {
                self.diagnostics.report_failure("queue releases to pin", &error);
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// Pause or resume the download queue. Pausing stops the running download at
    /// once and leaves its entry queued; while paused the worker parks instead of
    /// starting the next release. Resuming wakes the worker, which starts the
    /// stopped download again from the beginning.
    pub async fn set_downloads_paused(&self, paused: bool) -> Result<(), LibraryError> {
        Ok(self.downloads.set_paused(paused).await?)
    }

    /// Cancel a release's download. Drops a queued or failed entry; cancelling
    /// an active pin signals its worker to restore the previous pin state.
    pub async fn cancel_download(&self, release_id: &str) -> Result<(), LibraryError> {
        // An active entry needs no follow-up here: the aborted pin task closes its
        // progress channel, and the worker's drain clears the streamed transfer
        // action, finds the entry gone, leaves the queue alone, and re-parks.
        Ok(self.downloads.cancel(release_id).await?)
    }

    /// Flip every failed download back to queued and wake the worker to retry
    /// them.
    pub async fn retry_downloads(&self) -> Result<(), LibraryError> {
        Ok(self.downloads.retry_failed().await?)
    }

    /// The serial download worker: drains the pin queue one release at a time
    /// through [`QueuedReleases::run_serial`], which owns the queue protocol
    /// (the activate/cancel race, cancel-is-not-a-failure, remove vs
    /// mark-failed).
    ///
    /// [`QueuedReleases::run_serial`]: bae_transfers::QueuedReleases::run_serial
    ///
    /// All this supplies is how a pin runs: resolve the release's byte totals,
    /// ask [`LibraryManager::start_pin_transfer`] to compose the pin task with this worker's progress
    /// drain, and yield the resulting operation. The
    /// outcome comes from draining the transfer's progress channel — that drain
    /// also publishes the action the storage row reads — and then joining the pin
    /// task, so a panicked pin reports the panic rather than "the channel closed".
    ///
    /// Success needs no follow-up here: `pin_release_files_with_progress`
    /// pins the files and ends the queue entry in one write, so the release
    /// subscription updates its `pinned` flag as the entry leaves.
    /// Completion and failure reach diagnostics from inside the transfer
    /// (`StorageTransferCompleted` / `StorageTransferFailed`), so there is nothing
    /// to report on the way out.
    pub(super) async fn run_download_worker(&self) {
        self.downloads
            .run_serial(
                "Pin",
                |op, entry| async move {
                    let release_id = op.release_id;
                    let initial_progress = self.initial_download_progress(&release_id).await?;
                    let drive_release_id = release_id.clone();
                    let running =
                        self.start_pin_transfer(release_id, entry, move |progress| async move {
                            self.drive_transfer(
                                &drive_release_id,
                                ReleaseStorageAction::Pin,
                                progress,
                            )
                            .await
                        });
                    Ok((initial_progress, running))
                },
                |_release_id, _result| {},
                |label, error| self.report_queue_store_failure(label, error),
                |panic| self.panicked(panic),
            )
            .await
    }
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
