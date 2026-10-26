//! The write of an identification's verdict, and the check under the same
//! lock that the candidate can still be answered.

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
    /// answered or its files are not the ones the verdict answers. The check
    /// and the write share the commit lock, and the write ends `run`'s pending
    /// save itself, so a caller torn down mid-write leaves nothing pending.
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
        candidate_key: &str,
        run: crate::identify::IdentifyRunId,
        row: &crate::db::NewImportCandidateVerdict,
    ) -> Result<VerdictWrite, crate::library::LibraryError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        let row = row.clone();
        self.committed(async move {
            let _commit = this
                .folder_state_commit
                .lock("store an identification verdict")
                .await;
            match this
                .save_candidate_verdict_if_current_write(&candidate_key, &row, run)
                .await
            {
                Ok(Some(import_asked)) => Ok(VerdictWrite::Stored { import_asked }),
                Ok(None) => {
                    this.runtime
                        .end_identification_answer(&candidate_key, run)
                        .await?;
                    Ok(VerdictWrite::Refused)
                }
                Err(error) => {
                    let recorded = this
                        .runtime
                        .fail_identification(
                            &candidate_key,
                            run,
                            &crate::import::CandidateFiles {
                                content_hash: row.content_hash.clone(),
                                file_edit_revision: row.file_edit_revision,
                            },
                            crate::import::SaveFailure::NotWritten {
                                error: error.to_string(),
                            },
                        )
                        .await;
                    Err(match recorded {
                        Ok(()) => error,
                        Err(recording) => crate::library::LibraryError::Internal(format!(
                            "{error}; recording that it was not written failed too: {recording}"
                        )),
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
        candidate_key: &str,
        row: &crate::db::NewImportCandidateVerdict,
        run: crate::identify::IdentifyRunId,
    ) -> Result<Option<bool>, crate::library::LibraryError> {
        let Some(candidate) = self.answerable_candidate(candidate_key).await? else {
            return Ok(None);
        };
        // A verdict on other files than the candidate has now would describe
        // a shape it no longer has.
        if candidate.files.content_hash() != row.content_hash
            || candidate.file_edit_revision != row.file_edit_revision
        {
            return Ok(None);
        }
        let held = self.runtime.hold_standing_writes().await;
        Ok(self
            .preparations
            .store_run_verdict(row, candidate_key, run)
            .await?
            .map(|ended| {
                self.runtime
                    .answer_ended_by_verdict(&held, candidate_key, run, ended)
            }))
    }

    /// The candidate at `key` as an identification can still answer it: a
    /// stored, actionable candidate whose standing is answerable.
    pub(crate) async fn answerable_candidate(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::folder_scanner::FolderCandidate>, crate::library::LibraryError>
    {
        let Some(candidate) = self.get_release_candidate(key).await? else {
            return Ok(None);
        };
        if !self.candidate_standing(key, &candidate).await?.answerable() {
            return Ok(None);
        }
        Ok(Some(candidate))
    }
}
