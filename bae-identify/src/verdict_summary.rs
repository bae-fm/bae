//! What the queue reads of a stored verdict, and the two judgements made from
//! it: which check against the folder the found release failed, which the pane
//! states beside Import and under the release in Find online, and whether the
//! verdict picks its one release unattended — applied to the draft as the run
//! settles, and taken by automatic import.
//!
//! Derived on read, never stored: the verdict's own columns are the whole
//! input. Whether the release is already in the library is not part of either —
//! importing a second copy is the person's to moderate. Nothing here blocks an
//! import a person asks for.

use super::fit::Declined;
use super::fit::TracklistFit;
use super::verdict::TerminalVerdict;
use super::MediumConflict;

/// A check of the found release against the folder that did not pass,
/// carrying what it takes to state the disagreement beside Import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderCheck {
    /// The release lists no tracks, so its count cannot be checked against
    /// the folder's. Not admitted unverified.
    SourceTracksUnknown,
    /// The folder's own files rule out every release found — a CD rip against
    /// no CD, or audio no CD holds against CDs alone — carrying what they
    /// prove. The releases are offered for a person to pick; none is picked
    /// for them.
    MediumDisagrees { folder: MediumConflict },
}

/// Which shape a stored verdict has, as its `kind` column says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerdictKind {
    Found,
    NotFound,
    ManualOnly,
    Failed,
    /// bae broke on its own side, and how.
    Error {
        failure: bae_catalogs::InternalFailure,
    },
}

/// As much of a stored verdict as the queue's list reads: which shape it is,
/// how many pressings it named, and how its lead match's tracklist fits the
/// folder.
///
/// The list reads these off the candidate's verdict row and its match rows
/// rather than rebuilding a [`TerminalVerdict`] — the pane, which shows the
/// failures and the matched barcode too, still reads the whole verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictSummary {
    pub kind: VerdictKind,
    /// How many physical pressings the `found` list names — the rows the run
    /// built, so two sources' records of one pressing count once. Zero for
    /// the shapes that hold no findings.
    pub pressing_count: u32,
    /// How the tracklist of the match the findings lead with, the first
    /// release they offer, fits the folder's track count; `None` when they
    /// offer none. The judgements consult only the lead: they reach the
    /// tracklist only when the matches make a single pressing, and then the
    /// lead is that pressing's own — the release its documents were settled
    /// from, and so the only one carrying a tracklist. The matches are stored
    /// in the order the pressings put them, so position 0 is that lead.
    pub lead: Option<TracklistFit>,
    /// The folder's own files rule out every release the verdict found.
    pub medium_conflict: Option<MediumConflict>,
    /// A record the `found` list names whose full document could not be
    /// read: what it states was never checked against the folder.
    pub unread_document: bool,
    /// The person kept their own draft over what the verdict offered.
    pub kept_own_draft: bool,
}

impl VerdictSummary {
    /// A stored verdict's summary, with whether the person kept their own
    /// draft over it.
    pub fn of(verdict: &TerminalVerdict, kept_own_draft: bool) -> Self {
        let kind = match verdict {
            TerminalVerdict::Found { .. } => VerdictKind::Found,
            TerminalVerdict::NotFoundAnywhere { .. } => VerdictKind::NotFound,
            TerminalVerdict::ManualOnly { .. } => VerdictKind::ManualOnly,
            TerminalVerdict::Failed { .. } => VerdictKind::Failed,
            TerminalVerdict::Error { failure } => VerdictKind::Error { failure: failure.clone() },
        };
        let track_count = match verdict {
            TerminalVerdict::Found { track_count, .. }
            | TerminalVerdict::ManualOnly { track_count, .. }
            | TerminalVerdict::Failed { track_count, .. } => Some(*track_count),
            TerminalVerdict::NotFoundAnywhere { .. } | TerminalVerdict::Error { .. } => None,
        };
        // A failed verdict leads with what its answering lookups found, as a
        // found one does; its kind is what keeps it from being auto-importable.
        Self::with_findings(kind, track_count, verdict.findings(), kept_own_draft)
    }

    /// A found verdict's summary, from what its lookups found and the
    /// folder's track count. It was just reached, so nobody has decided
    /// anything about it.
    pub(crate) fn of_found(findings: &super::Findings, track_count: u32) -> Self {
        Self::with_findings(VerdictKind::Found, Some(track_count), Some(findings), false)
    }

    fn with_findings(
        kind: VerdictKind,
        track_count: Option<u32>,
        findings: Option<&super::Findings>,
        kept_own_draft: bool,
    ) -> Self {
        Self {
            kept_own_draft,
            kind,
            pressing_count: findings
                .map_or(0, |findings| crate::release_group::row_count(&findings.pressings) as u32),
            lead: findings.and_then(|findings| findings.matches.first()).map(|lead| {
                TracklistFit::of(lead.source_tracks.as_ref(), track_count.unwrap_or_default())
            }),
            medium_conflict: findings.and_then(|findings| findings.medium_conflict),
            unread_document: findings.is_some_and(|findings| {
                findings.matches.iter().any(|result| result.document_failure.is_some())
            }),
        }
    }
}

impl VerdictSummary {
    /// Why a found verdict picks none of its releases unattended, by the one
    /// rule ([`super::fit::unattended_pick`]); `None` when it picks one, and for the
    /// shapes that hold no find.
    pub fn declined(&self) -> Option<Declined> {
        if self.kind != VerdictKind::Found {
            return None;
        }
        super::fit::decline(
            self.medium_conflict,
            self.pressing_count as usize,
            self.lead,
            self.unread_document,
        )
    }

    /// The check against the folder the found release did not pass, when one
    /// failed: why a found verdict picks nothing, when a check is why.
    pub fn folder_check(&self) -> Option<FolderCheck> {
        match self.declined() {
            Some(Declined::FolderCheck(check)) => Some(check),
            Some(Declined::NothingFound | Declined::Several | Declined::UnreadDocument) | None => {
                None
            }
        }
    }

    /// Whether the verdict picks its one release without a person — see
    /// [`super::fit::unattended_pick`].
    pub fn picks_unattended(&self) -> bool {
        self.kind == VerdictKind::Found && self.declined().is_none()
    }

    /// Both judgements at once, for a test to compare in one assertion.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn judgement(&self) -> (bool, Option<FolderCheck>) {
        (self.picks_unattended(), self.folder_check())
    }
}

#[cfg(test)]
#[path = "verdict_summary_tests.rs"]
mod tests;
