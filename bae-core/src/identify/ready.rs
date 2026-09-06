//! The Ready rule: whether a candidate's stored verdict is strong enough to
//! import in bulk without anyone looking at it.
//!
//! Derived on read, never stored. The rule's inputs move independently of the
//! verdict — another import landing flips a candidate from Ready to "already in
//! library" without its own verdict changing — so a cached classification would
//! go stale with nothing to invalidate it. See `plans/import-derived-state.md`.
//!
//! Nothing here blocks an import. Failing the rule means the candidate lands in
//! Needs you *with the disagreement named*, and importing it from there is one
//! click; the rule only decides what may be imported unattended.

use super::combine::ResultProvenance;
use super::verdict::TerminalVerdict;
use crate::db::LibraryStatus;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::MetadataSource;

/// How much the candidate's probed total may differ from the source's own
/// total and still count as agreement.
///
/// `500 ms × track_count`, with a `5 s` floor. What each part absorbs:
///
/// - **500 ms per track** is exactly the worst case when a source rounds each
///   track to whole seconds, which MusicBrainz entries transcribed from a
///   sleeve and every Discogs `duration` string do. Twelve tracks then permit
///   6 s; twenty permit 10 s.
/// - **The 5 s floor** covers what does not scale with track count: the
///   pre-gap of track one counted on one side and not the other (2 s on a
///   Red Book disc), and lossy encoder delay and padding.
///
/// What it deliberately does *not* absorb is a different edition. Editions
/// differ by whole tracks — a bonus track, a hidden track, a different mix —
/// and the shortest of those is tens of seconds, several times the widest
/// tolerance this yields for any realistic tracklist.
fn duration_tolerance_ms(track_count: u32) -> u64 {
    (500 * track_count as u64).max(5_000)
}

/// What the queue needs from the user for one candidate, derived from its
/// stored verdict. `Ready` is the only bulk-importable answer; every other
/// variant names the question being asked, which is what the sidebar groups by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueClassification {
    /// Exactly one pressing, not in the library, and the source agrees with
    /// the files on both track count and total length.
    Ready,
    NeedsYou(NeedsYou),
}

/// Why a candidate is not Ready — one variant per question the user is being
/// asked, carrying what it takes to state the disagreement on the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedsYou {
    /// One match, but it (or its album) is already in the library.
    AlreadyInLibrary,
    /// Several pressings matched; which one is on disk is the user's call.
    /// `count` is pressings, not result rows — the number of rows the list
    /// shows.
    SeveralMatches { count: u32 },
    /// Signals ran and matched nothing anywhere.
    NoMatch,
    /// Nothing to look up: no disc-ID artifact and no barcode source. Manual
    /// search is the only way forward.
    NothingToLookUp,
    /// An automatic provider lookup failed. A person may retry it explicitly.
    LookupFailed,
    /// The source's track count differs from the folder's.
    TrackCountDisagrees { local: u32, source: u32 },
    /// Both counts agree but the totals do not, beyond
    /// `duration_tolerance_ms`.
    DurationsDisagree {
        probed_ms: u64,
        source_ms: u64,
        tolerance_ms: u64,
    },
    /// The counts agree, but the source states no track lengths to check the
    /// total against. Not admitted unverified.
    SourceLengthsUnknown,
    /// The counts agree, but the candidate's own audio would not probe, so
    /// there is no local total to compare. Not admitted unverified.
    LocalDurationUnknown,
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

/// The match a `Found` verdict leads with, as its own columns.
///
/// One row of `import_candidate_match` at `list = 'found'`, `position = 0`.
/// Everything the queue asks of a verdict's matches is asked of this one: the
/// Ready rule consults the lead and nothing else (it only reaches the
/// tracklist comparison when the matches make a single pressing), and the row
/// leads with the lead's title, artist and cover whatever the count.
///
/// When they do make a single pressing this is also that pressing's own lead —
/// the release the documents were settled from, and so the only one carrying a
/// tracklist. A pressing holds at most one release per source and the first
/// match is the first MusicBrainz one, so a second MusicBrainz release would be
/// a second pressing and the rule would never have reached the tracklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadMatch {
    pub release_id: String,
    pub source: MetadataSource,
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub cover_thumbnail_url: Option<String>,
    pub source_tracks: Option<SourceTracks>,
    pub by_disc_id: bool,
    pub by_barcode: bool,
}

impl LeadMatch {
    /// The lead of a `Found` verdict's index-aligned match and provenance
    /// lists — also how the stored `position = 0` row reads back.
    pub(crate) fn of(result: &MetadataResult, provenance: Option<&ResultProvenance>) -> Self {
        Self {
            release_id: result.release_id.clone(),
            source: result.source,
            source_group_id: result.source_group_id.clone(),
            title: result.title.clone(),
            artist: result.artist.clone(),
            year: result.year,
            format: result.format.clone(),
            cover_thumbnail_url: result
                .cover_art
                .as_ref()
                .map(|cover: &RemoteCover| cover.thumbnail_url.clone()),
            source_tracks: result.source_tracks.clone(),
            by_disc_id: provenance.is_some_and(|provenance| provenance.by_disc_id),
            by_barcode: provenance.is_some_and(|provenance| provenance.by_barcode),
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
    /// How many physical pressings the `found` list names — its releases as
    /// [`crate::import::release_group::pressing_count`] groups them, so two
    /// sources' records of one pressing count once. Zero for the other shapes.
    pub pressing_count: u32,
    pub lead: Option<LeadMatch>,
}

impl VerdictSummary {
    pub fn of(verdict: &TerminalVerdict) -> Self {
        match verdict {
            TerminalVerdict::Found {
                matches,
                track_count,
                provenance,
                ..
            } => Self {
                kind: VerdictKind::Found,
                track_count: Some(*track_count),
                pressing_count: crate::import::release_group::pressing_count(matches.clone())
                    as u32,
                lead: matches
                    .first()
                    .map(|result| LeadMatch::of(result, provenance.first())),
            },
            TerminalVerdict::NotFoundAnywhere => Self {
                kind: VerdictKind::NotFound,
                track_count: None,
                pressing_count: 0,
                lead: None,
            },
            TerminalVerdict::ManualOnly { track_count } => Self {
                kind: VerdictKind::ManualOnly,
                track_count: Some(*track_count),
                pressing_count: 0,
                lead: None,
            },
            TerminalVerdict::Failed { track_count, .. } => Self {
                kind: VerdictKind::Failed,
                track_count: Some(*track_count),
                pressing_count: 0,
                lead: None,
            },
        }
    }
}

/// Classify one candidate.
///
/// `probed_total_duration_ms` is [`crate::signals::Signals`]' probed total, as
/// stored alongside the verdict; `0` means nothing was probed.
/// `library_statuses` is a **live** check of the verdict's matches, matched
/// back to them by release id (see `in_library`) — never a copy stored with
/// the verdict, which is the whole reason this is computed on read. Order and
/// completeness are not part of the contract: a caller batching one check
/// across a whole queue hands over what it resolved.
pub fn classify(
    verdict: &TerminalVerdict,
    probed_total_duration_ms: u64,
    library_statuses: &[LibraryStatus],
) -> QueueClassification {
    let summary = VerdictSummary::of(verdict);
    let lead_status = summary.lead.as_ref().and_then(|lead| {
        library_statuses
            .iter()
            .find(|status| status.release_id == lead.release_id)
    });
    classify_summary(&summary, probed_total_duration_ms, lead_status)
}

/// Classify one candidate from the columns its stored row holds.
///
/// `lead_status` is a **live** check of the lead match alone — the only match
/// the rule consults, because every other shape is answered before the
/// library is asked. `None` reads as "not in the library"; a caller that
/// cannot answer for the lead must fail its read rather than hand over `None`.
pub fn classify_summary(
    summary: &VerdictSummary,
    probed_total_duration_ms: u64,
    lead_status: Option<&LibraryStatus>,
) -> QueueClassification {
    let track_count = match summary.kind {
        VerdictKind::Found => summary.track_count.unwrap_or_default(),
        VerdictKind::NotFound => return QueueClassification::NeedsYou(NeedsYou::NoMatch),
        VerdictKind::ManualOnly => return QueueClassification::NeedsYou(NeedsYou::NothingToLookUp),
        VerdictKind::Failed => return QueueClassification::NeedsYou(NeedsYou::LookupFailed),
    };

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

    if lead_status.is_some_and(|status| status.release_in_library || status.album_in_library) {
        return QueueClassification::NeedsYou(NeedsYou::AlreadyInLibrary);
    }

    // `None` (nobody has asked the source yet) and `Nothing` (it answered and
    // listed no tracks) are different facts about the queue — one is waiting on
    // a lookup, the other is finished — but they ask the user the same
    // question, so they classify alike.
    let Some(SourceTracks::Listed {
        count,
        total_duration_ms,
    }) = &lead.source_tracks
    else {
        return QueueClassification::NeedsYou(NeedsYou::SourceLengthsUnknown);
    };

    if *count != track_count {
        return QueueClassification::NeedsYou(NeedsYou::TrackCountDisagrees {
            local: track_count,
            source: *count,
        });
    }

    // Totals, never per track. A continuous piece split differently between the
    // rip and the source, or a pre-gap counted into the previous track, changes
    // where the boundaries fall without changing how long the record plays —
    // and those are exactly the correct matches a per-track comparison would
    // wrongly demote. The mapping pane still shows both durations per row, so a
    // person reading the slots sees the divergence this ignores.
    let Some(source_ms) = *total_duration_ms else {
        return QueueClassification::NeedsYou(NeedsYou::SourceLengthsUnknown);
    };
    let probed_ms = probed_total_duration_ms;
    if probed_ms == 0 {
        return QueueClassification::NeedsYou(NeedsYou::LocalDurationUnknown);
    }
    let tolerance_ms = duration_tolerance_ms(track_count);
    if probed_ms.abs_diff(source_ms) > tolerance_ms {
        return QueueClassification::NeedsYou(NeedsYou::DurationsDisagree {
            probed_ms,
            source_ms,
            tolerance_ms,
        });
    }

    QueueClassification::Ready
}

#[cfg(test)]
mod tests;
