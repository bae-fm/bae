//! Signal extraction: what a candidate's files say that identification looks
//! up and narrows by, produced as one [`Signals`] value per pass. Embedded tags
//! are not a signal; they seed the file-metadata import instead.

// `failure` and `origin` are plain data used on every platform; the rest is
// desktop-only extraction.
pub mod failure;
pub use failure::{Failure, InternalFailure, LookupFailure};

pub mod origin;
pub use origin::{SourcedValue, TextOrigin};

#[cfg(desktop)]
mod analyzer;
#[cfg(desktop)]
pub mod artwork;
#[cfg(desktop)]
pub mod audio;
#[cfg(desktop)]
pub mod barcode;
#[cfg(desktop)]
pub(crate) mod candidate_text;
#[cfg(desktop)]
pub mod disc_id;
#[cfg(desktop)]
mod fast_pass;
#[cfg(desktop)]
mod pool;
#[cfg(desktop)]
pub(crate) mod release;
#[cfg(desktop)]
pub mod rip;
#[cfg(desktop)]
pub mod service;
#[cfg(desktop)]
pub mod text;

#[cfg(desktop)]
pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
#[cfg(desktop)]
pub use artwork::ArtworkScan;
#[cfg(desktop)]
pub use audio::AudioFacts;
#[cfg(desktop)]
pub use barcode::BarcodeSignal;
#[cfg(desktop)]
pub use disc_id::{ComputedDiscId, DiscIdSignal};
#[cfg(desktop)]
pub use rip::{AudioOrigin, AudioSource, CdProof, DownloadProof, StoreMarker};
#[cfg(desktop)]
pub use service::{
    ExtractionService, ExtractionServiceHandle, ExtractionSource, ExtractionWatch, SignalsSnapshot,
};
#[cfg(desktop)]
pub use text::{TextLine, TextSignal};

/// The signals extracted from one candidate's files, streamed as snapshots
/// while the artwork is read.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signals {
    /// What the files say about where the audio came from; it decides
    /// whether a sheet is hashed and sets aside rows it contradicts.
    pub origin: AudioOrigin,
    pub disc_id: DiscIdSignal,
    pub barcode: BarcodeSignal,
    pub text: TextSignal,
    /// Every line of the candidate's own text, in reading order, which
    /// results are ranked against.
    pub text_pool: Vec<TextLine>,
    /// Each audio file's ISRC, as [`crate::isrc::code`] reads its tag, in the
    /// files' order; a file whose tags carry none adds nothing. They are
    /// looked up on MusicBrainz, and say where the recordings were registered
    /// (see [`crate::isrc::registered_in`]).
    pub isrcs: Vec<String>,
    /// Each track's title, in the tracks' order: its file's title tag, or
    /// failing that its file name without the track number. Empty unless
    /// every track is a file of its own, each has one, and the files' own
    /// numbers put them in that order.
    pub track_titles: Vec<String>,
}

#[cfg(desktop)]
impl Signals {
    /// How bae broke reading the folder's files, when it did: the disc ID's
    /// derivation first, then the artwork's reading, which fails the barcode
    /// and text signals together.
    pub fn failure(&self) -> Option<&InternalFailure> {
        match (&self.disc_id, &self.barcode, &self.text) {
            (DiscIdSignal::Failed { failure }, _, _)
            | (_, BarcodeSignal::Failed { failure, .. }, _)
            | (_, _, TextSignal::Failed { failure, .. }) => Some(failure),
            _ => None,
        }
    }
}
