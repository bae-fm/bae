//! The import sidebar's rows, decided once in core.
//!
//! The sidebar asks the same questions of every candidate — which tab it
//! belongs to, what it leads with, and which commands it offers — and every
//! one of them is a rule rather than a rendering. [`bae_identify::view`] is the precedent: shape the
//! state for the surfaces once, here, so both desktop UIs render the same
//! decisions instead of each re-deriving them from a
//! [`FolderCandidate`](crate::FolderCandidate) and a
//! [`TerminalVerdict`](bae_identify::TerminalVerdict).
//!
//! **Nothing here formats text.** Years, counts, durations and byte sizes cross
//! as numbers, and a failed check against the folder crosses as its own
//! [`FolderCheck`] variant carrying its operands, so each platform builds the
//! sentence in its own locale.
//!
//! **Nothing here re-judges a verdict.** [`bae_identify::VerdictSummary`]
//! already says which check against the folder its release failed; this module
//! decides where the row goes and what it shows. Which rows exist and in what
//! order is [`crate::list`]'s.

use super::folder_scanner::FolderReleaseDecisionKey;
use super::types::ReleaseLink;
use super::MetadataAuthor;
use bae_identify::{FolderCheck, VerdictSummary};

mod actions;
mod model;
mod selection;
mod standing;

pub use actions::{
    CandidateAction, CandidateActionBasis, CandidateLiveState, IdentifyingStep, StoredLookup,
};
pub use model::*;
pub use selection::{keys_for, selection_offers, SelectionMember, SelectionOffer, SelectionPart};
pub use standing::{BadgeTone, NeedsYouReason, PendingBadge, PendingStanding, PendingState};

/// The failed check against the folder the pane states beside Import: the
/// found release's, unless a person or the tags wrote a valid draft, which is
/// their answer and not judged by the verdict.
pub fn stated_folder_check(
    author: MetadataAuthor,
    draft_valid: bool,
    verdict: Option<&VerdictSummary>,
) -> Option<FolderCheck> {
    // Spelled out: who wrote the draft decides whether the verdict judges it.
    let answered = match author {
        MetadataAuthor::Person | MetadataAuthor::Prefill => draft_valid,
        MetadataAuthor::Identification | MetadataAuthor::Nobody => false,
    };
    if answered {
        return None;
    }
    verdict.and_then(VerdictSummary::folder_check)
}

/// What the last import of a candidate left in the tables.
///
/// The release wins: the failure row is written when an attempt fails and
/// cleared by the commit of the next one that lands, so a release this device
/// imported from this hash means an attempt already succeeded and any leftover
/// error is behind it.
///
/// The stored failure is here rather than only in the pane because a row has
/// to say it too. Without it, quitting after a failed import brings the
/// candidate back as an ordinary pending row, and the only way to find out it
/// failed is to open it. It stays in Pending either way — see
/// [`TriagePlacement::Failed`] — but as a row that says what went wrong.
pub fn import_status_of(
    imported: Option<&ImportedRelease>,
    blocked: Option<&crate::GroupingBlock>,
    failure: Option<&crate::ImportFailureReason>,
) -> Option<TriageImportStatus> {
    if let Some(release) = imported {
        return Some(TriageImportStatus::Complete { release: release.clone() });
    }
    if let Some(reason) = blocked {
        return Some(TriageImportStatus::Blocked { reason: reason.clone() });
    }
    failure.map(|failure| TriageImportStatus::Error { failure: failure.clone() })
}

/// Where a candidate stands, as its row reads it from the standings table.
/// How far a run has got, or how far through its step an import is, is not
/// part of it, and leaves the row as it was drawn. The default is a key
/// nothing is running for: no identification work exists and no import has
/// claimed it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TriageRuntimeFacts {
    pub identification: Option<IdentificationStatus>,
    /// The step of the import that owns this candidate right now, or `None`
    /// when no import does. How far through it the import is is the import
    /// percents' stream, read by the leaf that draws the bar.
    pub import: Option<crate::ImportStep>,
}

/// Where an import that owns a candidate stands: what a row draws for it and
/// what can be asked of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStanding {
    /// Claimed and waiting for the worker to take it up. A cancel drops it.
    Queued,
    /// Taken up by the worker and not yet writing its release. A cancel
    /// drops it.
    Running,
    /// Writing its release, which completes whatever is asked of it.
    Writing,
}

impl ImportStanding {
    /// Where an import at `step` stands.
    pub(crate) fn of_step(step: crate::ImportStep) -> Self {
        use crate::{ImportPhase, ImportStep, PrepareStep};
        match step {
            ImportStep::Preparing(PrepareStep::Queued) => Self::Queued,
            ImportStep::Preparing(
                PrepareStep::ValidatingSourceFiles | PrepareStep::FetchingArtistPictures,
            )
            | ImportStep::Running(ImportPhase::ReadingFiles | ImportPhase::MeasuringLoudness) => {
                Self::Running
            }
            ImportStep::Running(ImportPhase::Finalizing) => Self::Writing,
        }
    }
}

impl TriageRuntimeFacts {
    /// An import owns the candidate, whatever it has reached.
    pub fn importing(&self) -> bool {
        self.import.is_some()
    }

    /// Where the import that owns the candidate stands, from its step.
    pub fn import_standing(&self) -> Option<ImportStanding> {
        self.import.map(ImportStanding::of_step)
    }

    /// A run for the candidate is queued, running, or having its answer
    /// written, its picked release read first — something is still to come
    /// from it. A write that failed is over.
    pub fn identifying(&self) -> bool {
        matches!(
            self.identification,
            Some(
                IdentificationStatus::Queued
                    | IdentificationStatus::Running
                    | IdentificationStatus::Finalizing
                    | IdentificationStatus::ReadingPick { .. }
            )
        )
    }
}

#[cfg(test)]
#[path = "triage_tests.rs"]
mod tests;
