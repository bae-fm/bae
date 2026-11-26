//! Setting a candidate aside, or back: the skip a person asks for, which ends
//! the candidate's identification.

use super::*;

impl ImportServiceHandle {
    /// Mark the candidate at `candidate_key` skipped or unskipped and
    /// announce the change; a request that changes nothing writes nothing and
    /// sends no event.
    ///
    /// Skipping ends the candidate's identification, since the person has set
    /// it aside.
    pub async fn set_candidate_skipped(
        &self,
        candidate_key: crate::CandidateKey,
        skipped: bool,
    ) -> Result<(), crate::ImportError> {
        let this = self.clone();
        self.committed(
            async move { this.set_candidate_skipped_write(candidate_key, skipped).await },
        )
        .await
    }

    async fn set_candidate_skipped_write(
        &self,
        candidate_key: crate::CandidateKey,
        skipped: bool,
    ) -> Result<(), crate::ImportError> {
        if self.services.tables.set_candidate_skipped(&candidate_key, skipped).await? {
            if skipped {
                self.identification_ended(&candidate_key).await;
            }
            self.services.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                candidate_key,
                skipped,
            }));
        }
        Ok(())
    }
}
