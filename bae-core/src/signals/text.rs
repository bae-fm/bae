//! The text signal: catalog-number candidates and free-text lines harvested
//! from a candidate's surfaces (artwork OCR, folder name, filenames, CUE,
//! text files) and classified by the `candidate_text` module.

use super::{ImageRegion, LookupFailure, SignalOrigin, SourcedValue};

/// Two classified pools. `catalogs` are the catalog-number candidates: identify
/// narrows by them, each becomes a Refine badge, and they feed the Catalog
/// autocomplete — so they carry a [`SignalOrigin`] to show where each came from.
/// `free_text` are artist/album candidates and only feed an autocomplete, so they
/// don't. Both accumulate while `Scanning` and are final once `Settled`; either may
/// be empty.
///
/// [`SignalOrigin`]: super::SignalOrigin
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSignal {
    Scanning {
        catalogs: Vec<SourcedValue>,
        free_text: Vec<String>,
    },
    Settled {
        catalogs: Vec<SourcedValue>,
        free_text: Vec<String>,
    },
    Failed {
        failure: LookupFailure,
        catalogs: Vec<SourcedValue>,
        free_text: Vec<String>,
    },
}

impl TextSignal {
    pub fn catalogs(&self) -> &[SourcedValue] {
        match self {
            TextSignal::Scanning { catalogs, .. }
            | TextSignal::Settled { catalogs, .. }
            | TextSignal::Failed { catalogs, .. } => catalogs,
        }
    }

    #[cfg(test)]
    pub fn free_text(&self) -> &[String] {
        match self {
            TextSignal::Scanning { free_text, .. }
            | TextSignal::Settled { free_text, .. }
            | TextSignal::Failed { free_text, .. } => free_text,
        }
    }
}

/// One line of the candidate's own text, kept whole. Extraction gathers every
/// line it read — artwork OCR, the folder and parent names, file names, CUE
/// fields, `.txt` contents — and keeps them here beside the classified pools.
///
/// This is what ranking reads: a result is judged by looking for the result's
/// own fields in these lines. Nothing is extracted from them to judge with, so
/// nothing here is a lookup input and no run reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLine {
    /// The line as it was read, verbatim.
    pub text: String,
    pub origin: SignalOrigin,
    /// The candidate-relative path of the file the line was read off, where
    /// the origin is a file. `None` for the folder's own name, and for a
    /// re-identify pass over a library release, whose images are stored blobs.
    pub file: Option<String>,
    /// Where on the image the line was read, for an artwork line whose
    /// recognizer reports positions. `None` for every other origin.
    pub region: Option<ImageRegion>,
}
