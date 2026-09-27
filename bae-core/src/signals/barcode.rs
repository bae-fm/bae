//! The barcode signal: UPC/EAN codes found on a candidate's artwork — decoded
//! from the bars or read from the digits printed under them — or in a CUE
//! `CATALOG` field. What counts as a code is [`crate::barcode`]'s to say.

use super::{ArtworkAnalysis, LookupFailure, SourcedValue};
use crate::barcode::Barcode;

/// The codes found in a candidate's files, in discovery order, each once per
/// file it was read off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarcodeSignal {
    /// Artwork OCR in flight; `codes` accumulates as images are analyzed.
    Scanning { codes: Vec<SourcedValue> },
    /// Finished; empty `codes` means the sources were read and held none.
    Settled { codes: Vec<SourcedValue> },
    /// Artwork OCR failed before barcode extraction finished.
    Failed {
        failure: LookupFailure,
        codes: Vec<SourcedValue>,
    },
    /// No barcode source was read: no CUE `CATALOG`, and no artwork read.
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

/// The codes on one image: detector payloads first, then lines that are the
/// digits printed under bars — which catch codes whose bars did not decode.
pub(super) fn codes_in(analysis: &ArtworkAnalysis) -> impl Iterator<Item = Barcode> + '_ {
    let detected = analysis
        .barcodes
        .iter()
        .filter_map(|payload| Barcode::stated(payload));
    let printed = analysis
        .text_lines
        .iter()
        .filter_map(|line| Barcode::printed(line));
    detected.chain(printed)
}

#[cfg(test)]
#[path = "barcode_tests.rs"]
mod tests;
