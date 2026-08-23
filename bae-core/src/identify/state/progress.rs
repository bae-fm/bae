//! How each signal's lookup progresses, and what the toolbar makes of it.
//!
//! The reducer in the parent module drives these: it starts a lookup, records
//! what came back, and stands a settled context back up as the progress it
//! would have reached. Nothing here decides anything about the candidate —
//! that is `super::step`'s.

use super::{Effect, SignalState, SignalsContext};
use crate::db::LibraryStatus;
use crate::import::search::MetadataResult;
use crate::signals::{DiscIdSignal, LookupFailure, SourcedValue};

/// The disc-ID signal's progress. `Done` / `Skipped` / `Failed` are the settled
/// variants — combine fires once every signal is settled.
#[derive(Clone, Debug, PartialEq)]
pub enum DiscidProgress {
    Computing,
    LookingUp,
    Done {
        results: Vec<(MetadataResult, LibraryStatus)>,
        track_count: u32,
    },
    /// No LOG/CUE to derive a disc ID from. Still carries the local track count,
    /// so a barcode match can report "N tracks here vs. M on the matched release."
    Skipped {
        track_count: u32,
    },
    Failed {
        failure: LookupFailure,
        track_count: u32,
    },
}

impl DiscidProgress {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            DiscidProgress::Done { .. }
                | DiscidProgress::Skipped { .. }
                | DiscidProgress::Failed { .. }
        )
    }

    pub fn results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        match self {
            DiscidProgress::Done { results, .. } => results.clone(),
            _ => Vec::new(),
        }
    }
}

/// The barcode signal's progress. `LookingUp` carries position + total so the UI
/// can show "Looking up barcode 2 of 3."
#[derive(Clone, Debug, PartialEq)]
pub enum BarcodeProgress {
    Scanning,
    LookingUp {
        current: String,
        position: u32,
        total: u32,
        remaining: Vec<String>,
    },
    Done {
        /// Which barcode produced the results; `None` when the queue drained
        /// without a match, or held no codes at all. Carried so the UI can name
        /// the value: "Barcode 5051961234567 matched 1 release."
        matched: Option<String>,
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    Failed {
        failure: LookupFailure,
    },
    /// No barcode source at all. Combine treats it like `Done { results: [] }`.
    Skipped,
}

impl BarcodeProgress {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            BarcodeProgress::Done { .. }
                | BarcodeProgress::Failed { .. }
                | BarcodeProgress::Skipped
        )
    }

    pub fn results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        match self {
            BarcodeProgress::Done { results, .. } => results.clone(),
            _ => Vec::new(),
        }
    }

    /// Which barcode produced the matched results, if any.
    pub fn matched_barcode(&self) -> Option<&str> {
        match self {
            BarcodeProgress::Done { matched, .. } => matched.as_deref(),
            _ => None,
        }
    }
}

/// The chosen catalog number's lookup. `Skipped` is the resting state: the
/// catalog runs only once the user picks one of the extracted numbers.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogProgress {
    /// No catalog number chosen, so nothing to look up.
    Skipped,
    LookingUp,
    Done {
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    Failed {
        failure: LookupFailure,
    },
}

impl CatalogProgress {
    pub fn is_settled(&self) -> bool {
        !matches!(self, CatalogProgress::LookingUp)
    }

    pub fn results(&self) -> Vec<(MetadataResult, LibraryStatus)> {
        match self {
            CatalogProgress::Done { results } => results.clone(),
            _ => Vec::new(),
        }
    }
}

pub(super) fn discid_progress_state(progress: &DiscidProgress) -> SignalState {
    match progress {
        DiscidProgress::Computing | DiscidProgress::LookingUp => SignalState::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results.len() as u32),
        DiscidProgress::Skipped { .. } => SignalState::Skipped,
        DiscidProgress::Failed { failure, .. } => SignalState::Failed {
            failure: failure.clone(),
        },
    }
}

pub(super) fn barcode_progress_state(progress: &BarcodeProgress) -> SignalState {
    match progress {
        BarcodeProgress::Scanning | BarcodeProgress::LookingUp { .. } => SignalState::LookingUp,
        BarcodeProgress::Done { results, .. } => found_or_no_match(results.len() as u32),
        BarcodeProgress::Failed { failure } => SignalState::Failed {
            failure: failure.clone(),
        },
        BarcodeProgress::Skipped => SignalState::Skipped,
    }
}

pub(super) fn catalog_progress_state(progress: &CatalogProgress) -> SignalState {
    match progress {
        CatalogProgress::Skipped => SignalState::Skipped,
        CatalogProgress::LookingUp => SignalState::LookingUp,
        CatalogProgress::Done { results } => found_or_no_match(results.len() as u32),
        CatalogProgress::Failed { failure } => SignalState::Failed {
            failure: failure.clone(),
        },
    }
}

/// The disc-ID pipe a settled context stands back up as. The lookup failure is
/// checked first: `disc_id` only ever reports whether a disc ID could be
/// *computed* (a readable TOC), so a lookup that ran against a perfectly good
/// disc ID and then hit a network/provider error would otherwise read as
/// `Computed` with zero results — indistinguishable from a clean no-match.
/// `context.discid_failure` is what tells the two apart.
///
/// This is what a state that has left `Triangulating` re-enters it with when
/// another signal starts a lookup, and what its badge state is read off.
pub(super) fn settled_discid_progress(context: &SignalsContext) -> DiscidProgress {
    let track_count = context.track_count;
    if let Some(failure) = &context.discid_failure {
        return DiscidProgress::Failed {
            failure: failure.clone(),
            track_count,
        };
    }
    match &context.disc_id {
        DiscIdSignal::Absent { .. } => DiscidProgress::Skipped { track_count },
        DiscIdSignal::Failed { failure, .. } => DiscidProgress::Failed {
            failure: failure.clone(),
            track_count,
        },
        DiscIdSignal::Computed { .. } => DiscidProgress::Done {
            results: context.discid_results.clone(),
            track_count,
        },
    }
}

/// The barcode pipe a settled context stands back up as. Scanned and found
/// nothing settles as `Done` with no results — a no-match — while nothing to
/// scan at all is a skip.
pub(super) fn settled_barcode_progress(context: &SignalsContext) -> BarcodeProgress {
    if let Some(failure) = &context.barcode_failure {
        return BarcodeProgress::Failed {
            failure: failure.clone(),
        };
    }
    if context.barcode_codes.is_empty() && !context.had_barcode_source {
        return BarcodeProgress::Skipped;
    }
    BarcodeProgress::Done {
        matched: context.matched_barcode.clone(),
        results: context.barcode_results.clone(),
    }
}

/// The catalog pipe a settled context stands back up as.
pub(super) fn settled_catalog_progress(context: &SignalsContext) -> CatalogProgress {
    if let Some(failure) = &context.catalog_failure {
        return CatalogProgress::Failed {
            failure: failure.clone(),
        };
    }
    if context.chosen_catalog.is_none() {
        return CatalogProgress::Skipped;
    }
    CatalogProgress::Done {
        results: context.catalog_results.clone(),
    }
}

pub(super) fn settled_identity_state(context: &SignalsContext) -> SignalState {
    discid_progress_state(&settled_discid_progress(context))
}

pub(super) fn barcode_settled_state(context: &SignalsContext) -> SignalState {
    barcode_progress_state(&settled_barcode_progress(context))
}

pub(super) fn catalog_settled_state(context: &SignalsContext) -> SignalState {
    catalog_progress_state(&settled_catalog_progress(context))
}

pub(super) fn found_or_no_match(count: u32) -> SignalState {
    if count == 0 {
        SignalState::NoMatch
    } else {
        SignalState::Found { count }
    }
}
pub(super) fn start_discid_progress(
    signal: &DiscIdSignal,
    effects: &mut Vec<Effect>,
) -> DiscidProgress {
    match signal {
        DiscIdSignal::Computed {
            disc_id,
            track_count,
        } => {
            effects.push(Effect::LookupDiscid {
                disc_id: disc_id.clone(),
                track_count: *track_count,
            });
            DiscidProgress::LookingUp
        }
        DiscIdSignal::Absent { track_count } => DiscidProgress::Skipped {
            track_count: *track_count,
        },
        DiscIdSignal::Failed {
            failure,
            track_count,
        } => DiscidProgress::Failed {
            failure: failure.clone(),
            track_count: *track_count,
        },
    }
}

pub(super) fn start_barcode_progress(
    codes: &[SourcedValue],
    had_source: bool,
    effects: &mut Vec<Effect>,
) -> BarcodeProgress {
    if codes.is_empty() {
        // Nothing to look up. Whether that settles as "looked, found no match" or
        // "never looked" turns on whether a barcode source existed — the empty vec
        // alone cannot say.
        return if had_source {
            BarcodeProgress::Done {
                matched: None,
                results: Vec::new(),
            }
        } else {
            BarcodeProgress::Skipped
        };
    }

    let mut values: Vec<String> = codes.iter().map(|c| c.value.clone()).collect();
    let total = values.len() as u32;
    let current = values.remove(0);
    effects.push(Effect::LookupBarcode {
        barcode: current.clone(),
    });
    BarcodeProgress::LookingUp {
        current,
        position: 1,
        total,
        remaining: values,
    }
}
/// The track count is whatever the disc-ID signal reported — every one of its
/// settled variants carries the local count, whether or not a disc ID was derived.
pub(super) fn settled_track_count(discid: &DiscidProgress) -> u32 {
    match discid {
        DiscidProgress::Done { track_count, .. } => *track_count,
        DiscidProgress::Skipped { track_count } => *track_count,
        DiscidProgress::Failed { track_count, .. } => *track_count,
        DiscidProgress::Computing | DiscidProgress::LookingUp => 0,
    }
}
