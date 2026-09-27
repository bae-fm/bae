//! What the queue reads of a stored verdict, and the two judgements made from
//! it: which check against the folder the found release failed, which the pane
//! states beside Import, and whether automatic import may take the candidate.
//!
//! Derived on read, never stored: the verdict's own columns are the whole
//! input. Whether the release is already in the library is not part of either —
//! importing a second copy is the person's to moderate. Nothing here blocks an
//! import a person asks for.

use super::combine::LookupProvenance;
use super::verdict::TerminalVerdict;
use super::MediumConflict;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::Catalog;

/// A check of the found release against the folder that did not pass,
/// carrying what it takes to state the disagreement beside Import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderCheck {
    /// The source's track count differs from the folder's.
    TrackCountDisagrees { local: u32, source: u32 },
    /// The release lists no tracks, so its count cannot be checked against
    /// the folder's. Not admitted unverified.
    SourceTracksUnknown,
    /// The folder's own files rule out every release found — a CD rip against
    /// no CD, or audio no CD holds against CDs alone — carrying what they
    /// prove. The releases are offered for a person to pick; none is picked
    /// for them.
    MediumDisagrees { folder: MediumConflict },
}

/// Which shape a stored verdict has. The first three mirror the normal verdict
/// column; `Failed` is the attached failed-verdict row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictKind {
    Found,
    NotFound,
    ManualOnly,
    Failed,
}

/// The match a verdict's findings lead with, as its own columns.
///
/// One row of `import_candidate_match` at `list = 'found'`, `position = 0`.
/// Everything the queue asks of a verdict's matches is asked of this one: the
/// judgements consult the lead and nothing else (they only reach the
/// tracklist comparison when the matches make a single pressing), and the row
/// leads with the lead's title, artist and cover whatever the count.
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
    /// `NotFound`, which counts nothing.
    pub track_count: Option<u32>,
    /// How many physical pressings the `found` list names — the rows the run
    /// built, so two sources' records of one pressing count once. Zero for
    /// the shapes that hold no findings.
    pub pressing_count: u32,
    pub lead: Option<LeadMatch>,
    /// The folder's own files rule out every release the verdict found.
    pub medium_conflict: Option<MediumConflict>,
}

impl VerdictSummary {
    pub fn of(verdict: &TerminalVerdict) -> Self {
        let kind = match verdict {
            TerminalVerdict::Found { .. } => VerdictKind::Found,
            TerminalVerdict::NotFoundAnywhere { .. } => VerdictKind::NotFound,
            TerminalVerdict::ManualOnly { .. } => VerdictKind::ManualOnly,
            TerminalVerdict::Failed { .. } => VerdictKind::Failed,
        };
        let track_count = match verdict {
            TerminalVerdict::Found { track_count, .. }
            | TerminalVerdict::ManualOnly { track_count, .. }
            | TerminalVerdict::Failed { track_count, .. } => Some(*track_count),
            TerminalVerdict::NotFoundAnywhere { .. } => None,
        };
        // A failed verdict leads with what its answering lookups found, as a
        // found one does; its kind is what keeps it from being auto-importable.
        let findings = verdict.findings();
        Self {
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
        }
    }
}

impl VerdictSummary {
    /// The check against the folder the found release did not pass, when one
    /// failed. Only a found verdict has a release to check.
    pub fn folder_check(&self) -> Option<FolderCheck> {
        if self.kind != VerdictKind::Found {
            return None;
        }
        let track_count = self.track_count.unwrap_or_default();

        // A release the folder's own files rule out is never picked unattended,
        // however well the lookups agree on it: the person reads the evidence
        // and picks, or does not. Checked before the pressing count, so it is
        // named over several pressings too.
        if let Some(folder) = self.medium_conflict {
            return Some(FolderCheck::MediumDisagrees { folder });
        }

        // Several pressings are a choice, not a failed check: the tracklist is
        // only compared once the matches make a single pressing.
        let (Some(lead), 1) = (self.lead.as_ref(), self.pressing_count) else {
            return None;
        };

        // `None` (nobody has asked the source yet) and `Nothing` (it answered
        // and listed no tracks) are different facts about the queue, but they
        // leave the same count unchecked, so they fail alike.
        let Some(SourceTracks::Listed { count }) = &lead.source_tracks else {
            return Some(FolderCheck::SourceTracksUnknown);
        };

        // The count, never the lengths. A source's lengths are whatever it
        // transcribed — rounded to whole seconds, counted with or without a
        // pre-gap, missing for some tracks — so disagreeing lengths say more
        // about the source than about the match. The mapping pane shows both
        // durations per row for a person who wants to read them.
        (*count != track_count).then_some(FolderCheck::TrackCountDisagrees {
            local: track_count,
            source: *count,
        })
    }

    /// Whether automatic import may take the candidate unattended. Auto-
    /// importable means unambiguously identified given the information we
    /// have: one pressing found, and no check against the folder failed.
    ///
    /// "An exact signal is not the same as a unique result" — a disc ID or a
    /// barcode routinely returns several pressings of one release group, and
    /// picking between them is the person's job. Two sources' records of the
    /// same pressing are not that choice: they are one row, so they count once.
    pub fn auto_importable(&self) -> bool {
        self.kind == VerdictKind::Found
            && self.pressing_count == 1
            && self.lead.is_some()
            && self.folder_check().is_none()
    }

    /// Both judgements at once, for a test to compare in one assertion.
    #[cfg(test)]
    pub(crate) fn judgement(&self) -> (bool, Option<FolderCheck>) {
        (self.auto_importable(), self.folder_check())
    }
}

#[cfg(test)]
mod tests;
