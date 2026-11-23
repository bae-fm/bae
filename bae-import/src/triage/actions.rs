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

    /// Every action one row offers by itself, in [`Self::ALL`]'s order:
    /// every action but Combine. Combine makes several rows one release, so
    /// only a selection of several rows offers it.
    pub const ROW: [CandidateAction; Self::ALL.len() - 1] = {
        let mut row = [CandidateAction::Import; Self::ALL.len() - 1];
        let mut from = 0;
        let mut to = 0;
        while from < Self::ALL.len() {
            if !matches!(Self::ALL[from], CandidateAction::Combine) {
                row[to] = Self::ALL[from];
                to += 1;
            }
            from += 1;
        }
        assert!(to == row.len(), "ALL holds Combine exactly once");
        row
    };
}

/// What the list's placement (`import_list_placement`) says of a candidate
/// beside its commands: whether it can be acted on at all, and where it is
/// placed.
///
/// The placement's columns decide the commands themselves, in SQL, with what
/// is running for the candidate right now, its standing: the read that places
/// the row composes the two into the row's [`CandidateLiveState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateActionBasis {
    pub actionable: bool,
    pub placement: TriagePlacement,
}

/// What is running for one candidate right now, and the commands its row
/// offers and the state it is in with it: the part of a row its standing
/// decides, read with the row from the standings table and the list's
/// placement, which counts the standing in the row's state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CandidateLiveState {
    pub facts: TriageRuntimeFacts,
    /// The commands the row offers by itself, apart from the ones a selection
    /// of several rows offers, from [`CandidateAction::ROW`].
    pub actions: Vec<CandidateAction>,
    /// Where the row stands among Found's states; `None` off Found.
    pub standing: Option<PendingStanding>,
}

impl CandidateLiveState {
    /// Builds the row's live state from the commands the row offers, the
    /// work running for it, and the state its placement gives it.
    pub fn of(
        actions: Vec<CandidateAction>,
        facts: TriageRuntimeFacts,
        standing: Option<PendingStanding>,
    ) -> Self {
        Self { actions, standing, facts }
    }

    /// What the row's own menu offers: each command the row offers, for that
    /// one row, each able to run.
    pub fn offers(&self) -> Vec<super::SelectionOffer> {
        row_offers(&self.actions)
    }
}

/// What can run for a list row and what the row offers while it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct TriageListLiveState {
    pub identification: Option<IdentificationStatus>,
    pub actions: Vec<CandidateAction>,
    pub standing: Option<PendingStanding>,
    pub status: Option<super::TriageRowStatus>,
}

impl TriageListLiveState {
    pub fn offers(&self) -> Vec<super::SelectionOffer> {
        row_offers(&self.actions)
    }

    /// Only rows counted as Identifying show lookup work.
    pub fn identifying(&self) -> Option<IdentifyingStep> {
        IdentifyingStep::of(self.standing.as_ref(), self.identification.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn import_step(&self) -> Option<crate::ImportStep> {
        match self.status {
            Some(super::TriageRowStatus::QueuedToImport) => {
                Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued))
            }
            Some(super::TriageRowStatus::Line {
                line: super::TriageStateLine::Importing { step },
            }) => Some(step),
            _ => None,
        }
    }
}

fn row_offers(actions: &[CandidateAction]) -> Vec<super::SelectionOffer> {
    actions
        .iter()
        .map(|&action| super::SelectionOffer { action, count: 1, enabled: true })
        .collect()
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

impl IdentifyingStep {
    fn of(
        standing: Option<&PendingStanding>,
        identification: Option<&IdentificationStatus>,
    ) -> Option<Self> {
        if standing != Some(&PendingStanding::Identifying) {
            return None;
        }
        Some(if identification == Some(&IdentificationStatus::Queued) {
            Self::Queued
        } else {
            Self::Running
        })
    }
}
