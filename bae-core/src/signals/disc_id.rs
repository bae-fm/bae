//! The disc-ID signal: a MusicBrainz disc ID derived from a candidate's
//! LOG/CUE artifacts, or supplied directly for a physical CD.

use super::LookupFailure;

/// The disc-ID signal extracted from a candidate's files.
///
/// Derived once during the extraction pass (or supplied for a CD). The
/// identify pipeline turns a `Computed` disc ID into a MusicBrainz lookup;
/// `Absent`/`Failed` settle the disc-ID pipe with no results. `track_count`
/// is the candidate's local count, carried on every variant so the barcode
/// lookup can report "N tracks here vs. M on the matched release."
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdSignal {
    /// A disc ID was derived from a LOG/CUE artifact (or supplied for a CD).
    Computed { disc_id: String, track_count: u32 },
    /// No LOG/CUE artifact to derive a disc ID from.
    Absent { track_count: u32 },
    /// Derivation failed (re-identify resolution: DB load, "release not
    /// found", a disc-ID compute task panic) — always a local error, hence
    /// `LookupFailure::Diagnostic` in practice.
    Failed {
        failure: LookupFailure,
        track_count: u32,
    },
}

impl DiscIdSignal {
    /// The candidate's local track count, regardless of outcome.
    pub fn track_count(&self) -> u32 {
        match self {
            DiscIdSignal::Computed { track_count, .. }
            | DiscIdSignal::Absent { track_count }
            | DiscIdSignal::Failed { track_count, .. } => *track_count,
        }
    }

    /// The disc-ID hash when one was computed; `None` otherwise. The toolbar
    /// disc-ID badge shows this as its value.
    pub fn discid_value(&self) -> Option<String> {
        match self {
            DiscIdSignal::Computed { disc_id, .. } => Some(disc_id.clone()),
            DiscIdSignal::Absent { .. } | DiscIdSignal::Failed { .. } => None,
        }
    }
}
