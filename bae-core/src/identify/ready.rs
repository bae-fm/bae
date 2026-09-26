//! The Ready rule: whether a candidate's stored verdict is strong enough to
//! import in bulk without anyone looking at it.
//!
//! Derived on read, never stored: the verdict's own columns are the rule's
//! whole input. Whether the release is already in the library is not part of
//! it — importing a second copy is the person's to moderate.
//!
//! Nothing here blocks an import. Failing the rule means the candidate lands in
//! Needs you *with the disagreement named*, and importing it from there is one
//! click; the rule only decides what may be imported unattended.

use super::combine::LookupProvenance;
use super::verdict::TerminalVerdict;
use super::MediumConflict;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::Catalog;

/// What the queue needs from the user for one candidate, derived from its
/// stored verdict. `Ready` is the only bulk-importable answer; every other
/// variant names the question being asked, which is what the sidebar groups by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueClassification {
    /// Exactly one pressing, and the source lists as many tracks as the
    /// folder holds.
    Ready,
    NeedsYou(NeedsYou),
}

/// Why a candidate is not Ready — one variant per question the user is being
/// asked, carrying what it takes to state the disagreement on the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedsYou {
    /// Several pressings matched; which one is on disk is the user's call.
    /// `count` is pressings, not result rows — the number of rows the list
    /// shows.
    SeveralMatches { count: u32 },
    /// Signals ran and matched nothing anywhere.
    NoMatch,
    /// Nothing to look up: no disc-ID artifact and no barcode source, or the
    /// lookups there were are switched off. Manual search is the only way
    /// forward.
    NothingToLookUp,
    /// An automatic provider lookup failed. A person may retry it explicitly.
    LookupFailed,
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

/// Which question a [`NeedsYou`] asks, without what it carries to state it:
/// what a list is filtered by, where the row's operands do not matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NeedsYouKind {
    SeveralMatches,
    NoMatch,
    NothingToLookUp,
    LookupFailed,
    TrackCountDisagrees,
    SourceTracksUnknown,
}

impl NeedsYou {
    pub fn kind(&self) -> NeedsYouKind {
        match self {
            NeedsYou::SeveralMatches { .. } => NeedsYouKind::SeveralMatches,
            NeedsYou::NoMatch => NeedsYouKind::NoMatch,
            NeedsYou::NothingToLookUp => NeedsYouKind::NothingToLookUp,
            NeedsYou::LookupFailed => NeedsYouKind::LookupFailed,
            NeedsYou::TrackCountDisagrees { .. } => NeedsYouKind::TrackCountDisagrees,
            NeedsYou::SourceTracksUnknown => NeedsYouKind::SourceTracksUnknown,
        }
    }
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
/// Ready rule consults the lead and nothing else (it only reaches the
/// tracklist comparison when the matches make a single pressing), and the row
/// leads with the lead's title, artist and cover whatever the count.
///
/// When they do make a single pressing this is also that pressing's own lead —
/// the release the documents were settled from, and so the only one carrying a
/// tracklist. The matches are stored in the order the pressings put them, so
/// position 0 is that lead; a pressing holds at most one release per source,
/// so a second release from the same source would be a second pressing and the
/// rule would never have reached the tracklist.
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
        // found one does; its kind is what keeps it from being Ready.
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

/// Classify one candidate.
pub fn classify(verdict: &TerminalVerdict) -> QueueClassification {
    classify_summary(&VerdictSummary::of(verdict))
}

/// Classify one candidate from the columns its stored row holds.
pub fn classify_summary(summary: &VerdictSummary) -> QueueClassification {
    let track_count = match summary.kind {
        VerdictKind::Found => summary.track_count.unwrap_or_default(),
        VerdictKind::NotFound => return QueueClassification::NeedsYou(NeedsYou::NoMatch),
        VerdictKind::ManualOnly => return QueueClassification::NeedsYou(NeedsYou::NothingToLookUp),
        VerdictKind::Failed => return QueueClassification::NeedsYou(NeedsYou::LookupFailed),
    };

    // A release the folder's own files rule out is never picked unattended,
    // however well the lookups agree on it: the person reads the evidence and
    // picks, or does not.
    if let Some(folder) = summary.medium_conflict {
        return QueueClassification::NeedsYou(NeedsYou::MediumDisagrees { folder });
    }

    // "An exact signal is not the same as a unique result" — a disc ID or a
    // barcode routinely returns several pressings of one release group, and
    // picking between them is the user's job. Two sources' records of the same
    // pressing are not that choice: they are one row on the list, picked whole,
    // so they count once here and the candidate is still answered.
    let (Some(lead), 1) = (summary.lead.as_ref(), summary.pressing_count) else {
        return QueueClassification::NeedsYou(NeedsYou::SeveralMatches {
            count: summary.pressing_count,
        });
    };

    // `None` (nobody has asked the source yet) and `Nothing` (it answered and
    // listed no tracks) are different facts about the queue — one is waiting on
    // a lookup, the other is finished — but they ask the user the same
    // question, so they classify alike.
    let Some(SourceTracks::Listed { count }) = &lead.source_tracks else {
        return QueueClassification::NeedsYou(NeedsYou::SourceTracksUnknown);
    };

    // The count, never the lengths. A source's lengths are whatever it
    // transcribed — rounded to whole seconds, counted with or without a
    // pre-gap, missing for some tracks — so disagreeing lengths say more about
    // the source than about the match. The mapping pane shows both durations
    // per row for a person who wants to read them.
    if *count != track_count {
        return QueueClassification::NeedsYou(NeedsYou::TrackCountDisagrees {
            local: track_count,
            source: *count,
        });
    }

    QueueClassification::Ready
}

#[cfg(test)]
mod tests;
