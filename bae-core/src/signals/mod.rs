//! Signal extraction. A candidate's files carry *identifying signals* — the
//! data we use to look it up against external metadata and narrow the matches.
//! This module models those signals as one [`Signals`] value and (in
//! `service`) produces it in a single pass over the candidate.
//!
//! Three signal kinds, each its own module:
//!
//! * [`disc_id`] — a MusicBrainz disc ID from LOG/CUE artifacts.
//! * [`barcode`] — UPC/EAN codes from artwork OCR and CUE `CATALOG`.
//! * [`text`] — catalog-number candidates and free text from artwork OCR,
//!   folder name, filenames, CUE, and text files.
//!
//! Embedded audio metadata (artist/album/year from tags) is deliberately NOT
//! a signal here: it isn't used to look up or narrow external matches. It
//! seeds the "Add as Unknown" import path instead.
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod analyzer;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod barcode;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod cancellation;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod candidate_text;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod disc_id;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod fast_pass;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod origin;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod pool;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod release;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod service;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod text;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use barcode::{is_placeholder_code, BarcodeSignal};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use disc_id::DiscIdSignal;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use origin::{SignalOrigin, SourcedValue};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use service::{ExtractionService, ExtractionServiceHandle, ExtractionSource};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use text::TextSignal;

/// The identifying signals extracted from one candidate's files. Produced by
/// the extraction pass as a stream of snapshots (signals settle as scanning
/// and OCR progress) and consumed by the identify pipeline and the search UI.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signals {
    pub disc_id: DiscIdSignal,
    pub barcode: BarcodeSignal,
    pub text: TextSignal,
    /// What every one of the candidate's audio units plays for, read off the
    /// disk in the same pass the disc ID came from. Not a lookup input like
    /// the three above — it narrows, which is the other half of what a signal
    /// is for: the Ready rule admits a single match only when the total agrees
    /// with the source's own. The verdict write stores these rows, and the
    /// mapping table reads them back instead of opening the folder again.
    ///
    /// Empty for a re-identified library release: there is no folder to walk.
    pub durations: crate::import::probe::ProbedDurations,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl Signals {
    /// Total playing time of the candidate's audio, in milliseconds.
    ///
    /// `0` means "not probed" — a folder whose scan failed, a re-identified
    /// library release (no folder to walk), or audio that would not probe. A
    /// release of zero length does not exist, so the two are one fact to every
    /// consumer: there is no total to compare, and the candidate is not Ready.
    /// The stored column is `NOT NULL`, and carries the same `0`.
    pub fn probed_total_duration_ms(&self) -> u64 {
        self.durations.total_ms()
    }
}
