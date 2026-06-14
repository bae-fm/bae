//! The barcode signal: UPC/EAN code payloads found on a candidate's artwork
//! (via OCR) or in a CUE `CATALOG` field.

use super::SourcedValue;

/// The barcode signal extracted from a candidate's files. Carries the code
/// payloads (deduped, in discovery order) with their [`SignalOrigin`];
/// identify looks each one up.
///
/// [`SignalOrigin`]: super::SignalOrigin
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarcodeSignal {
    /// Artwork OCR in flight; `codes` accumulates as images are analyzed.
    Scanning { codes: Vec<SourcedValue> },
    /// Scanning finished. An empty `codes` means artwork was scanned but no
    /// codes were found — distinct from `Absent`.
    Settled { codes: Vec<SourcedValue> },
    /// No barcode source at all — no artwork to scan and no CUE `CATALOG`.
    Absent,
}

impl BarcodeSignal {
    /// The code payloads (with origin) found so far (`Scanning`) or finally
    /// (`Settled`); empty for `Absent`.
    pub fn codes(&self) -> &[SourcedValue] {
        match self {
            BarcodeSignal::Scanning { codes } | BarcodeSignal::Settled { codes } => codes,
            BarcodeSignal::Absent => &[],
        }
    }
}
