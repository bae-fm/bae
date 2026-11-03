//! What a candidate's files say that identification looks up and narrows by,
//! as one [`Signals`] value per extraction pass. Embedded tags are not a
//! signal; they seed the file-metadata import instead. Import's extraction
//! reads them off the files.

pub mod artwork;
pub mod audio;
pub mod barcode;
pub mod disc_id;
pub mod origin;
pub mod text;

pub use artwork::ArtworkScan;
pub use audio::AudioFacts;
pub use barcode::BarcodeSignal;
pub use disc_id::{ComputedDiscId, DiscIdSignal};
pub use origin::{SourcedValue, TextOrigin};
pub use text::{TextLine, TextSignal};

use bae_catalogs::{AudioOrigin, InternalFailure};

/// The signals extracted from one candidate's files, streamed as snapshots
/// while the artwork is read.
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
    /// Each audio file's ISRC, as [`bae_catalogs::isrc::code`] reads its tag, in the
    /// files' order; a file whose tags carry none adds nothing. They are
    /// looked up on MusicBrainz, and say where the recordings were registered
    /// (see [`bae_catalogs::isrc::registered_in`]).
    pub isrcs: Vec<String>,
    /// Each track's title, in the tracks' order: its file's title tag, or
    /// failing that its file name without the track number. Empty unless
    /// every track is a file of its own, each has one, and the files' own
    /// numbers put them in that order.
    pub track_titles: Vec<String>,
}

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
