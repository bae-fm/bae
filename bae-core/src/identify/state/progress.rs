//! How each signal's lookup progresses, and what the toolbar makes of it.
//!
//! The reducer in the parent module drives these: it starts a lookup, records
//! what came back, and stands a settled context back up as the progress it
//! would have reached. Nothing here decides anything about the candidate —
//! that is `super::step`'s.
//!
//! The barcode and catalog lookups ask every provider in the run, and each
//! provider answers for itself: one still looking never holds up what another
//! already found, and one failing leaves the others' answers standing. So a
//! pipe holds one entry per provider, and settles only once every one of them
//! has.

use super::{Effect, SignalState, SignalsContext};
use crate::db::LibraryStatus;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::MetadataSource;
use crate::signals::{DiscIdSignal, LookupFailure, SourcedValue};

/// What one lookup produced: each match paired with its library status.
pub type LookupResults = Vec<(MetadataResult, LibraryStatus)>;

/// The disc-ID signal's progress. `Done` / `Skipped` / `Failed` are the settled
/// variants — combine fires once every signal is settled. The disc-ID endpoint
/// is MusicBrainz's alone, so this pipe has one provider and no list.
#[derive(Clone, Debug, PartialEq)]
pub enum DiscidProgress {
    Computing,
    LookingUp,
    Done {
        results: LookupResults,
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

    pub fn results(&self) -> LookupResults {
        match self {
            DiscidProgress::Done { results, .. } => results.clone(),
            _ => Vec::new(),
        }
    }
}

/// One provider's part of a lookup with a single value to ask about — the
/// chosen catalog number.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderLookup {
    pub source: MetadataSource,
    pub state: LookupState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LookupState {
    LookingUp,
    Done { results: LookupResults },
    Failed { failure: LookupFailure },
}

impl LookupState {
    fn is_settled(&self) -> bool {
        !matches!(self, LookupState::LookingUp)
    }
}

/// One provider trying the candidate's barcodes in order, on its own: a code
/// that matches ends its walk, a miss moves it to the next code, and a failure
/// leaves it failed where it was.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderBarcodeLookup {
    pub source: MetadataSource,
    pub state: BarcodeLookupState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BarcodeLookupState {
    /// Asking about the code at `index` in the pipe's list.
    Trying {
        index: usize,
    },
    /// A code matched. `code` is `None` for a pipe stood back up from a
    /// settled context, which records what each provider found but not
    /// which of several codes found it; the run's own matched code stands
    /// for it then.
    Matched {
        code: Option<String>,
        results: LookupResults,
    },
    /// Every code tried, none matched.
    Exhausted,
    Failed {
        failure: LookupFailure,
    },
}

impl BarcodeLookupState {
    fn is_settled(&self) -> bool {
        !matches!(self, BarcodeLookupState::Trying { .. })
    }
}

/// The barcode signal's progress.
#[derive(Clone, Debug, PartialEq)]
pub enum BarcodeProgress {
    /// The artwork is still being read for codes.
    Scanning,
    /// There was a barcode source and it held no code: a no-match with
    /// nothing to ask.
    NoCodes,
    /// The codes to try, and every provider's walk through them. Settled once
    /// every provider is.
    Lookups {
        codes: Vec<String>,
        providers: Vec<ProviderBarcodeLookup>,
    },
    /// Reading the candidate's barcodes failed, so no provider was ever asked.
    /// Not a provider's failure, and not a skip either: there was artwork to
    /// read and reading it did not work.
    ScanFailed { failure: LookupFailure },
    /// No barcode source at all. Combine treats it like a no-match.
    Skipped,
}

impl BarcodeProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            BarcodeProgress::Scanning => false,
            BarcodeProgress::Lookups { providers, .. } => {
                providers.iter().all(|p| p.state.is_settled())
            }
            BarcodeProgress::NoCodes
            | BarcodeProgress::ScanFailed { .. }
            | BarcodeProgress::Skipped => true,
        }
    }

    /// What every provider that matched found, in provider order.
    pub fn results(&self) -> LookupResults {
        match self {
            BarcodeProgress::Lookups { providers, .. } => providers
                .iter()
                .filter_map(|p| match &p.state {
                    BarcodeLookupState::Matched { results, .. } => Some(results.clone()),
                    _ => None,
                })
                .flatten()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The providers that failed, whether or not the others answered.
    pub fn failures(&self) -> Vec<SourceFailure> {
        match self {
            BarcodeProgress::Lookups { providers, .. } => providers
                .iter()
                .filter_map(|p| match &p.state {
                    BarcodeLookupState::Failed { failure } => Some(SourceFailure {
                        source: p.source,
                        failure: failure.clone(),
                    }),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Why reading the candidate's barcodes failed, where it did.
    pub fn scan_failure(&self) -> Option<&LookupFailure> {
        match self {
            BarcodeProgress::ScanFailed { failure } => Some(failure),
            _ => None,
        }
    }

    /// Which code found the release: the earliest in the list that any
    /// provider matched. A provider stood back up from a settled context
    /// matched *some* code without saying which, so `previous` — the run's
    /// matched code before the stand-up — competes on its behalf.
    pub fn matched_barcode(&self, previous: Option<&str>) -> Option<String> {
        let BarcodeProgress::Lookups { codes, providers } = self else {
            return None;
        };
        let index_of = |code: &str| codes.iter().position(|c| c == code);
        let mut candidates: Vec<&str> = Vec::new();
        for provider in providers {
            match &provider.state {
                BarcodeLookupState::Matched {
                    code: Some(code), ..
                } => candidates.push(code),
                BarcodeLookupState::Matched { code: None, .. } => {
                    candidates.extend(previous);
                }
                _ => {}
            }
        }
        candidates
            .into_iter()
            .min_by_key(|code| index_of(code).unwrap_or(usize::MAX))
            .map(str::to_string)
    }
}

/// The chosen catalog number's lookup. `Skipped` is the resting state: the
/// catalog runs only once the user picks one of the extracted numbers.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogProgress {
    /// No catalog number chosen, so nothing to look up.
    Skipped,
    /// Every provider's part of the chosen number's lookup. Settled once
    /// every provider is.
    Lookups { lookups: Vec<ProviderLookup> },
}

impl CatalogProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            CatalogProgress::Skipped => true,
            CatalogProgress::Lookups { lookups } => lookups.iter().all(|l| l.state.is_settled()),
        }
    }

    pub fn results(&self) -> LookupResults {
        match self {
            CatalogProgress::Lookups { lookups } => lookups
                .iter()
                .filter_map(|l| match &l.state {
                    LookupState::Done { results } => Some(results.clone()),
                    _ => None,
                })
                .flatten()
                .collect(),
            CatalogProgress::Skipped => Vec::new(),
        }
    }

    /// The providers that failed this lookup, whether or not any answered.
    pub fn failures(&self) -> Vec<SourceFailure> {
        match self {
            CatalogProgress::Lookups { lookups } => lookups
                .iter()
                .filter_map(|l| match &l.state {
                    LookupState::Failed { failure } => Some(SourceFailure {
                        source: l.source,
                        failure: failure.clone(),
                    }),
                    _ => None,
                })
                .collect(),
            CatalogProgress::Skipped => Vec::new(),
        }
    }
}

// ── Toolbar badge states ────────────────────────────────────────────────────

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

/// A lookup that got results from one provider and a failure from another is
/// `Found`: the badge says what the signal turned up, and the failure is named
/// where the pane names failures. Only a lookup that turned up nothing reads
/// as failed, and its badge carries the first provider's reason.
pub(super) fn barcode_progress_state(progress: &BarcodeProgress) -> SignalState {
    match progress {
        BarcodeProgress::Scanning => SignalState::LookingUp,
        BarcodeProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        BarcodeProgress::Lookups { .. } => {
            settled_lookup_state(progress.results().len(), &progress.failures())
        }
        BarcodeProgress::NoCodes => SignalState::NoMatch,
        BarcodeProgress::ScanFailed { failure } => SignalState::Failed {
            failure: failure.clone(),
        },
        BarcodeProgress::Skipped => SignalState::Skipped,
    }
}

pub(super) fn catalog_progress_state(progress: &CatalogProgress) -> SignalState {
    match progress {
        CatalogProgress::Skipped => SignalState::Skipped,
        CatalogProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        CatalogProgress::Lookups { .. } => {
            settled_lookup_state(progress.results().len(), &progress.failures())
        }
    }
}

/// The badge for a settled multi-provider lookup: what it found, else why it
/// found nothing. Failures with no results carry the first provider's reason;
/// no results and no failures is a no-match.
fn settled_lookup_state(n_results: usize, failures: &[SourceFailure]) -> SignalState {
    if n_results > 0 {
        return found_or_no_match(n_results as u32);
    }
    match failures.first() {
        Some(first) => SignalState::Failed {
            failure: first.failure.clone(),
        },
        None => SignalState::NoMatch,
    }
}

// ── Standing a settled context back up ──────────────────────────────────────

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

/// The barcode pipe a settled context stands back up as: one entry per
/// provider, each holding what the context recorded for it. Scanned and found
/// nothing settles as `NoCodes`, while nothing to scan at all is a skip.
pub(super) fn settled_barcode_progress(context: &SignalsContext) -> BarcodeProgress {
    if let Some(failure) = &context.barcode_scan_failure {
        return BarcodeProgress::ScanFailed {
            failure: failure.clone(),
        };
    }
    if context.barcode_codes.is_empty() {
        return if context.had_barcode_source {
            BarcodeProgress::NoCodes
        } else {
            BarcodeProgress::Skipped
        };
    }
    BarcodeProgress::Lookups {
        codes: code_values(&context.barcode_codes),
        providers: context
            .providers
            .iter()
            .map(|&source| ProviderBarcodeLookup {
                source,
                state: recorded_barcode_state(context, source),
            })
            .collect(),
    }
}

/// What the context recorded for one provider's barcode walk.
fn recorded_barcode_state(context: &SignalsContext, source: MetadataSource) -> BarcodeLookupState {
    if let Some(failure) = context.barcode_failures.iter().find(|f| f.source == source) {
        return BarcodeLookupState::Failed {
            failure: failure.failure.clone(),
        };
    }
    let results = results_from(&context.barcode_results, source);
    if results.is_empty() {
        BarcodeLookupState::Exhausted
    } else {
        BarcodeLookupState::Matched {
            code: None,
            results,
        }
    }
}

/// The catalog pipe a settled context stands back up as.
pub(super) fn settled_catalog_progress(context: &SignalsContext) -> CatalogProgress {
    if context.chosen_catalog.is_none() {
        return CatalogProgress::Skipped;
    }
    CatalogProgress::Lookups {
        lookups: context
            .providers
            .iter()
            .map(|&source| ProviderLookup {
                source,
                state: recorded_catalog_state(context, source),
            })
            .collect(),
    }
}

fn recorded_catalog_state(context: &SignalsContext, source: MetadataSource) -> LookupState {
    if let Some(failure) = context.catalog_failures.iter().find(|f| f.source == source) {
        return LookupState::Failed {
            failure: failure.failure.clone(),
        };
    }
    LookupState::Done {
        results: results_from(&context.catalog_results, source),
    }
}

/// One provider's share of a recorded result list: every result names the
/// source that returned it.
fn results_from(results: &LookupResults, source: MetadataSource) -> LookupResults {
    results
        .iter()
        .filter(|(result, _)| result.source == source)
        .cloned()
        .collect()
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

// ── Starting lookups ────────────────────────────────────────────────────────

pub(super) fn start_discid_progress(
    signal: &DiscIdSignal,
    effects: &mut Vec<Effect>,
) -> DiscidProgress {
    match signal {
        DiscIdSignal::Computed {
            disc_id,
            track_count,
            ..
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

/// Start (or restart) every provider's walk through the codes. A scan that
/// failed has no codes to walk and never gets a lookup, so it settles as the
/// failure it is rather than as the no-match an empty list would otherwise
/// read as — which is what a re-run over a failed scan used to produce.
pub(super) fn start_barcode_progress(
    codes: &[SourcedValue],
    had_source: bool,
    scan_failure: Option<&LookupFailure>,
    providers: &[MetadataSource],
    effects: &mut Vec<Effect>,
) -> BarcodeProgress {
    if let Some(failure) = scan_failure {
        return BarcodeProgress::ScanFailed {
            failure: failure.clone(),
        };
    }
    if codes.is_empty() {
        // Nothing to look up. Whether that settles as "looked, found no match" or
        // "never looked" turns on whether a barcode source existed — the empty
        // list alone cannot say.
        return if had_source {
            BarcodeProgress::NoCodes
        } else {
            BarcodeProgress::Skipped
        };
    }
    let codes = code_values(codes);
    let providers = providers
        .iter()
        .map(|&source| {
            effects.push(Effect::LookupBarcode {
                source,
                barcode: codes[0].clone(),
            });
            ProviderBarcodeLookup {
                source,
                state: BarcodeLookupState::Trying { index: 0 },
            }
        })
        .collect();
    BarcodeProgress::Lookups { codes, providers }
}

/// Ask every provider about the chosen catalog number.
pub(super) fn start_catalog_progress(
    catalog: &str,
    providers: &[MetadataSource],
    effects: &mut Vec<Effect>,
) -> CatalogProgress {
    CatalogProgress::Lookups {
        lookups: providers
            .iter()
            .map(|&source| {
                effects.push(Effect::LookupCatalog {
                    source,
                    catalog: catalog.to_string(),
                });
                ProviderLookup {
                    source,
                    state: LookupState::LookingUp,
                }
            })
            .collect(),
    }
}

// ── Retrying what failed ────────────────────────────────────────────────────

/// Put every failed barcode walk back to its first code and ask again. What
/// the other providers found stays as it is.
pub(super) fn retry_failed_barcode_lookups(
    progress: &mut BarcodeProgress,
    effects: &mut Vec<Effect>,
) {
    let BarcodeProgress::Lookups { codes, providers } = progress else {
        return;
    };
    for provider in providers.iter_mut() {
        if matches!(provider.state, BarcodeLookupState::Failed { .. }) {
            effects.push(Effect::LookupBarcode {
                source: provider.source,
                barcode: codes[0].clone(),
            });
            provider.state = BarcodeLookupState::Trying { index: 0 };
        }
    }
}

/// Ask every provider whose catalog lookup failed again.
pub(super) fn retry_failed_catalog_lookups(
    progress: &mut CatalogProgress,
    catalog: &str,
    effects: &mut Vec<Effect>,
) {
    let CatalogProgress::Lookups { lookups } = progress else {
        return;
    };
    for lookup in lookups.iter_mut() {
        if matches!(lookup.state, LookupState::Failed { .. }) {
            effects.push(Effect::LookupCatalog {
                source: lookup.source,
                catalog: catalog.to_string(),
            });
            lookup.state = LookupState::LookingUp;
        }
    }
}

/// Ask MusicBrainz about the disc ID again, when the failure was the lookup's
/// and not the derivation's: a disc ID that could not be computed has nothing
/// to ask about.
pub(super) fn retry_failed_discid_lookup(
    progress: &mut DiscidProgress,
    signal: &DiscIdSignal,
    effects: &mut Vec<Effect>,
) {
    if !matches!(progress, DiscidProgress::Failed { .. }) {
        return;
    }
    if let DiscIdSignal::Computed {
        disc_id,
        track_count,
        ..
    } = signal
    {
        effects.push(Effect::LookupDiscid {
            disc_id: disc_id.clone(),
            track_count: *track_count,
        });
        *progress = DiscidProgress::LookingUp;
    }
}

fn code_values(codes: &[SourcedValue]) -> Vec<String> {
    codes.iter().map(|c| c.value.clone()).collect()
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
