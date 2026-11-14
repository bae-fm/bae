use super::{IdentificationStatus, PendingStanding, TriagePlacement, TriageRuntimeFacts};

/// Commands offered for a candidate at its current lifecycle position —
/// every one a candidate can take, so every surface that acts on candidates
/// offers them from this one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateAction {
    /// Import the candidate from its draft, wherever Pending places it.
    Import,
    Identify,
    /// Stop the identification that is waiting, running, or being written.
    CancelIdentification,
    /// Stop the import that is waiting for the worker or running.
    CancelImport,
    RetryIdentification,
    ResetToFileMetadata,
    ClearMetadata,
    /// Read this folder together with others as one release. Offered by each
    /// candidate that could join one; a selection offers it once, over every
    /// member, and only when it holds two or more.
    Combine,
    /// Read this release as the folders it is made of.
    Separate,
    Skip,
    Restore,
    /// Show the candidate's folders where the platform keeps files.
    RevealFolder,
}

impl CandidateAction {
    /// Every action, in the order a surface lists them.
    pub const ALL: [CandidateAction; 12] = [
        CandidateAction::Import,
        CandidateAction::CancelImport,
        CandidateAction::Identify,
        CandidateAction::CancelIdentification,
        CandidateAction::RetryIdentification,
        CandidateAction::ResetToFileMetadata,
        CandidateAction::ClearMetadata,
        CandidateAction::Combine,
        CandidateAction::Separate,
        CandidateAction::Skip,
        CandidateAction::Restore,
        CandidateAction::RevealFolder,
    ];
}

/// What the tables say a candidate's commands are decided from: whether it can
/// be acted on at all, where it is placed, whether its draft would import, and
/// what its stored lookup came to — none offers identifying it, and a failed
/// one offers a retry whatever the draft over it says. The list's placement
/// holds each of them per row (`import_list_placement`).
///
/// The commands a row offers are these facts and what is running for it right
/// now, its standing: the read that places the row composes the two into the
/// row's [`CandidateLiveState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateActionBasis {
    pub actionable: bool,
    pub placement: TriagePlacement,
    /// Whether the draft shapes into a release an import can commit.
    pub draft_valid: bool,
    /// What the lookup stored for the candidate's current files came to, or
    /// `None` when none is stored.
    pub lookup: Option<StoredLookup>,
    /// Whether this release is folders a grouping reads as one, which the
    /// candidate offers to read as releases of their own.
    pub separable: bool,
}

/// What a candidate's stored lookup came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredLookup {
    /// It found a release, found none, or left the choice to the person.
    Answered,
    /// A source it asked could not answer, a release it found could not be
    /// read in full, or bae broke on its own side: another run may answer.
    Failed,
}

impl CandidateActionBasis {
    /// `lookup` is what the candidate's stored lookup came to, or `None` with
    /// none stored.
    #[cfg(test)]
    pub(crate) fn of(
        actionable: bool,
        placement: &TriagePlacement,
        draft_valid: bool,
        lookup: Option<StoredLookup>,
        separable: bool,
    ) -> Self {
        Self { actionable, placement: *placement, draft_valid, lookup, separable }
    }
}

/// What is running for one candidate right now, and the commands its row
/// offers and the state it is in with it: the part of a row its standing
/// decides, read with the row from the standings table and the list's
/// placement, which counts the standing in the row's state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CandidateLiveState {
    pub facts: TriageRuntimeFacts,
    pub actions: Vec<CandidateAction>,
    /// Where the row stands among Found's states; `None` off Found.
    pub standing: Option<PendingStanding>,
}

impl CandidateLiveState {
    /// The row's live state from the commands it offers, what is running for
    /// it, and the state the placement puts it in with that.
    pub fn of(
        actions: Vec<CandidateAction>,
        facts: TriageRuntimeFacts,
        standing: Option<PendingStanding>,
    ) -> Self {
        Self { actions, standing, facts }
    }

    /// The lookup at work the row shows: the work its state counts, so only a
    /// row Identifying shows one. A run reading the release it picked
    /// unattended leaves its row Identified, which shows none.
    pub fn identifying(&self) -> Option<IdentifyingStep> {
        if self.standing != Some(PendingStanding::Identifying) {
            return None;
        }
        // A row Identifying waits only while its lookup is queued.
        Some(if self.facts.identification == Some(IdentificationStatus::Queued) {
            IdentifyingStep::Queued
        } else {
            IdentifyingStep::Running
        })
    }

    /// Whether the release the run picked unattended is being read in full
    /// before its answer is stored, which the pane shows on that release. An
    /// answer that picks nothing reads no release.
    pub fn reading_pick(&self) -> bool {
        matches!(self.facts.identification, Some(IdentificationStatus::ReadingPick { .. }))
    }
}

/// How far the lookup of a row whose state is Identifying has got, which the
/// row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifyingStep {
    /// Waiting for its run to start.
    Queued,
    /// Its run is going, or writing an answer that picks no release.
    Running,
}
