//! The disc-ID signal: the MusicBrainz disc IDs derived from a candidate's
//! LOG/CUE artifacts — from a folder's own, or from a library release's, when
//! re-identifying.

use super::InternalFailure;

/// Derived once during the extraction pass. Identify looks every `Computed`
/// disc ID up on MusicBrainz; `Absent`, `NotCdAudio` and `Failed` settle the
/// signal with no results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdSignal {
    /// One disc ID per disc the candidate's logs and sheets describe, in the
    /// order they were read. Never empty.
    Computed { discs: Vec<ComputedDiscId> },
    /// No LOG/CUE artifact to derive one from.
    Absent,
    /// A track sheet was there, and the audio it lays out is sampled at a
    /// rate a CD does not play at (see [`super::AudioOrigin::not_cd_rate`]), so no
    /// disc could have had the layout the sheet describes and none is hashed
    /// to ask about.
    NotCdAudio,
    /// Derivation failed — a DB load, a "release not found", a compute task
    /// panic: bae broke reading the artifacts.
    Failed { failure: InternalFailure },
}

/// A disc ID and the file it was derived from — the rip log, or the sheet that
/// carves the tracks. The file rides along so a surface can put the disc ID on
/// the row for that file rather than beside the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedDiscId {
    pub disc_id: String,
    /// The candidate-relative path of the LOG or CUE it came from. `None`
    /// where the artifact is not a file of a scanned folder — a library
    /// release's own files, resolved for a re-identify pass, which no row
    /// points at.
    pub source_file: Option<String>,
}

impl DiscIdSignal {
    /// Every disc ID computed, in reading order; empty when none was.
    pub fn disc_ids(&self) -> Vec<String> {
        match self {
            DiscIdSignal::Computed { discs } => {
                discs.iter().map(|disc| disc.disc_id.clone()).collect()
            }
            DiscIdSignal::Absent | DiscIdSignal::NotCdAudio | DiscIdSignal::Failed { .. } => {
                Vec::new()
            }
        }
    }
}
