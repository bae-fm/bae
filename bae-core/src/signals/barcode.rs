//! The barcode signal: UPC/EAN codes found on a candidate's artwork — decoded
//! from the bars or read from the digits printed under them — or in a CUE
//! `CATALOG` field. What counts as a code is [`crate::barcode`]'s to say.

use super::{ArtworkAnalysis, ImageRegion, LookupFailure, SignalOrigin, SourcedValue};
use crate::barcode::Barcode;

/// The codes found in a candidate's files, deduped, in discovery order, each with
/// its [`SignalOrigin`]. A run looks up the ones the person left in, and those
/// same codes are the barcodes the release keeps as marks.
///
/// [`SignalOrigin`]: super::SignalOrigin
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarcodeSignal {
    /// Artwork OCR in flight; `codes` accumulates as images are analyzed.
    Scanning { codes: Vec<SourcedValue> },
    /// Finished. Empty `codes` here means artwork *was* scanned and held none —
    /// which is not the same as `Absent`.
    Settled { codes: Vec<SourcedValue> },
    /// Artwork OCR failed before barcode extraction finished.
    Failed {
        failure: LookupFailure,
        codes: Vec<SourcedValue>,
    },
    /// No barcode source at all — no artwork to scan and no CUE `CATALOG`.
    Absent,
}

impl BarcodeSignal {
    pub fn codes(&self) -> &[SourcedValue] {
        match self {
            BarcodeSignal::Scanning { codes }
            | BarcodeSignal::Settled { codes }
            | BarcodeSignal::Failed { codes, .. } => codes,
            BarcodeSignal::Absent => &[],
        }
    }
}

/// One code read off an image: the code, where on the image it was read,
/// and how — [`SignalOrigin::ArtworkBarcode`] for bars the detector decoded,
/// [`SignalOrigin::Artwork`] for the digits the recognizer read under them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodeReading {
    pub(super) code: Barcode,
    pub(super) region: Option<ImageRegion>,
    pub(super) origin: SignalOrigin,
}

/// The codes one read of an image holds: every detector payload that is a
/// code, then every recognized line that is a code printed under its bars.
/// Both are read through [`Barcode`], so the bars and the digits printed
/// under them spell one code the same way and the extraction pass keeps it
/// once — the detector's sighting, the bars themselves, since it comes first.
///
/// The printed digits matter where the bars do not decode: a scan too coarse
/// for the detector still reads as text.
pub(super) fn codes_in(analysis: &ArtworkAnalysis) -> impl Iterator<Item = CodeReading> + '_ {
    let detected = analysis.barcodes.iter().filter_map(|barcode| {
        Some(CodeReading {
            code: Barcode::stated(&barcode.payload)?,
            region: barcode.region,
            origin: SignalOrigin::ArtworkBarcode,
        })
    });
    let printed = analysis.text_lines.iter().filter_map(|line| {
        Some(CodeReading {
            code: Barcode::printed(&line.text)?,
            region: line.region,
            origin: SignalOrigin::Artwork,
        })
    });
    detected.chain(printed)
}

#[cfg(test)]
#[path = "barcode_tests.rs"]
mod tests;
