//! Signal extraction: what a candidate's files say that identification looks
//! up and narrows by, produced as one [`Signals`] value per pass. Embedded tags
//! are not a signal; they seed the file-metadata import instead.

// `failure` and `origin` are plain data used on every platform; the rest is
// desktop-only extraction.
pub mod failure;
pub use failure::LookupFailure;

pub mod origin;
pub use origin::{SourcedValue, TextOrigin};

desktop_only! {
    mod analyzer;
    pub mod artwork;
    pub mod audio;
    pub mod barcode;
    mod cancellation;
    pub(crate) mod candidate_text;
    pub mod disc_id;
    mod fast_pass;
    mod pool;
    mod release;
    pub mod rip;
    pub mod service;
    pub mod text;

    pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
    pub use artwork::ArtworkScan;
    pub use audio::AudioFacts;
    pub use barcode::BarcodeSignal;
    pub use disc_id::DiscIdSignal;
    pub use rip::{AudioOrigin, AudioSource, CdProof, DownloadProof, StoreMarker};
    pub use service::{
        ExtractionService, ExtractionServiceHandle, ExtractionSource, ExtractionWatch,
        SignalsSnapshot,
    };
    pub use text::{TextLine, TextSignal};
}

/// The signals extracted from one candidate's files, streamed as snapshots
/// while the artwork is read.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    /// Where most of the audio's recordings were registered, as the ISRCs its
    /// tags carry say — see [`crate::isrc::registered_in`].
    pub registered_in: Option<crate::pressing::ReleaseArea>,
}
