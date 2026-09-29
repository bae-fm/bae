//! Where each of Found's rows stands: exactly one state per row.
//!
//! Found is the Pending tab. The filter menu lists its [`PendingState`]s and
//! every row carries its own as a [`PendingStanding`], which also says why a
//! row waiting on the person is waiting.
//!
//! The tables decide most of it and what is running for the candidate the
//! rest. Where more than one could apply, the first of these wins:
//!
//! 1. **Importing** — an import is queued or running.
//! 2. **Identifying** — a run is queued, running, or writing its answer.
//! 3. **Error** — bae could not store a run's answer.
//! 4. **Import error** — the last import failed, or the release cannot be
//!    worked on as it stands.
//! 5. **Identified** — the draft is read from a catalog.
//! 6. **Not looked up** — no lookup is stored for the folder's files.
//! 7. **Unmatched** — the person kept their own draft over what the lookup
//!    offered.
//! 8. **Error** — bae broke on its own side and the stored run ended there.
//! 9. **Lookup error** — a catalog could not answer the stored lookup, or
//!    could not hand over a release it found in full.
//! 10. **Unmatched** — the lookup took its release for the folder and the
//!     draft is no longer read from it: the person set it aside.
//! 11. **Needs You** — the lookup left the answer to the person.
//!
//! A lookup error is a catalog's; an error is bae's own, which the row states
//! with its text.

use super::{IdentificationStatus, TriagePlacement, TriageRuntimeFacts};
use crate::identify::{Declined, FolderCheck, MediumConflict, VerdictKind, VerdictSummary};
use crate::import::MetadataProvenance;
use crate::signals::InternalFailure;

/// One state a Found row is in. Declared in the menu's order, which is the
/// order a set of them lists in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PendingState {
    NotLookedUp,
    Identifying,
    NeedsYou,
    Identified,
    Unmatched,
    LookupError,
    Error,
    Importing,
    ImportError,
}

impl PendingState {
    /// Every state, in the groups the menu sets apart and each group in the
    /// order it lists them: where identification stands, then where the
    /// import does. Each group ends with its failure.
    pub const GROUPS: [&'static [Self]; 2] = [
        &[
            Self::NotLookedUp,
            Self::Identifying,
            Self::NeedsYou,
            Self::Identified,
            Self::Unmatched,
            Self::LookupError,
            Self::Error,
        ],
        &[Self::Importing, Self::ImportError],
    ];

    pub(crate) fn every() -> impl Iterator<Item = Self> {
        Self::GROUPS.into_iter().flatten().copied()
    }
}

/// The state what is running for a candidate puts it in, over whatever the
/// tables say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveStanding {
    Identifying,
    /// bae could not store a run's answer.
    Error { failure: InternalFailure },
    Importing,
}

impl LiveStanding {
    /// `None` while nothing running decides the candidate's state.
    pub fn of(facts: &TriageRuntimeFacts) -> Option<Self> {
        if facts.importing() {
            return Some(Self::Importing);
        }
        match &facts.identification {
            Some(
                IdentificationStatus::Queued
                | IdentificationStatus::Running
                | IdentificationStatus::Finalizing,
            ) => Some(Self::Identifying),
            Some(IdentificationStatus::FinalizationFailed { failure }) => Some(Self::Error {
                failure: InternalFailure {
                    detail: failure.error().to_string(),
                },
            }),
            None => None,
        }
    }

    pub fn state(&self) -> PendingState {
        match self {
            Self::Identifying => PendingState::Identifying,
            Self::Error { .. } => PendingState::Error,
            Self::Importing => PendingState::Importing,
        }
    }
}

impl From<LiveStanding> for PendingStanding {
    fn from(live: LiveStanding) -> Self {
        match live {
            LiveStanding::Identifying => Self::Identifying,
            LiveStanding::Error { failure } => Self::Error { failure },
            LiveStanding::Importing => Self::Importing,
        }
    }
}

/// Where one Found row stands: its [`PendingState`], why a row that needs
/// the person does, and how bae broke for a row in error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingStanding {
    NotLookedUp,
    Identifying,
    NeedsYou { reason: NeedsYouReason },
    Identified,
    Unmatched,
    LookupError,
    Error { failure: InternalFailure },
    Importing,
    ImportError,
}

/// Why the lookup left a folder's answer to the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsYouReason {
    /// Several pressings could be the folder's: `count` of them.
    Matches { count: u32 },
    /// The one release found lists `source` tracks; the folder holds
    /// `local`.
    TrackCountMismatch { local: u32, source: u32 },
    /// The one release found lists no tracks, so whether it fits the folder
    /// was never checked.
    NoTracklist,
    /// The folder's own files rule out every release found, of which there
    /// are `releases`.
    MediumMismatch {
        folder: MediumConflict,
        releases: u32,
    },
    /// No catalog has the folder.
    NotFound,
    /// The folder has nothing to look it up by.
    NothingToLookUp,
}

impl PendingStanding {
    pub fn state(&self) -> PendingState {
        match self {
            Self::NotLookedUp => PendingState::NotLookedUp,
            Self::Identifying => PendingState::Identifying,
            Self::NeedsYou { .. } => PendingState::NeedsYou,
            Self::Identified => PendingState::Identified,
            Self::Unmatched => PendingState::Unmatched,
            Self::LookupError => PendingState::LookupError,
            Self::Error { .. } => PendingState::Error,
            Self::Importing => PendingState::Importing,
            Self::ImportError => PendingState::ImportError,
        }
    }

    /// Where the tables put a row placed at `placement`, whose draft was read
    /// from `provenance` and whose stored lookup came to `verdict`; `None`
    /// for a row off Found.
    pub(crate) fn stored(
        placement: TriagePlacement,
        provenance: Option<&MetadataProvenance>,
        verdict: Option<&VerdictSummary>,
    ) -> Option<Self> {
        match placement {
            TriagePlacement::Done | TriagePlacement::Skipped => return None,
            TriagePlacement::Failed => return Some(Self::ImportError),
            TriagePlacement::Pending => {}
        }
        let identified = match provenance {
            Some(MetadataProvenance::ExternalRelease { .. }) => true,
            Some(MetadataProvenance::FileMetadata) | None => false,
        };
        if identified {
            return Some(Self::Identified);
        }
        let Some(verdict) = verdict else {
            return Some(Self::NotLookedUp);
        };
        if verdict.kept_own_draft {
            return Some(Self::Unmatched);
        }
        let reason = match &verdict.kind {
            VerdictKind::Error { failure } => {
                return Some(Self::Error {
                    failure: failure.clone(),
                })
            }
            VerdictKind::Failed => return Some(Self::LookupError),
            VerdictKind::NotFound => NeedsYouReason::NotFound,
            VerdictKind::ManualOnly => NeedsYouReason::NothingToLookUp,
            VerdictKind::Found => match verdict.declined() {
                None => return Some(Self::Unmatched),
                Some(Declined::UnreadDocument) => return Some(Self::LookupError),
                Some(Declined::NothingFound) => NeedsYouReason::NotFound,
                Some(Declined::Several) => NeedsYouReason::Matches {
                    count: verdict.pressing_count,
                },
                Some(Declined::FolderCheck(check)) => match check {
                    FolderCheck::TrackCountDisagrees { local, source } => {
                        NeedsYouReason::TrackCountMismatch { local, source }
                    }
                    FolderCheck::SourceTracksUnknown => NeedsYouReason::NoTracklist,
                    FolderCheck::MediumDisagrees { folder } => NeedsYouReason::MediumMismatch {
                        folder,
                        releases: verdict.pressing_count,
                    },
                },
            },
        };
        Some(Self::NeedsYou { reason })
    }

    /// Where the row stands with `facts` running for its candidate.
    pub fn with_live(self, facts: &TriageRuntimeFacts) -> Self {
        match LiveStanding::of(facts) {
            Some(live) => live.into(),
            None => self,
        }
    }
}

#[cfg(test)]
#[path = "standing_tests.rs"]
mod tests;
