//! The write of an identification's verdict.

use super::*;

/// What one verdict write did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerdictWrite {
    /// The candidate can no longer be answered, or its files are not the ones
    /// the run read.
    Refused,
    /// The row landed. `import_asked`: a person asked, while the release the
    /// answer picks unattended was read, for the candidate to be imported once
    /// it was stored.
    Stored { import_asked: bool },
}

impl ImportServiceHandle {
    /// Store the verdict `run` reached, unless the candidate can no longer be
    /// answered or its files are not the ones the verdict answers, which the
    /// write reads inside its own transaction. It runs under the commit lock
    /// and ends `run`'s pending save itself, so a caller torn down mid-write
    /// leaves nothing pending.
    ///
    /// A verdict that lands ends `run`'s identification of the candidate in
    /// its own transaction: no read sees the verdict beside the run, or the
    /// run gone without its verdict. A refused one ends it in a write of its
    /// own, and a write that fails records the failure in one.
    ///
    /// The pending save ends under the commit lock too, the lock an Import
    /// asked for while the answer's pick is read takes to mark the answer. So
    /// that ask lands on the answer before its write, which then carries it
    /// out, or after the save has ended, as an import of the stored pick:
    /// never in between, where it would be lost.
    pub(crate) async fn save_candidate_verdict_if_current(
        &self,
        candidate_key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
        row: &crate::tables::NewImportCandidateVerdict,
    ) -> Result<VerdictWrite, crate::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.clone();
        let row = row.clone();
        self.committed(async move {
            let _commit =
                this.services.folder_state_commit.lock("store an identification verdict").await;
            match this.save_candidate_verdict_if_current_write(&candidate_key, &row, run).await {
                Ok(Some(import_asked)) => Ok(VerdictWrite::Stored { import_asked }),
                Ok(None) => {
                    this.runtime.end_identification_answer(&candidate_key, run).await?;
                    Ok(VerdictWrite::Refused)
                }
                Err(error) => {
                    let recorded = this
                        .runtime
                        .fail_identification(
                            &candidate_key,
                            run,
                            &crate::CandidateFiles {
                                content_hash: row.content_hash.clone(),
                                file_edit_revision: row.file_edit_revision,
                            },
                            crate::SaveFailure::NotWritten { error: error.to_string() },
                        )
                        .await;
                    Err(match recorded {
                        Ok(()) => error,
                        Err(recording) => crate::ImportError::Internal {
                            detail: format!(
                            "{error}; recording that it was not written failed too: {recording}"
                        ),
                        },
                    })
                }
            }
        })
        .await
    }

    /// The write itself, under the commit lock its caller holds. `None` when
    /// it refused the verdict; otherwise whether a person had asked for the
    /// candidate to be imported once the answer stored its pick.
    async fn save_candidate_verdict_if_current_write(
        &self,
        candidate_key: &crate::CandidateKey,
        row: &crate::tables::NewImportCandidateVerdict,
        run: crate::IdentifyRunId,
    ) -> Result<Option<bool>, crate::ImportError> {
        match self.services.tables.store_run_verdict(row, candidate_key, run).await? {
            // The verdict's write ended the answer, and what its run
            // published, inside it.
            Some(ended) => {
                #[cfg(any(test, feature = "test-utils"))]
                self.runtime.tell_every_change(std::iter::once(candidate_key)).await;
                Ok(Some(ended.ended_with_import_asked()))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "verdict_write_tests.rs"]
mod tests;
