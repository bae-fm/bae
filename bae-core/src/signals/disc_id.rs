//! The disc-ID signal: a MusicBrainz disc ID derived from a candidate's LOG/CUE
//! artifacts — from a folder's own, or from a library release's, when re-identifying.

use super::LookupFailure;

/// Derived once during the extraction pass. Identify turns a `Computed` disc ID into
/// a MusicBrainz lookup; `Absent` and `Failed` settle the signal with no results.
///
/// `track_count` is the candidate's own count and rides every variant, so a barcode
/// match can still report "N tracks here vs. M on the matched release."
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdSignal {
    /// A disc ID was derived from a LOG/CUE artifact.
    Computed { disc_id: String, track_count: u32 },
    /// No LOG/CUE artifact to derive one from.
    Absent { track_count: u32 },
    /// Derivation failed — a DB load, a "release not found", a compute task panic.
    /// Always local, so always a `LookupFailure::Diagnostic` in practice.
    Failed {
        failure: LookupFailure,
        track_count: u32,
    },
}

impl DiscIdSignal {
    pub fn track_count(&self) -> u32 {
        match self {
            DiscIdSignal::Computed { track_count, .. }
            | DiscIdSignal::Absent { track_count }
            | DiscIdSignal::Failed { track_count, .. } => *track_count,
        }
    }

    /// The hash when one was computed — the toolbar badge's value.
    pub fn discid_value(&self) -> Option<String> {
        match self {
            DiscIdSignal::Computed { disc_id, .. } => Some(disc_id.clone()),
            DiscIdSignal::Absent { .. } | DiscIdSignal::Failed { .. } => None,
        }
    }
}
