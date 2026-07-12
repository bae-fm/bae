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

// `failure` is a pure typed enum with no platform dependencies — it stays
// available everywhere so the shared metadata-search path can map provider
// errors into it. The rest is the desktop-only extraction machinery (artwork
// OCR, disc-ID compute), gated off mobile alongside the import pipeline that
// drives it.
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
mod dump;
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
pub use barcode::BarcodeSignal;
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
}
