//! Where a candidate something is running for stands, as the standings table
//! (`import_candidate_standing`) holds it, and the change each owner of it
//! makes.
//!
//! The table is the only place a candidate's standing is held. Each change
//! below is decided from the row as it stands, inside the write that stores
//! its result, so a guard and the write it guards are one: no change is
//! applied to a standing it did not read.

use crate::identify::IdentifyRunId;
use crate::import::candidates::{Admission, CandidateFiles, SaveFailure};
use crate::import::triage::{IdentificationStatus, ImportStanding, TriageRuntimeFacts};

/// One candidate's row: its identification and its import, each absent when
/// nothing of that kind is going for it.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct StoredStanding {
    pub(crate) identification: Option<StoredIdentification>,
    pub(crate) import: Option<StoredImport>,
}

impl StoredStanding {
    /// Where the candidate stands as its row and the list read it.
    pub(crate) fn facts(&self) -> TriageRuntimeFacts {
        TriageRuntimeFacts {
            identification: self
                .identification
                .as_ref()
                .map(|identification| identification.stage.status()),
            import: self.import.as_ref().map(|import| import.standing),
        }
    }
}

/// An identification of the files it names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredIdentification {
    pub(crate) files: CandidateFiles,
    pub(crate) stage: IdentificationStage,
}

/// How far one identification has got.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IdentificationStage {
    /// On the identification queue, its run not started.
    Queued { admission: Admission },
    /// `run` is in flight.
    Running { run: IdentifyRunId },
    /// `run` answered, and its answer is being written.
    Finalizing { run: IdentifyRunId },
    /// `run` answered with a release it picks unattended, which is read in
    /// full before the answer is written; `import_asked` is a person asking
    /// for the candidate to be imported once it is.
    ReadingPick {
        run: IdentifyRunId,
        import_asked: bool,
    },
    /// The last answer's write failed.
    Failed { failure: SaveFailure },
}

impl IdentificationStage {
    /// The run this stage is of; none while queued or after a failed write.
    pub(crate) fn run(&self) -> Option<IdentifyRunId> {
        match self {
            Self::Running { run } | Self::Finalizing { run } | Self::ReadingPick { run, .. } => {
                Some(*run)
            }
            Self::Queued { .. } | Self::Failed { .. } => None,
        }
    }

    fn status(&self) -> IdentificationStatus {
        match self {
            Self::Queued { .. } => IdentificationStatus::Queued,
            Self::Running { .. } => IdentificationStatus::Running,
            Self::Finalizing { .. } => IdentificationStatus::Finalizing,
            Self::ReadingPick { import_asked, .. } => IdentificationStatus::ReadingPick {
                import_asked: *import_asked,
            },
            Self::Failed { failure } => IdentificationStatus::FinalizationFailed {
                failure: failure.clone(),
            },
        }
    }
}

/// The import that owns a candidate, and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredImport {
    pub(crate) import_id: String,
    pub(crate) standing: ImportStanding,
}

/// One owner's change to a candidate's identification, decided from the one
/// it reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IdentificationChange {
    /// The queue admitted the candidate's `files`: it waits for a run, over
    /// whatever its identification was.
    Admit {
        files: CandidateFiles,
        admission: Admission,
    },
    /// The queue let the candidate go before its run started.
    Withdraw,
    /// `run`'s first report: it started from the wait for `files`.
    RunStarted { files: CandidateFiles, run: IdentifyRunId },
    /// `run` reached its answer; `reads_pick` when the answer picks a release
    /// unattended, which is read before it is written.
    RunAnswered { run: IdentifyRunId, reads_pick: bool },
    /// `run` ended without an answer: it was cancelled.
    RunEnded { run: IdentifyRunId },
    /// `run`'s answer is disposed of: written, refused, or given up.
    AnswerEnded { run: IdentifyRunId },
    /// `run`'s answer for `files` could not be written.
    AnswerFailed {
        files: CandidateFiles,
        run: IdentifyRunId,
        failure: SaveFailure,
    },
    /// A person asked for the candidate to be imported once the answer being
    /// written stores the release it picks unattended.
    ImportAsked,
    /// The candidate's identification was cancelled: whatever run or answer
    /// it held is over, and nothing will write what it found.
    Cancelled,
    /// The candidate's identification ends whatever it was: a person decided
    /// what the candidate is, or an import took it.
    Ended,
    /// The scan now holds the candidate with `files`, or not at all with
    /// `None`: an identification of other files ends.
    ScanHolds { files: Option<CandidateFiles> },
}

impl IdentificationChange {
    /// The identification this change leaves, from the one stored.
    pub(crate) fn applied(
        &self,
        stored: Option<&StoredIdentification>,
    ) -> Option<StoredIdentification> {
        let stage = stored.map(|stored| &stored.stage);
        let unchanged = || stored.cloned();
        match self {
            Self::Admit { files, admission } => Some(StoredIdentification {
                files: files.clone(),
                stage: IdentificationStage::Queued {
                    admission: *admission,
                },
            }),
            Self::Withdraw => match stage {
                Some(IdentificationStage::Queued { .. }) => None,
                _ => unchanged(),
            },
            // A run starts only from the wait its job was admitted to; a run
            // that lost that wait (cancelled, or its candidate admitted again
            // as other files) reports into nothing.
            Self::RunStarted { files, run } => match stored {
                Some(StoredIdentification {
                    files: queued,
                    stage: IdentificationStage::Queued { .. },
                }) if queued == files => Some(StoredIdentification {
                    files: files.clone(),
                    stage: IdentificationStage::Running { run: *run },
                }),
                _ => unchanged(),
            },
            Self::RunAnswered { run, reads_pick } => match stored {
                Some(StoredIdentification {
                    files,
                    stage: IdentificationStage::Running { run: running },
                }) if running == run => Some(StoredIdentification {
                    files: files.clone(),
                    stage: if *reads_pick {
                        IdentificationStage::ReadingPick {
                            run: *run,
                            import_asked: false,
                        }
                    } else {
                        IdentificationStage::Finalizing { run: *run }
                    },
                }),
                _ => unchanged(),
            },
            Self::RunEnded { run } => match stage {
                Some(IdentificationStage::Running { run: running }) if running == run => None,
                _ => unchanged(),
            },
            // The answer ends whether or not its stage was recorded: a run
            // whose answer's record did not land still ends with it.
            Self::AnswerEnded { run } => match stage {
                Some(stage) if stage.run() == Some(*run) => None,
                _ => unchanged(),
            },
            // A failure replaces this run's answer, an earlier failure, or
            // nothing; a candidate waiting for or running a later
            // identification keeps it.
            Self::AnswerFailed {
                files,
                run,
                failure,
            } => {
                let failed = |files: &CandidateFiles| {
                    Some(StoredIdentification {
                        files: files.clone(),
                        stage: IdentificationStage::Failed {
                            failure: failure.clone(),
                        },
                    })
                };
                match stored {
                    None
                    | Some(StoredIdentification {
                        stage: IdentificationStage::Failed { .. },
                        ..
                    }) => failed(files),
                    Some(answered) if answered.stage.run() == Some(*run) => {
                        failed(&answered.files)
                    }
                    Some(_) => unchanged(),
                }
            }
            Self::ImportAsked => match stored {
                Some(StoredIdentification {
                    files,
                    stage: IdentificationStage::ReadingPick { run, .. },
                }) => Some(StoredIdentification {
                    files: files.clone(),
                    stage: IdentificationStage::ReadingPick {
                        run: *run,
                        import_asked: true,
                    },
                }),
                _ => unchanged(),
            },
            Self::Cancelled => match stage {
                Some(stage) if stage.run().is_some() => None,
                _ => unchanged(),
            },
            Self::Ended => None,
            Self::ScanHolds { files } => match stored {
                Some(stored) if Some(&stored.files) == files.as_ref() => unchanged(),
                _ => None,
            },
        }
    }
}

/// Where a candidate stands in identification, as the batch counts it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Identifying {
    /// Admitted and its run not started, which is what joins the batch.
    pub(crate) queued: bool,
    /// Waiting, running, or having its answer written; a failed write is
    /// over.
    pub(crate) in_flight: bool,
}

impl Identifying {
    pub(crate) fn of(stored: Option<&StoredIdentification>) -> Self {
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

/// What a change did to a candidate's identification: the one it read and
/// the one it left.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IdentificationMove {
    pub(crate) before: Option<StoredIdentification>,
    pub(crate) after: Option<StoredIdentification>,
}

impl IdentificationMove {
    /// Whether a person asked for an Import of the answer this move ended.
    pub(crate) fn ended_with_import_asked(&self) -> bool {
        self.after.is_none()
            && matches!(
                self.before.as_ref().map(|before| &before.stage),
                Some(IdentificationStage::ReadingPick {
                    import_asked: true,
                    ..
                })
            )
    }
}

#[cfg(test)]
#[path = "stored_standing_tests.rs"]
mod tests;
