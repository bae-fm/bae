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

/// Whether a code is a placeholder rather than a barcode: every character the
/// same digit, which is what an unfilled tag or CUE `CATALOG` field holds
/// (`0000000000000`). No printed UPC or EAN reads this way — its check digit
/// alone rules it out — so a lookup for one can only miss, and extraction
/// drops it rather than spending a request to learn that.
pub fn is_placeholder_code(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_digit() && chars.all(|c| c == first)
}

#[cfg(test)]
mod tests {
    use super::is_placeholder_code;

    /// A run of one digit is a placeholder whatever the length or the digit;
    /// anything with a second distinct character is a code to look up.
    #[test]
    fn a_run_of_one_digit_is_a_placeholder() {
        for value in [
            "0000000000000",
            "000000000000",
            "00000000",
            "1111111111111",
            "9999999999999",
            "0",
        ] {
            assert!(is_placeholder_code(value), "{value} is a placeholder");
        }
        for value in [
            "0075678164521",
            "5051961234567",
            "0000000000001",
            "1000000000000",
            "",
            "N/A",
            "000-000",
        ] {
            assert!(!is_placeholder_code(value), "{value} is a code");
        }
    }
}
