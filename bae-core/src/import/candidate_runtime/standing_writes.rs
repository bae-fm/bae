//! The writes that change where a folder candidate stands, each awaited by
//! the owner of what it changes, and the count of identifications in flight
//! they move.
//!
//! A change is decided from the row as stored inside its own write (see
//! [`IdentificationChange::applied`]), so a run that lost its standing — it
//! was cancelled, or its candidate admitted again — reports into nothing
//! rather than over a newer one. Each write, and the count it moves, happens
//! under [`StandingWrites`], so the count follows the writes in the order they
//! landed.

use super::{
    CandidateRuntime, IdentificationChange, IdentificationMove, IdentificationStage,
    StoredIdentification,
};
use crate::db::{DbError, ImportClaim};
use crate::identify::IdentifyRunId;
use crate::import::candidates::{Admission, CandidateFiles, SaveFailure};
use crate::import::triage::{ImportStanding, TriageRuntimeFacts};

/// What a run identifies: a folder candidate's files, whose standing the
/// table holds, or a library release, which has none.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RunSubject {
    Candidate(CandidateFiles),
    Release,
}

/// The hold on standing writes a write that moves the identification count
/// keeps until the count is moved.
pub(crate) struct StandingWrites {
    _held: tokio::sync::OwnedMutexGuard<()>,
}

/// Where a key stands in identification, as the count reads it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Identifying {
    /// Admitted and its run not started, which is what joins the batch.
    queued: bool,
    /// Waiting, running, or having its answer written; a failed write is
    /// over.
    in_flight: bool,
}

impl Identifying {
    fn of(stored: Option<&StoredIdentification>) -> Self {
        match stored.map(|stored| &stored.stage) {
            Some(IdentificationStage::Queued { .. }) => Self {
                queued: true,
                in_flight: true,
            },
            Some(
                IdentificationStage::Running { .. }
                | IdentificationStage::Finalizing { .. }
                | IdentificationStage::ReadingPick { .. },
            ) => Self {
                queued: false,
                in_flight: true,
            },
            Some(IdentificationStage::Failed { .. }) | None => Self::default(),
        }
    }
}

impl CandidateRuntime {
    pub(crate) async fn hold_standing_writes(&self) -> StandingWrites {
        StandingWrites {
            _held: self.standing_writes.clone().lock_owned().await,
        }
    }

    /// Move the identification count by what `moves` did, under the hold the
    /// writes that made them kept.
    pub(crate) fn count_moves(&self, _held: &StandingWrites, moves: &[(String, IdentificationMove)]) {
        let counted = {
            let mut inner = self.inner.lock().unwrap();
            let mut counted = false;
            for (key, moved) in moves {
                let was = Identifying::of(moved.before.as_ref());
                let is = Identifying::of(moved.after.as_ref());
                if !was.queued && is.queued {
                    counted |= inner.batch.admit(key);
                } else if was.in_flight && !is.in_flight {
                    counted |= inner.batch.end(key);
                }
            }
            counted.then(|| inner.batch.progress())
        };
        if counted.is_some() {
            self.held_changed(counted);
        }
    }

    async fn change_identifications(
        &self,
        changes: Vec<(String, IdentificationChange)>,
    ) -> Result<Vec<(String, IdentificationMove)>, DbError> {
        let held = self.hold_standing_writes().await;
        let moved = self.database.change_candidate_identifications(changes).await?;
        self.count_moves(&held, &moved);
        #[cfg(any(test, feature = "test-utils"))]
        self.tell_every_change(moved.iter().map(|(key, _)| key.as_str()))
            .await;
        Ok(moved)
    }

    /// Tell the test readers of every change what each of `keys` reads as
    /// now, under the hold the write that changed them keeps.
    #[cfg(any(test, feature = "test-utils"))]
    async fn tell_every_change(&self, keys: impl Iterator<Item = &str>) {
        if self.every_change.lock().unwrap().is_empty() {
            return;
        }
        for key in keys {
            let change = match self
                .snapshot(key)
                .await
                .expect("a test reads back the standing it just wrote")
            {
                Some(runtime) => super::CandidateRuntimeChange::Updated {
                    key: key.to_string(),
                    runtime,
                },
                None => super::CandidateRuntimeChange::Removed {
                    key: key.to_string(),
                },
            };
            self.every_change
                .lock()
                .unwrap()
                .retain(|reader| reader.send(change.clone()).is_ok());
        }
    }

    async fn change_identification(
        &self,
        key: &str,
        change: IdentificationChange,
    ) -> Result<IdentificationMove, DbError> {
        let (_, moved) = self
            .change_identifications(vec![(key.to_string(), change)])
            .await?
            .pop()
            .expect("one change makes one move");
        Ok(moved)
    }

    /// Mark every one of `candidates` as waiting on `admission` in one write,
    /// so the count opens at its total.
    pub(crate) async fn admit(
        &self,
        candidates: Vec<(String, CandidateFiles)>,
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
    pub(crate) async fn withdraw(&self, key: &str) -> Result<(), DbError> {
        self.change_identification(key, IdentificationChange::Withdraw)
            .await?;
        Ok(())
    }

    /// `run` started from the wait for `files`; `false` when the key is not
    /// waiting for it any more, and the run's reports are no standing's.
    pub(crate) async fn run_started(
        &self,
        key: &str,
        files: &CandidateFiles,
        run: IdentifyRunId,
    ) -> Result<bool, DbError> {
        let moved = self
            .change_identification(
                key,
                IdentificationChange::RunStarted {
                    files: files.clone(),
                    run,
                },
            )
            .await?;
        Ok(moved
            .after
            .is_some_and(|after| after.stage == IdentificationStage::Running { run }))
    }

    /// `run` reached its answer, which is now being written; `false` when
    /// the key's standing is not that run's any more.
    pub(crate) async fn run_answered(
        &self,
        key: &str,
        run: IdentifyRunId,
        reads_pick: bool,
    ) -> Result<bool, DbError> {
        let moved = self
            .change_identification(key, IdentificationChange::RunAnswered { run, reads_pick })
            .await?;
        Ok(moved.after != moved.before)
    }

    /// `run` ended without an answer. What it published goes once the table
    /// no longer names it.
    pub(crate) async fn run_ended(&self, key: &str, run: IdentifyRunId) -> Result<(), DbError> {
        self.change_identification(key, IdentificationChange::RunEnded { run })
            .await?;
        self.forget_run(key, run);
        Ok(())
    }

    /// Identification of this key is over and nothing will write what it
    /// found: whatever run or answer its standing holds ends, and what every
    /// run of it published goes. A library release's re-identify run, which
    /// holds no standing, ends the same way when its sheet closes.
    pub(crate) async fn cancel_identification(&self, key: &str) -> Result<(), DbError> {
        self.change_identification(key, IdentificationChange::Cancelled)
            .await?;
        self.forget_runs(key);
        Ok(())
    }

    /// `run`'s answer is disposed of with no write of its verdict. Returns
    /// whether a person had asked for the candidate to be imported once that
    /// answer stored its pick.
    pub(crate) async fn end_identification_answer(
        &self,
        key: &str,
        run: IdentifyRunId,
    ) -> Result<bool, DbError> {
        let moved = self
            .change_identification(key, IdentificationChange::AnswerEnded { run })
            .await?;
        self.forget_run(key, run);
        Ok(moved.ended_with_import_asked())
    }

    /// A verdict's save ended `run`'s answer inside it, which `moved` says;
    /// count it under the hold the save kept, and return whether a person had
    /// asked for the candidate to be imported once the answer stored its pick.
    pub(crate) fn answer_ended_by_verdict(
        &self,
        held: &StandingWrites,
        key: &str,
        run: IdentifyRunId,
        moved: IdentificationMove,
    ) -> bool {
        let import_asked = moved.ended_with_import_asked();
        self.count_moves(held, &[(key.to_string(), moved)]);
        self.forget_run(key, run);
        import_asked
    }

    /// `run`'s answer for `files` could not be written; the standing says why.
    /// bae's own failure, so it is logged as one.
    pub(crate) async fn fail_identification(
        &self,
        key: &str,
        run: IdentifyRunId,
        files: &CandidateFiles,
        failure: SaveFailure,
    ) -> Result<(), DbError> {
        tracing::error!(
            "identification: {key}'s answer did not land: {}",
            failure.error()
        );
        self.change_identification(
            key,
            IdentificationChange::AnswerFailed {
                files: files.clone(),
                run,
                failure,
            },
        )
        .await?;
        self.forget_run(key, run);
        Ok(())
    }

    /// Ask for the candidate to be imported once the answer being written
    /// stores the release it picks unattended, which is still being read.
    /// `false`, asking nothing, when no answer that picks unattended is held
    /// for the key.
    pub(crate) async fn ask_import_once_stored(&self, key: &str) -> Result<bool, DbError> {
        let moved = self
            .change_identification(key, IdentificationChange::ImportAsked)
            .await?;
        Ok(matches!(
            moved.after.as_ref().map(|after| &after.stage),
            Some(IdentificationStage::ReadingPick { .. })
        ))
    }

    /// Record that `import_id` owns this candidate, from the moment its
    /// command is queued rather than when the worker first reports, since
    /// whether identification may still answer the candidate reads it.
    /// Refused when an import already owns it. With `ending_identification`
    /// the claim ends whatever run or answer the candidate's identification
    /// holds, in the same write: once claimed, it is not identification's to
    /// answer.
    pub(crate) async fn claim_for_import(
        &self,
        key: &str,
        import_id: &str,
        ending_identification: bool,
    ) -> Result<(), crate::import::ImportError> {
        let held = self.hold_standing_writes().await;
        let claim = self
            .database
            .claim_candidate_import(
                key,
                import_id,
                ending_identification.then_some(IdentificationChange::Cancelled),
            )
            .await
            .map_err(crate::library::LibraryError::from)?;
        match claim {
            ImportClaim::Refused => Err(crate::import::ImportError::CandidateImportInProgress),
            ImportClaim::Claimed { identification } => {
                if let Some(moved) = identification {
                    self.count_moves(&held, &[(key.to_string(), moved)]);
                    self.forget_runs(key);
                }
                #[cfg(any(test, feature = "test-utils"))]
                self.tell_every_change(std::iter::once(key)).await;
                drop(held);
                Ok(())
            }
        }
    }

    /// Where `import_id`, which owns `key`, stands now, or its end with
    /// `None`: its claim released, cancelled, or ended. Refused when that
    /// import does not own the key.
    pub(crate) async fn move_import(
        &self,
        key: &str,
        import_id: &str,
        next: Option<ImportStanding>,
    ) -> Result<(), DbError> {
        let held = self.hold_standing_writes().await;
        self.database
            .move_candidate_import(key, import_id, next)
            .await?;
        #[cfg(any(test, feature = "test-utils"))]
        self.tell_every_change(std::iter::once(key)).await;
        drop(held);
        Ok(())
    }

    /// Whether an import owns `key`, read from its claim.
    pub(crate) async fn import_claimed(&self, key: &str) -> Result<bool, DbError> {
        Ok(self
            .database
            .load_candidate_standing(key)
            .await?
            .import
            .is_some())
    }

    /// Where `key` stands, as its row reads it.
    pub(crate) async fn standing_facts(&self, key: &str) -> Result<TriageRuntimeFacts, DbError> {
        Ok(self.database.load_candidate_standing(key).await?.facts())
    }
}
