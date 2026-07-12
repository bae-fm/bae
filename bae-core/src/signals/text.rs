//! The text signal: catalog-number candidates and free-text lines harvested
//! from a candidate's surfaces (artwork OCR, folder name, filenames, CUE,
//! text files) and classified by the `candidate_text` module.

use super::{LookupFailure, SourcedValue};

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
