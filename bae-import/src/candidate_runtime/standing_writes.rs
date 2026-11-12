//! The writes that change where a folder candidate stands, each awaited by
//! the owner of what it changes.
//!
//! A change is decided from the row as stored inside its own write (see
//! [`IdentificationChange::applied`]), so a run that lost its standing — it
//! was cancelled, or its candidate admitted again — reports into nothing
//! rather than over a newer one. The identification count moves in the same
//! write as the change that moves it.

use super::{CandidateRuntime, IdentificationChange, IdentificationMove, IdentificationStage};
use crate::tables::ImportClaim;
use crate::triage::TriageRuntimeFacts;
use crate::CandidateKey;
use crate::IdentifyRunId;
use crate::ImportStep;
use crate::{Admission, CandidateFiles, SaveFailure};
use bae_store::DbError;

/// What a run identifies: a folder candidate's files, whose standing the
/// table holds, or a library release, which has none.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RunSubject {
    Candidate(CandidateFiles),
    Release,
}

impl CandidateRuntime {
    async fn change_identifications(
        &self,
        changes: Vec<(CandidateKey, IdentificationChange)>,
    ) -> Result<Vec<(CandidateKey, IdentificationMove)>, DbError> {
        let moved = self.tables.change_candidate_identifications(changes).await?;
        #[cfg(any(test, feature = "test-utils"))]
        self.tell_every_change(moved.iter().map(|(key, _)| key)).await;
        Ok(moved)
    }

    /// Tell the test readers of every change what each of `keys` reads as
    /// now, and of every count what the count reads as when it moved.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn tell_every_change(&self, keys: impl Iterator<Item = &CandidateKey>) {
        let count = self
            .identification_progress()
            .await
            .expect("a test reads back the count it just moved");
        let moved = {
            let mut told = self.told_count.lock().expect("told count lock poisoned");
            let moved = *told != count;
            *told = count;
            moved
        };
        if moved {
            self.every_count
                .lock()
                .expect("count readers lock poisoned")
                .retain(|reader| reader.send(count).is_ok());
        }
        if self.every_change.lock().expect("change readers lock poisoned").is_empty() {
            return;
        }
        for key in keys {
            let change = match self
                .snapshot(key)
                .await
                .expect("a test reads back the standing it just wrote")
            {
                Some(runtime) => {
                    super::CandidateRuntimeChange::Updated { key: key.clone(), runtime }
                }
                None => super::CandidateRuntimeChange::Removed { key: key.clone() },
            };
            self.every_change
                .lock()
                .expect("change readers lock poisoned")
                .retain(|reader| reader.send(change.clone()).is_ok());
        }
    }

    async fn change_identification(
        &self,
        key: &CandidateKey,
        change: IdentificationChange,
    ) -> Result<IdentificationMove, DbError> {
        let (_, moved) = self
            .change_identifications(vec![(key.clone(), change)])
            .await?
            .pop()
            .expect("one change makes one move");
        Ok(moved)
    }

    /// Mark every one of `candidates` as waiting on `admission` in one write,
    /// so the count opens at its total.
    pub(crate) async fn admit(
        &self,
        candidates: Vec<(CandidateKey, CandidateFiles)>,
        admission: Admission,
    ) -> Result<(), DbError> {
        if candidates.is_empty() {
            return Ok(());
        }
        self.change_identifications(
            candidates
                .into_iter()
                .map(|(key, files)| (key, IdentificationChange::Admit { files, admission }))
                .collect(),
        )
        .await?;
        Ok(())
    }

    /// This key is not waiting any more: nothing came of the admission that
    /// put it there.
    pub(crate) async fn withdraw(&self, key: &CandidateKey) -> Result<(), DbError> {
        self.change_identification(key, IdentificationChange::Withdraw).await?;
        Ok(())
    }

    /// `run` started from the wait for `files`, having published `state`
    /// first, which is stored in the same write; `false` when the key is not
    /// waiting for it any more, and the run's reports are no standing's.
    pub(crate) async fn run_started(
        &self,
        key: &CandidateKey,
        files: &CandidateFiles,
        run: IdentifyRunId,
        state: &bae_identify::IdentifyState,
    ) -> Result<bool, DbError> {
        let moved = self.tables.start_candidate_run(key, files, run, state).await?;
        #[cfg(any(test, feature = "test-utils"))]
        self.tell_every_change(std::iter::once(key)).await;
        Ok(moved.after.is_some_and(|after| after.stage == IdentificationStage::Running { run }))
    }

    /// `run` reached its answer, which is now being written; `false` when
    /// the key's standing is not that run's any more.
    pub(crate) async fn run_answered(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        reads_pick: bool,
    ) -> Result<bool, DbError> {
        let moved = self
            .change_identification(key, IdentificationChange::RunAnswered { run, reads_pick })
            .await?;
        Ok(moved.after != moved.before)
    }

    /// `run` ended without an answer; what it published goes in the same
    /// write.
    pub(crate) async fn run_ended(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
    ) -> Result<(), DbError> {
        self.change_identification(key, IdentificationChange::RunEnded { run }).await?;
        Ok(())
    }

    /// Identification of this key is over and nothing will write what it
    /// found: whatever run or answer its standing holds ends, and what every
    /// run of it published goes, in one write. A library release's
    /// re-identify run, which holds no standing, ends the same way when its
    /// sheet closes.
    pub(crate) async fn cancel_identification(&self, key: &CandidateKey) -> Result<(), DbError> {
        self.tables.cancel_candidate_identification(key).await?;
        #[cfg(any(test, feature = "test-utils"))]
        self.tell_every_change(std::iter::once(key)).await;
        Ok(())
    }

    /// `run`'s answer is disposed of with no write of its verdict. Returns
    /// whether a person had asked for the candidate to be imported once that
    /// answer stored its pick.
    pub(crate) async fn end_identification_answer(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
    ) -> Result<bool, DbError> {
        let moved =
            self.change_identification(key, IdentificationChange::AnswerEnded { run }).await?;
        Ok(moved.ended_with_import_asked())
    }

    /// How far the batch of identifications in flight has got: ended of
    /// admitted, for a test; the live view carries it to everyone else.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn identification_progress(&self) -> Result<(u32, u32), DbError> {
        self.tables.load_identification_progress().await
    }

    /// `run`'s answer for `files` could not be written; the standing says why.
    /// bae's own failure, so it is logged as one.
    pub(crate) async fn fail_identification(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        files: &CandidateFiles,
        failure: SaveFailure,
    ) -> Result<(), DbError> {
        tracing::error!("identification: {key}'s answer did not land: {}", failure.error());
        self.change_identification(
            key,
            IdentificationChange::AnswerFailed { files: files.clone(), run, failure },
        )
        .await?;
        Ok(())
    }

    /// No run of `files` could start, because what it starts from could not
    /// be read; the standing says why, and identifying the candidate again
    /// can start one. Bae's own failure, so it is logged as one.
    pub(crate) async fn fail_identification_start(
        &self,
        key: &CandidateKey,
        files: &CandidateFiles,
        failure: SaveFailure,
    ) -> Result<(), DbError> {
        tracing::error!("identification: no run of {key} could start: {}", failure.error());
        self.change_identification(
            key,
            IdentificationChange::StartFailed { files: files.clone(), failure },
        )
        .await?;
        Ok(())
    }

    /// Ask for the candidate to be imported once the answer being written
    /// stores the release it picks unattended, which is still being read.
    /// `false`, asking nothing, when no answer that picks unattended is held
    /// for the key.
    pub(crate) async fn ask_import_once_stored(&self, key: &CandidateKey) -> Result<bool, DbError> {
        let moved = self.change_identification(key, IdentificationChange::ImportAsked).await?;
        Ok(matches!(
            moved.after.as_ref().map(|after| &after.stage),
            Some(IdentificationStage::ReadingPick { .. })
        ))
    }

    /// Record that `import_id` owns this candidate, from the moment its
    /// command is queued rather than when the worker first reports, since
    /// whether identification may still answer the candidate reads it.
    /// Refused when an import already owns it. With `ending_identification`
    /// the claim ends the candidate's identification, whatever it was, in the
    /// same write: once claimed, it is not identification's to answer.
    pub(crate) async fn claim_for_import(
        &self,
        key: &CandidateKey,
        import_id: &str,
        ending_identification: bool,
    ) -> Result<(), crate::ImportError> {
        let claim = self
            .tables
            .claim_candidate_import(
                key,
                import_id,
                ending_identification.then_some(IdentificationChange::Ended),
            )
            .await?;
        match claim {
            ImportClaim::Refused => Err(crate::ImportError::CandidateImportInProgress),
            ImportClaim::Claimed { .. } => {
                #[cfg(any(test, feature = "test-utils"))]
                self.tell_every_change(std::iter::once(key)).await;
                Ok(())
            }
        }
    }

    /// The step `import_id`, which owns `key`, is at now, written before the
    /// import starts it, or its end with `None`: its claim released,
    /// cancelled, or ended. Refused when that import does not own the key.
    pub(crate) async fn move_import(
        &self,
        key: &CandidateKey,
        import_id: &str,
        next: Option<ImportStep>,
    ) -> Result<(), crate::ImportError> {
        self.tables.move_candidate_import(key, import_id, next).await?;
        #[cfg(any(test, feature = "test-utils"))]
        self.tell_every_change(std::iter::once(key)).await;
        Ok(())
    }

    /// Whether an import owns `key`, read from its claim.
    pub(crate) async fn import_claimed(&self, key: &CandidateKey) -> Result<bool, DbError> {
        Ok(self.tables.load_candidate_standing(key).await?.import.is_some())
    }

    /// Where `key` stands, as its row reads it.
    pub(crate) async fn standing_facts(
        &self,
        key: &CandidateKey,
    ) -> Result<TriageRuntimeFacts, DbError> {
        Ok(self.tables.load_candidate_standing(key).await?.facts())
    }
}

#[cfg(test)]
#[path = "standing_writes_tests.rs"]
mod tests;
