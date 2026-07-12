//! The text signal: catalog-number candidates and free-text lines harvested
//! from a candidate's surfaces (artwork OCR, folder name, filenames, CUE,
//! text files) and classified by the `candidate_text` module.

use super::{LookupFailure, SourcedValue};

/// The text signal extracted from a candidate's files.
///
/// Two classified pools: `catalogs` (catalog-number candidates — the identify
/// catalog filter narrows by these, they each become a Refine badge, and they
/// feed the Catalog autocomplete) and `free_text` (artist/album candidates for
/// the manual-search autocomplete). Catalogs carry their [`SignalOrigin`] so a
/// badge can show where it came from; free text doesn't (it only feeds an
/// autocomplete pool). Both are cumulative while `Scanning` and final once
/// `Settled`; either may be empty.
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
    /// Catalog-number candidates (with origin) harvested so far / finally.
    pub fn catalogs(&self) -> &[SourcedValue] {
        match self {
            TextSignal::Scanning { catalogs, .. }
            | TextSignal::Settled { catalogs, .. }
            | TextSignal::Failed { catalogs, .. } => catalogs,
        }
    }

    /// Free-text (artist/album) candidates harvested so far / finally.
    pub fn free_text(&self) -> &[String] {
        match self {
            TextSignal::Scanning { free_text, .. }
            | TextSignal::Settled { free_text, .. }
            | TextSignal::Failed { free_text, .. } => free_text,
        }
    }
}
