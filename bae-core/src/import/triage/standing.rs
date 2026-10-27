//! Where each of Found's rows stands: exactly one state per row.
//!
//! Found is the Pending tab. Every row carries its [`PendingState`] as a
//! [`PendingStanding`], which also says why a row waiting on the person is
//! waiting, and each entry of the filter menu
//! ([`PendingFilter`](crate::import::PendingFilter)) holds the rows in the
//! states it covers. Every row its Needs You entry holds wears one badge
//! saying why ([`PendingStanding::badge`]), and no other row wears any.
//!
//! The list's placement decides each row's state (`import_list_entry`, in the
//! schema), from the tables and what is running for the candidate. Where more
//! than one could apply, the first of these wins:
//!
//! 1. **Importing** — an import is queued or running.
//! 2. **Identifying** — a run is queued, running, or writing an answer that
//!    picks no release; one whose run picked its release unattended is
//!    **Identified** while that release is read in full.
//! 3. **Error** — bae could not store a run's answer.
//! 4. **Import error** — the last import failed, or the release cannot be
//!    worked on as it stands.
//! 5. **Identified** — the candidate is linked to a catalog pressing, or to
//!    an album whose pressing is unknown.
//! 6. **Not looked up** — no lookup is stored for the folder's files.
//! 7. **Unmatched** — the person kept their own draft over what the lookup
//!    offered.
//! 8. **Error** — bae broke on its own side and the stored run ended there.
//! 9. **Lookup error** — a catalog could not answer the stored lookup, or
//!    could not hand over a release it found in full.
//! 10. **Unmatched** — the lookup took its release for the folder and the
//!     candidate is no longer linked to it: the person set it aside.
//! 11. **Needs You** — the lookup left the answer to the person.
//!
//! A lookup error is a catalog's; an error is bae's own, which the row states
//! with its text.

use crate::identify::MediumConflict;
use crate::signals::InternalFailure;

/// One state a Found row is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    /// The one badge the row wears: a row waiting on the person has one,
    /// saying why, and no other row has any.
    pub fn badge(&self) -> Option<PendingBadge> {
        match self {
            Self::NeedsYou { reason } => Some(PendingBadge::NeedsYou { reason: *reason }),
            Self::LookupError => Some(PendingBadge::LookupError),
            Self::Error { .. } => Some(PendingBadge::Error),
            Self::ImportError => Some(PendingBadge::ImportError),
            Self::NotLookedUp
            | Self::Identifying
            | Self::Identified
            | Self::Unmatched
            | Self::Importing => None,
        }
    }
}

/// What the badge of a Found row waiting on the person says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingBadge {
    /// The lookup left the answer to the person, for this reason.
    NeedsYou { reason: NeedsYouReason },
    /// A catalog could not answer the lookup, or hand over a release it found
    /// in full.
    LookupError,
    /// bae broke on its own side.
    Error,
    /// The last import failed, or the release cannot be worked on as it
    /// stands.
    ImportError,
}

/// How a badge reads at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    /// An answer is the person's to give.
    Attention,
    /// Something failed.
    Failure,
}

impl PendingBadge {
    pub fn tone(self) -> BadgeTone {
        match self {
            Self::NeedsYou { .. } => BadgeTone::Attention,
            Self::LookupError | Self::Error | Self::ImportError => BadgeTone::Failure,
        }
    }
}

#[cfg(test)]
#[path = "standing_tests.rs"]
mod tests;
