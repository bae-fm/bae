//! Starting the import a candidate's verdict owes.
//!
//! An automatic run that settles on a verdict needing nothing from anyone,
//! while "Import automatically when identified" is on, stores that the verdict
//! owes an import, in the same write as the verdict. This is what pays it:
//! the same import a person's Import ready starts, going where the library's
//! stored storage choice says, refused on the same grounds, and decided under
//! the lock every write to the candidate is taken under.
//!
//! What was owed stays owed until something answers it — an import attempt
//! ending, which its own commit or failure write records, or the decision here
//! not to import — so the owed row is the whole record of whether an import
//! is still to come: one asked for and never started, because the app quit
//! in between, is found again on the next launch, and one that already ran is
//! never started twice.

use super::*;

/// What became of the import a candidate's verdict owed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwedImport {
    /// Nothing is owed: the verdict owes no import, or an attempt already
    /// answered it.
    NotOwed,
    /// An import owns the candidate already — a person's, or the one this
    /// owed import started — and ends what was owed when it ends.
    AlreadyImporting,
    /// Identification is running for the candidate again. The verdict that
    /// run stores replaces this one and decides again, so what this one owed
    /// is left for it.
    BeingIdentified,
    /// The candidate is no longer what the verdict owed an import for, so it
    /// is not imported and nothing is owed any more.
    Declined(OwedImportDeclined),
    /// The import started.
    Started { import_id: String },
    /// The import could not be started. The failure is recorded on the
    /// candidate as its failed import, where a person's failed import shows,
    /// and that answers what was owed.
    FailedToStart { error: String },
}

/// Why an owed import was not started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwedImportDeclined {
    /// Importing when identified is off now.
    SettingOff,
    /// Its draft changed after the verdict owed the import: someone decided
    /// something about it since, and what they decided is theirs to import.
    Edited,
    /// The candidate is not importable unattended as it stands: the same
    /// Ready rule a bulk import of the Ready set goes by says no.
    NotReady,
}

impl ImportServiceHandle {
    /// Start the import `candidate_key`'s verdict owes, if it still owes one,
    /// going to `destination`. `enabled` is whether importing when identified
    /// is on: off, whatever is owed is withdrawn instead.
    ///
    /// An error is a read that failed before anything was decided: nothing
    /// was written, and what was owed stays owed for the next time it is
    /// looked at.
    pub(crate) async fn import_owed(
        &self,
        candidate_key: &str,
        enabled: bool,
        destination: crate::config::ImportDestination,
    ) -> Result<OwedImport, crate::import::ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        self.committed(async move {
            this.import_owed_write(&candidate_key, enabled, destination)
                .await
        })
        .await
    }

    async fn import_owed_write(
        &self,
        candidate_key: &str,
        enabled: bool,
        destination: crate::config::ImportDestination,
    ) -> Result<OwedImport, crate::import::ImportError> {
        let commit = self.folder_state_commit.lock("start an owed import").await;
        let Some(candidate) = self.get_release_candidate(candidate_key).await? else {
            return Ok(OwedImport::NotOwed);
        };
        let content_hash = candidate.files.content_hash();
        let Some(owed_revision) = self.library_manager.load_owed_import(&content_hash).await?
        else {
            return Ok(OwedImport::NotOwed);
        };
        let facts = self.runtime_facts(candidate_key);
        if facts.importing {
            return Ok(OwedImport::AlreadyImporting);
        }
        if facts.identifying() {
            return Ok(OwedImport::BeingIdentified);
        }
        let declined = if !enabled {
            Some(OwedImportDeclined::SettingOff)
        } else {
            match self.library_manager.load_import_candidate(candidate_key).await? {
                None => Some(OwedImportDeclined::NotReady),
                Some(projection) => {
                    let detail = projection.resolve(&facts);
                    if detail.metadata_revision != owed_revision {
                        Some(OwedImportDeclined::Edited)
                    } else if !detail
                        .live
                        .actions
                        .contains(&crate::import::CandidateAction::ImportReady)
                    {
                        Some(OwedImportDeclined::NotReady)
                    } else {
                        None
                    }
                }
            }
        };
        if let Some(declined) = declined {
            self.library_manager
                .withdraw_owed_import(&content_hash)
                .await?;
            return Ok(OwedImport::Declined(declined));
        }
        match self
            .claim_import(commit, candidate_key, destination.storage_mode, destination.pin)
            .await
        {
            Ok(import_id) => Ok(OwedImport::Started { import_id }),
            Err(error) => {
                let failure = crate::import::service::ImportService::terminal_failure(
                    &error,
                    self.library_manager.now(),
                );
                self.library_manager
                    .save_import_candidate_failure(
                        &content_hash,
                        candidate.file_edit_revision,
                        &failure,
                    )
                    .await?;
                Ok(OwedImport::FailedToStart {
                    error: failure.error,
                })
            }
        }
    }
}
