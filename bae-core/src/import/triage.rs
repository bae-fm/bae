//! The import sidebar's rows, decided once in core.
//!
//! The sidebar asks the same questions of every candidate — which tab it
//! belongs to, which Needs-you group it joins, what it leads with, and which
//! commands it offers — and every one of them is a rule rather
//! than a rendering. [`crate::identify::view`] is the precedent: shape the
//! state for the surfaces once, here, so both desktop UIs render the same
//! decisions instead of each re-deriving them from a
//! [`FolderCandidate`](crate::import::FolderCandidate) and a
//! [`TerminalVerdict`](crate::identify::TerminalVerdict).
//!
//! **Nothing here formats text.** Years, counts, durations and byte sizes cross
//! as numbers, and a disagreement crosses as its own [`NeedsYou`] variant
//! carrying its operands, so each platform builds the sentence in its own
//! locale.
//!
//! **Nothing here re-classifies.** [`crate::identify::ready::classify`] already
//! answers what the queue needs from the user; this module decides where that
//! answer puts the row and what the row shows. Which rows exist and in what
//! order is [`crate::import::list`]'s.

use super::folder_scanner::{FolderReleaseDecisionKey, ResolvedFolderReleaseBoundary};
use super::search::{ImportSearchReleaseDetail, SourceTracks};
use super::types::{MetadataProvenance, MetadataSource};
use super::{CandidateRuntimeSnapshot, ImportedRelease};
use crate::identify::{IdentifyState, LeadMatch, NeedsYou, QueueClassification, VerdictSummary};

mod actions;
mod model;

pub(crate) use actions::candidate_actions;
pub use actions::CandidateAction;
pub use model::*;

/// Which tab a candidate belongs to, and why a Pending row still needs input.
///
/// A total function of four facts core already holds, checked in one order:
///
/// 1. **An import in flight outranks everything**, including the library
///    check: the release row lands partway through an import, so `is_added`
///    flips before the import is finished, and a row that reads Done then says
///    the folder is in the library while its files are still being copied.
/// 2. **Then Done**, which is an import that completed, or a folder a previous
///    session already imported. Not awaiting triage, whatever its verdict says
///    and whether or not it was ever skipped.
/// 3. **Then Skipped**, which is a decision the user already made.
/// 4. **Then a failed attempt**, which is Pending work: the folder is not in
///    the library and the only thing standing between it and being there is
///    another attempt. It comes before the pick because a failed candidate
///    always has one — read the pick first and the row would say Ready, and
///    join the set a bulk import sweeps up, on the strength of the attempt
///    that just failed.
/// 5. **Then a stored pick**, which is the user answering whatever the verdict
///    was going to ask. Nothing is left to ask, so the row is Ready.
/// 6. **Then what is known about it**, for a candidate nobody has answered.
///
/// A candidate with no verdict and no valid draft is not Ready. Placement
/// describes its preparation; action availability also accounts for live
/// identification so a stored Ready draft does not permit conflicting work.
pub fn place(
    skipped: bool,
    is_added: bool,
    import_status: Option<&TriageImportStatus>,
    picked: Option<&MetadataProvenance>,
    metadata_draft_valid: bool,
    answer: &CandidateAnswer,
) -> TriagePlacement {
    // Spelled out rather than `is_some()`: each variant places the row
    // somewhere different, and a new one should have to be placed here on
    // purpose rather than inherited by an `_`.
    let failed = match import_status {
        Some(TriageImportStatus::Importing) => return TriagePlacement::Importing,
        Some(TriageImportStatus::Complete { .. }) => return TriagePlacement::Done,
        Some(TriageImportStatus::Error { .. }) => true,
        None => false,
    };
    if is_added {
        return TriagePlacement::Done;
    }
    if skipped {
        return TriagePlacement::Skipped;
    }
    if failed {
        return TriagePlacement::Failed;
    }
    // The pick is the answer. Whatever the verdict was going to ask — which of
    // three pressings, which of two signals, a release already in the library
    // — the user has said which release this is, or that it reads as its own
    // tags, and the only thing left is to import it. Without this the row
    // keeps the question's tag forever after it was answered.
    if picked.is_some() || metadata_draft_valid {
        return TriagePlacement::Ready;
    }
    let reason = match answer {
        CandidateAnswer::Classified(QueueClassification::Ready) => return TriagePlacement::Ready,
        CandidateAnswer::Classified(QueueClassification::NeedsYou(needs_you)) => needs_you.clone(),
        CandidateAnswer::Unidentified => return TriagePlacement::Pending,
        CandidateAnswer::Identification(status) => {
            return TriagePlacement::Identification {
                status: status.clone(),
            }
        }
    };
    TriagePlacement::NeedsYou { reason }
}

/// Where a candidate's import stands, from the three places that can say so.
///
/// A running import is the only live fact, so it outranks both stored ones. Of
/// those, the release wins: the failure row is written when an attempt fails
/// and cleared when the next one is queued, so a release for this hash means
/// an attempt already succeeded and any leftover error is behind it.
///
/// The stored failure is here rather than only in the pane because a row has
/// to say it too. Without it, quitting after a failed import brings the
/// candidate back as an ordinary pending row, and the only way to find out it
/// failed is to open it. It stays in Pending either way — see
/// [`TriagePlacement::Failed`] — but as a row that says what went wrong.
pub fn import_status_of(
    importing: bool,
    imported: Option<&ImportedRelease>,
    failure: Option<&str>,
) -> Option<TriageImportStatus> {
    if importing {
        return Some(TriageImportStatus::Importing);
    }
    if let Some(release) = imported {
        return Some(TriageImportStatus::Complete {
            release: release.clone(),
        });
    }
    failure.map(|error| TriageImportStatus::Error {
        error: error.to_string(),
    })
}

/// The runtime facts a row's placement reads: a change to any other part of
/// a candidate's runtime — a progress tick within a running import — leaves
/// the queue as projected.
#[derive(Debug, Clone, PartialEq)]
pub struct TriageRuntimeFacts {
    pub identification: Option<IdentificationStatus>,
    /// Whether an import owns this candidate right now. How far it has got is
    /// the runtime's, read by the leaf that draws the bar.
    pub importing: bool,
}

impl Default for TriageRuntimeFacts {
    /// A key nothing is running for: no identification work exists and no
    /// import has claimed it.
    fn default() -> Self {
        Self {
            identification: None,
            importing: false,
        }
    }
}

impl TriageRuntimeFacts {
    pub fn of(runtime: &CandidateRuntimeSnapshot) -> Self {
        Self {
            identification: runtime.identify.as_ref().map(|identify| {
                if let Some(error) = identify.finalization_failure() {
                    return IdentificationStatus::FinalizationFailed {
                        error: error.to_string(),
                    };
                }
                match identify.state() {
                    None => IdentificationStatus::Queued,
                    Some(IdentifyState::Triangulating { .. }) => IdentificationStatus::Running,
                    Some(
                        IdentifyState::Found { .. }
                        | IdentifyState::NotFoundAnywhere { .. }
                        | IdentifyState::ManualOnly { .. }
                        | IdentifyState::Failed { .. },
                    ) => IdentificationStatus::Finalizing,
                    Some(IdentifyState::Idle) => {
                        unreachable!("idle identification has no runtime value")
                    }
                }
            }),
            importing: runtime.import.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_identify_result_is_not_a_needs_you_question() {
        let placement = place(
            false,
            false,
            None,
            None,
            false,
            &CandidateAnswer::Identification(IdentificationStatus::Finalizing),
        );

        assert!(!matches!(placement, TriagePlacement::NeedsYou { .. }));
    }
}
