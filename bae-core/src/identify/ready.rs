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

use super::verdict::TerminalVerdict;
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceTracks};

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
    /// Exactly one match, not in the library, and the source agrees with the
    /// files on both track count and total length.
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
    SeveralMatches { count: u32 },
    /// The signals disagreed — an empty intersection, or results spanning
    /// several release groups.
    SignalsConflict,
    /// Signals ran and matched nothing anywhere.
    NoMatch,
    /// Nothing to look up: no disc-ID artifact and no barcode source. Manual
    /// search is the only way forward.
    NothingToLookUp,
    /// The source's track count differs from the folder's.
    TrackCountDisagrees { local: u32, source: u32 },
    /// Both counts agree but the totals do not, beyond
    /// [`duration_tolerance_ms`].
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

/// Classify one candidate.
///
/// `probed_total_duration_ms` is [`crate::signals::Signals`]' probed total, as
/// stored alongside the verdict; `0` means nothing was probed.
/// `library_statuses` is a **live** check of the verdict's matches, matched
/// back to them by release id (see [`in_library`]) — never a copy stored with
/// the verdict, which is the whole reason this is computed on read. Order and
/// completeness are not part of the contract: a caller batching one check
/// across a whole queue hands over what it resolved.
pub fn classify(
    verdict: &TerminalVerdict,
    probed_total_duration_ms: u64,
    library_statuses: &[LibraryStatus],
) -> QueueClassification {
    let (matches, track_count) = match verdict {
        TerminalVerdict::Found {
            matches,
            track_count,
            ..
        } => (matches, *track_count),
        TerminalVerdict::Conflict { .. } => {
            return QueueClassification::NeedsYou(NeedsYou::SignalsConflict)
        }
        TerminalVerdict::NotFoundAnywhere => {
            return QueueClassification::NeedsYou(NeedsYou::NoMatch)
        }
        TerminalVerdict::ManualOnly { .. } => {
            return QueueClassification::NeedsYou(NeedsYou::NothingToLookUp)
        }
    };

    // "An exact signal is not the same as a unique result" — a disc ID or a
    // barcode routinely returns several pressings of one release group, and
    // picking between them is the user's job.
    let [only_match] = matches.as_slice() else {
        return QueueClassification::NeedsYou(NeedsYou::SeveralMatches {
            count: matches.len() as u32,
        });
    };

    if in_library(only_match, library_statuses) {
        return QueueClassification::NeedsYou(NeedsYou::AlreadyInLibrary);
    }

    // `None` (nobody has asked the source yet) and `Nothing` (it answered and
    // listed no tracks) are different facts about the queue — one is waiting on
    // a lookup, the other is finished — but they ask the user the same
    // question, so they classify alike.
    let Some(SourceTracks::Listed {
        count,
        total_duration_ms,
    }) = &only_match.source_tracks
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

/// Whether the library already holds this match. Both levels count: holding the
/// very release is the obvious case, and holding another pressing of the same
/// album is still a decision — importing a second pressing is legitimate, but
/// not something to do to a hundred folders while nobody is looking.
fn in_library(result: &MetadataResult, statuses: &[LibraryStatus]) -> bool {
    statuses
        .iter()
        .find(|status| status.release_id == result.release_id)
        .is_some_and(|status| status.release_in_library || status.album_in_library)
}

#[cfg(test)]
mod tests;
