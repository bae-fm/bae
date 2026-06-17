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

pub mod barcode;
pub mod disc_id;
pub mod origin;
pub mod service;
pub mod text;

pub use barcode::BarcodeSignal;
pub use disc_id::DiscIdSignal;
pub use origin::{SignalOrigin, SourcedValue};
pub use service::{ExtractionService, ExtractionServiceHandle, ExtractionSource};
pub use text::TextSignal;

/// The identifying signals extracted from one candidate's files. Produced by
/// the extraction pass as a stream of snapshots (signals settle as scanning
/// and OCR progress) and consumed by the identify pipeline and the search UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signals {
    pub disc_id: DiscIdSignal,
    pub barcode: BarcodeSignal,
    pub text: TextSignal,
}
