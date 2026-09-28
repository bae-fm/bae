//! The disc-ID signal: a MusicBrainz disc ID derived from a candidate's LOG/CUE
//! artifacts — from a folder's own, or from a library release's, when re-identifying.

use super::LookupFailure;

/// Derived once during the extraction pass. Identify turns a `Computed` disc ID into
/// a MusicBrainz lookup; `Absent`, `NotCdAudio` and `Failed` settle the signal with
/// no results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdSignal {
    /// A disc ID was derived from a LOG/CUE artifact.
    Computed {
        disc_id: String,
        /// The candidate-relative path of the LOG or CUE it came from, so a
        /// surface can put the disc ID on that file's row. `None` for a
        /// re-identify pass over a library release, which derives it from
        /// stored tracks rather than a file of a scanned folder.
        source_file: Option<String>,
    },
    /// No LOG/CUE artifact to derive one from.
    Absent,
    /// A track sheet was there, and the audio it lays out is sampled at a
    /// rate a CD does not play at (see [`super::AudioOrigin::not_cd_rate`]), so no
    /// disc could have had the layout the sheet describes and none is hashed
    /// to ask about.
    NotCdAudio,
    /// Derivation failed — a DB load, a "release not found", a compute task panic.
    /// Always local, so always a `LookupFailure::Diagnostic` in practice.
    Failed { failure: LookupFailure },
}

impl DiscIdSignal {
    /// The hash when one was computed — the toolbar badge's value.
    pub fn discid_value(&self) -> Option<String> {
        match self {
            DiscIdSignal::Computed { disc_id, .. } => Some(disc_id.clone()),
            DiscIdSignal::Absent
            | DiscIdSignal::NotCdAudio
            | DiscIdSignal::Failed { .. } => None,
        }
    }
}
