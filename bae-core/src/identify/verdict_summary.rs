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

use super::combine::LookupProvenance;
use super::fit::Declined;
use super::verdict::TerminalVerdict;
use super::MediumConflict;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::Catalog;

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
        failure: crate::signals::InternalFailure,
    },
}

/// The match a verdict's findings lead with, as its own columns.
///
/// One row of `import_candidate_match` at `list = 'found'`, `position = 0`.
/// Everything the queue asks of a verdict's matches is asked of this one, but
/// whether each was read in full: the judgements consult the lead (they only
/// reach the tracklist comparison when the matches make a single pressing),
/// and the row leads with the lead's title, artist and cover whatever the
/// count.
///
/// When they do make a single pressing this is also that pressing's own lead —
/// the release the documents were settled from, and so the only one carrying a
/// tracklist. The matches are stored in the order the pressings put them, so
/// position 0 is that lead; a pressing holds at most one release per source,
/// so a second release from the same source would be a second pressing and the
/// check would never have reached the tracklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadMatch {
    pub release_id: String,
    pub source: Catalog,
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub media: Vec<crate::pressing::MediaCount>,
    /// The lead match's cover, with the copies its catalog serves.
    pub cover: Option<crate::import::cover_art::RemoteImageSet>,
    pub source_tracks: Option<SourceTracks>,
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_isrc: bool,
    pub by_search: bool,
}

impl LeadMatch {
    /// The lead of a verdict's findings, from their index-aligned match and
    /// provenance lists — also how the stored `position = 0` row reads back.
    pub(crate) fn of(result: &MetadataResult, provenance: Option<&LookupProvenance>) -> Self {
        Self {
            release_id: result.release_id.clone(),
            source: result.source,
            source_group_id: result.source_group_id.clone(),
            title: result.title.clone(),
            artist: result.artist.clone(),
            year: result.year,
            media: result.media.counts(),
            cover: result
                .cover_art
                .as_ref()
                .map(|cover: &RemoteCover| cover.image.clone()),
            source_tracks: result.source_tracks.clone(),
            by_disc_id: provenance.is_some_and(|provenance| provenance.by_disc_id),
            by_barcode: provenance.is_some_and(|provenance| provenance.by_barcode),
            by_isrc: provenance.is_some_and(|provenance| provenance.by_isrc),
            by_search: provenance.is_some_and(|provenance| provenance.by_search),
        }
    }
}

/// As much of a stored verdict as the queue's list reads: which shape it is,
/// how many pressings it named, and the lead match's own columns.
///
/// The list reads these off the candidate's verdict row and its match rows
/// rather than rebuilding a [`TerminalVerdict`] — the pane, which shows the
/// failures and the matched barcode too, still reads the whole verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictSummary {
    pub kind: VerdictKind,
    /// The folder's own track count, as identification counted it. `None` for
    /// `NotFound`, which counts nothing, and for `Error`, which ended first.
    pub track_count: Option<u32>,
    /// How many physical pressings the `found` list names — the rows the run
    /// built, so two sources' records of one pressing count once. Zero for
    /// the shapes that hold no findings.
    pub pressing_count: u32,
    pub lead: Option<LeadMatch>,
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
            TerminalVerdict::Error { failure } => VerdictKind::Error {
                failure: failure.clone(),
            },
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
            track_count,
            pressing_count: findings.map_or(0, |findings| {
                crate::import::release_group::row_count(&findings.pressings) as u32
            }),
            lead: findings.and_then(|findings| {
                findings
                    .matches
                    .first()
                    .map(|result| LeadMatch::of(result, findings.provenance.first()))
            }),
            medium_conflict: findings.and_then(|findings| findings.medium_conflict),
            unread_document: findings.is_some_and(|findings| {
                findings
                    .matches
                    .iter()
                    .any(|result| result.document_failure.is_some())
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
        let track_count = self.track_count.unwrap_or_default();
        super::fit::decline(
            self.medium_conflict,
            self.pressing_count as usize,
            self.lead
                .as_ref()
                .map(|lead| super::fit::TracklistFit::of(lead.source_tracks.as_ref(), track_count)),
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
    #[cfg(test)]
    pub(crate) fn judgement(&self) -> (bool, Option<FolderCheck>) {
        (self.picks_unattended(), self.folder_check())
    }
}

#[cfg(test)]
mod tests;
