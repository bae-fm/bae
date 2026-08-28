//! The import sidebar's rows, decided once in core.
//!
//! The sidebar asks the same questions of every candidate — which tab it
//! belongs to, which Needs-you group it joins, what it leads with, and whether
//! it takes a bulk-import checkbox — and every one of them is a rule rather
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

mod model;

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
/// **A candidate with no verdict yet is Needs you, not Ready** — the design
/// mockup stacks the "still identifying" group under Ready, and that is the
/// side that is wrong. Ready's count is what a bulk import would act on, so
/// admitting rows nothing is known about turns the one number on the pane that
/// has to be exact into an overstatement, and makes it move on its own while
/// someone reads it. Those rows would also be the only Ready rows with no
/// checkbox, contradicting the design's own rule that Ready is where
/// multi-select lives. And it is worst at the moment it matters most: on a
/// first launch *every* candidate is unanswered, so Ready would open full of
/// dimmed, uncheckable rows with Needs you empty — the exact inverse of the
/// truth. Under Needs you, Ready starts empty and fills as verdicts land, which
/// is the signal a person actually wants, and a still-identifying row leaves
/// its group by itself without anyone answering it.
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
        CandidateAnswer::Classified(QueueClassification::NeedsYou(needs_you)) => {
            NeedsYouReason::Disagreement(needs_you.clone())
        }
        CandidateAnswer::Idle => return TriagePlacement::Pending,
        CandidateAnswer::Unanswered(phase) => NeedsYouReason::StillIdentifying { phase: *phase },
    };
    TriagePlacement::NeedsYou {
        group: NeedsYouGroup::of(&reason),
        reason,
    }
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
    pub phase: IdentifyPhase,
    /// Whether an import owns this candidate right now. How far it has got is
    /// the runtime's, read by the leaf that draws the bar.
    pub importing: bool,
}

impl Default for TriageRuntimeFacts {
    /// A key nothing is running for: the sweep has not reached it and no
    /// import has claimed it.
    fn default() -> Self {
        Self {
            phase: IdentifyPhase::Queued,
            importing: false,
        }
    }
}

impl TriageRuntimeFacts {
    pub fn of(runtime: &CandidateRuntimeSnapshot) -> Self {
        Self {
            phase: match &runtime.identify {
                Some(state) => IdentifyPhase::of(state),
                None => IdentifyPhase::Queued,
            },
            importing: runtime.import.is_some(),
        }
    }
}
