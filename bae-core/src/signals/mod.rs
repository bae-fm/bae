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
    pub use barcode::BarcodeSignal;
    pub use disc_id::DiscIdSignal;
    pub use rip::{CdProof, RipEvidence};
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
    /// What the files say about the medium the audio was ripped from; it
    /// decides whether a sheet is hashed and sets aside rows it contradicts.
    pub rip: RipEvidence,
    /// Whether every one of the candidate's audio files carries one channel.
    /// Not a lookup input: a row stating mono agrees with it, which breaks a
    /// tie between otherwise equal rows; a row stating stereo is not ruled
    /// out by it. Two channels are no evidence — a mono
    /// record is routinely ripped to two identical ones — so there is nothing
    /// to record about them.
    pub mono_audio: bool,
    pub disc_id: DiscIdSignal,
    pub barcode: BarcodeSignal,
    pub text: TextSignal,
    /// Every line of the candidate's own text, in reading order, which
    /// results are ranked against.
    pub text_pool: Vec<TextLine>,
    /// How long each audio unit plays, which a lead's tracklist is fitted
    /// to. Not stored with the verdict; empty for a library release.
    pub durations: crate::import::probe::SourceDurations,
}
