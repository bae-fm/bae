//! Cancelling a queued or running import.

use super::*;
use crate::import_cancel::CancelOutcome;

impl ImportServiceHandle {
    /// Cancel the waiting or running import of `candidate_key`, leaving the
    /// candidate as it stood before the import was asked for. An import already
    /// writing its release finishes, and that is the error.
    pub async fn cancel_import(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        match self.services.import_cancels.cancel(candidate_key) {
            CancelOutcome::NotImporting | CancelOutcome::CancelledRunning => Ok(()),
            CancelOutcome::CancelledWaiting { import_id } => {
                self.end_cancelled_import(candidate_key, import_id).await
            }
            CancelOutcome::Writing => Err(crate::ImportError::ImportWriting),
        }
    }

    /// Cancel every import that has not begun writing its release. Each one
    /// cancelled while it waited ends here; the first whose ending cannot be
    /// written fails the cancel, after the rest have been tried.
    pub async fn cancel_all_imports(&self) -> Result<(), crate::ImportError> {
        let mut failed = Ok(());
        for (candidate_key, import_id) in self.services.import_cancels.cancel_all() {
            let ended = self.end_cancelled_import(&candidate_key, import_id).await;
            if failed.is_ok() {
                failed = ended;
            }
        }
        failed
    }

    /// [`Self::cancel_all_imports`] for a caller that cannot wait for it — a
    /// person's command that returns at once. An ending that cannot be
    /// written has no one to fail to, so it is a bug, reported as one.
    pub fn cancel_all_imports_in_background(&self) {
        let this = self.clone();
        self.services.diagnostics.spawn_reported(
            &self.runtime_handle,
            "cancel every waiting import",
            async move {
                if let Err(error) = this.cancel_all_imports().await {
                    this.services.diagnostics.report_failure("cancel every waiting import", &error);
                }
            },
            // Reported; no one waits to hear more.
            |_| {},
        );
    }

    /// End an import cancelled while it waited, and announce it: the worker
    /// skips it, so it never will.
    async fn end_cancelled_import(
        &self,
        candidate_key: &crate::CandidateKey,
        import_id: String,
    ) -> Result<(), crate::ImportError> {
        info!("Import of {candidate_key} was cancelled before it started");
        self.runtime.move_import(candidate_key, &import_id, None).await?;
        self.services.event_tx.send(ImportEvent::ImportProgress {
            candidate_key: candidate_key.clone(),
            progress: ImportProgress::Cancelled { import_id },
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "cancel_tests.rs"]
mod tests;
