//! Signal extraction. A candidate's files carry *identifying signals* — the
//! data we use to look it up against external metadata and narrow the matches.
//! This module models those signals as one [`Signals`] value and (in
//! `service`) produces it in a single pass over the candidate.
//!
//! Three signal kinds, each its own module:
//!
//! * [`disc_id`] — a MusicBrainz disc ID from LOG/CUE artifacts.
//! * [`barcode`] — UPC/EAN codes from artwork (the bars, and the digits
//!   printed under them) and CUE `CATALOG`.
//! * [`text`] — catalog-number candidates and free text from artwork OCR,
//!   folder name, filenames, CUE, and text files.
//!
//! Embedded audio metadata (artist/album/year from tags) is deliberately NOT
//! a signal here: it isn't used to look up or narrow external matches. It
//! seeds the file-metadata import path instead.
//!
//! The identify pipeline consumes `Signals` (looking up the disc ID and
//! barcodes, narrowing by catalog number); the search UI surfaces the found
//! signals. Both read the same value.

// `failure` has no platform dependencies, so it stays available everywhere for
// the shared metadata-search path to map provider errors into. The rest is the
// desktop-only extraction machinery (artwork OCR, disc-ID compute), gated off
// mobile alongside the import pipeline that drives it.
pub mod failure;
pub use failure::LookupFailure;

// Where a value was read is plain data with no platform machinery behind it,
// and a release's marks carry it to every surface, mobile included — so it
// stays out of the desktop-only extraction block below.
pub mod origin;
pub use origin::{ImageRegion, SignalOrigin, SourcedValue, TextOrigin};

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
    pub mod service;
    pub mod text;

    pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer, DetectedBarcode, RecognizedLine};
    pub use artwork::ArtworkScan;
    pub use barcode::BarcodeSignal;
    pub use disc_id::DiscIdSignal;
    pub use service::{
        ExtractionService, ExtractionServiceHandle, ExtractionSource, ExtractionWatch,
        SignalsSnapshot,
    };
    pub use text::{TextLine, TextSignal};
}

/// The identifying signals extracted from one candidate's files. Produced by
/// the extraction pass as a stream of snapshots (signals settle as scanning
/// and OCR progress) and consumed by the identify pipeline and the search UI.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signals {
    pub disc_id: DiscIdSignal,
    pub barcode: BarcodeSignal,
    pub text: TextSignal,
    /// The candidate's own text, every line of it, in the order the pass read
    /// it. Not a lookup input like the three above — it narrows, the other
    /// half of what a signal is for: a result is ranked by how much of this
    /// agrees with the result's own fields, and one that nothing here agrees
    /// with is offered under "N more" rather than on the list. Nothing is
    /// extracted from it to look up.
    ///
    /// Empty for a re-identified library release until its artwork is read:
    /// there is no folder whose names and documents to gather.
    pub text_pool: Vec<TextLine>,
    /// What every one of the candidate's audio units plays for, read off the
    /// disk in the same pass the disc ID came from. Not a lookup input like
    /// the three above: settling a lead fits its tracklist to them.
    /// Not stored with the verdict — the scan already stores each file's
    /// facts, and a stored verdict's signals read back with none.
    ///
    /// Empty for a re-identified library release: there is no folder to walk.
    pub durations: crate::import::probe::SourceDurations,
}
