//! The barcode signal: UPC/EAN code payloads found on a candidate's artwork
//! (via OCR) or in a CUE `CATALOG` field.

use super::{LookupFailure, SourcedValue};

/// The codes found in a candidate's files, deduped, in discovery order, each with
/// its [`SignalOrigin`]. Identify looks every one of them up.
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
