//! How each step's lookup progresses, and the toolbar badge each one shows.
//!
//! Every provider answers for itself, so a lookup holds one entry per provider
//! and settles once all of them have.

use super::{Effect, LookupOutcome, SignalState, SignalsContext};
use crate::db::LibraryStatus;
use crate::identify::NotAskedReason;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::Catalog;
use crate::signals::{DiscIdSignal, LookupFailure};

/// What one lookup produced: each match paired with its library status.
pub type LookupResults = Vec<(MetadataResult, LibraryStatus)>;

/// The disc-ID lookup's progress. Only MusicBrainz answers disc IDs, so there
/// is one provider and no list.
#[derive(Clone, Debug, PartialEq)]
pub enum DiscidProgress {
    Computing,
    LookingUp,
    Done {
        results: LookupResults,
    },
    /// No disc ID was derived.
    Skipped,
    /// A disc ID was derived and nobody was asked about it, for `reason`.
    NotAsked {
        reason: NotAskedReason,
    },
    Failed {
        failure: LookupFailure,
    },
}

impl DiscidProgress {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            DiscidProgress::Done { .. }
                | DiscidProgress::Skipped
                | DiscidProgress::NotAsked { .. }
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

/// The ISRC lookup's progress. Only MusicBrainz answers ISRCs, and every code
/// the audio's tags carry is asked in one search, so there is one lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum IsrcProgress {
    /// Waiting for the first snapshot, which carries the tags' codes.
    Reading,
    LookingUp,
    Done {
        results: LookupResults,
    },
    /// No audio file's tags carry an ISRC.
    Skipped,
    /// The tags carry codes and nobody was asked about them, for `reason`.
    NotAsked {
        reason: NotAskedReason,
    },
    Failed {
        failure: LookupFailure,
    },
}

impl IsrcProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            IsrcProgress::Reading | IsrcProgress::LookingUp => false,
            IsrcProgress::Done { .. }
            | IsrcProgress::Skipped
            | IsrcProgress::NotAsked { .. }
            | IsrcProgress::Failed { .. } => true,
        }
    }

    pub fn results(&self) -> LookupResults {
        match self {
            IsrcProgress::Done { results } => results.clone(),
            IsrcProgress::Reading
            | IsrcProgress::LookingUp
            | IsrcProgress::Skipped
            | IsrcProgress::NotAsked { .. }
            | IsrcProgress::Failed { .. } => Vec::new(),
        }
    }
}

/// One provider's part of a lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderLookup {
    pub source: Catalog,
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

/// The barcode signal's progress.
#[derive(Clone, Debug, PartialEq)]
pub enum BarcodeProgress {
    /// The artwork is still being read for codes.
    Scanning,
    /// There was a barcode source and it held no code.
    NoCodes,
    /// One lookup per code the run asks about, in first-seen order; every code
    /// is asked, since a folder may hold two releases.
    Lookups { codes: Vec<ValueLookup> },
    /// The candidate has barcodes and nobody was asked about any of them, for
    /// `reason`; the codes are still listed.
    NotAsked {
        codes: Vec<String>,
        reason: NotAskedReason,
    },
    /// No barcode source at all.
    Skipped,
}

impl BarcodeProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            BarcodeProgress::Scanning => false,
            BarcodeProgress::Lookups { codes } => codes.iter().all(ValueLookup::is_settled),
            BarcodeProgress::NoCodes
            | BarcodeProgress::NotAsked { .. }
            | BarcodeProgress::Skipped => true,
        }
    }

    /// What every code's lookup found, in code then provider order.
    pub fn results(&self) -> LookupResults {
        self.lookups()
            .iter()
            .flat_map(ValueLookup::results)
            .collect()
    }

    /// Every provider that failed any code's lookup.
    pub fn failures(&self) -> Vec<SourceFailure> {
        self.lookups()
            .iter()
            .flat_map(ValueLookup::failures)
            .collect()
    }

    /// The codes' lookups, in first-seen order.
    pub fn lookups(&self) -> &[ValueLookup] {
        match self {
            BarcodeProgress::Lookups { codes } => codes,
            BarcodeProgress::Scanning
            | BarcodeProgress::NoCodes
            | BarcodeProgress::NotAsked { .. }
            | BarcodeProgress::Skipped => &[],
        }
    }

    /// The earliest code any provider found something for.
    pub fn matched_barcode(&self) -> Option<String> {
        self.lookups()
            .iter()
            .find(|lookup| !lookup.results().is_empty())
            .map(|lookup| lookup.value.clone())
    }
}

/// The title search's progress: the run's last step, decided once the three
/// identifiers and the ISRCs have settled.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchProgress {
    /// Waiting for the three identifiers and the ISRCs.
    Pending,
    /// The identifiers answered, or there was nothing to search by.
    Skipped,
    /// Nobody is asked the title, for `reason`.
    NotAsked { reason: NotAskedReason },
    /// One lookup per provider in the run.
    Lookups { providers: Vec<ProviderLookup> },
}

impl SearchProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            SearchProgress::Pending => false,
            SearchProgress::Skipped | SearchProgress::NotAsked { .. } => true,
            SearchProgress::Lookups { providers } => providers.iter().all(|l| l.state.is_settled()),
        }
    }

    /// What every provider that answered found, in provider order.
    pub fn results(&self) -> LookupResults {
        match self {
            SearchProgress::Lookups { providers } => providers
                .iter()
                .filter_map(|l| match &l.state {
                    LookupState::Done { results } => Some(results.clone()),
                    _ => None,
                })
                .flatten()
                .collect(),
            SearchProgress::Pending | SearchProgress::Skipped | SearchProgress::NotAsked { .. } => {
                Vec::new()
            }
        }
    }

    /// The providers that failed.
    pub fn failures(&self) -> Vec<SourceFailure> {
        match self {
            SearchProgress::Lookups { providers } => providers
                .iter()
                .filter_map(|l| match &l.state {
                    LookupState::Failed { failure } => Some(SourceFailure {
                        source: l.source,
                        failure: failure.clone(),
                    }),
                    _ => None,
                })
                .collect(),
            SearchProgress::Pending | SearchProgress::Skipped | SearchProgress::NotAsked { .. } => {
                Vec::new()
            }
        }
    }

    /// The lookups, in provider order.
    pub fn lookups(&self) -> &[ProviderLookup] {
        match self {
            SearchProgress::Lookups { providers } => providers,
            SearchProgress::Pending | SearchProgress::Skipped | SearchProgress::NotAsked { .. } => {
                &[]
            }
        }
    }
}

/// Every provider's lookup of one value: a barcode or a chosen catalog number.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueLookup {
    pub value: String,
    pub providers: Vec<ProviderLookup>,
}

impl ValueLookup {
    /// Ask every provider about `value`, each through the effect `ask` makes.
    fn started(
        value: &str,
        providers: &[Catalog],
        ask: impl Fn(Catalog, String) -> Effect,
        effects: &mut Vec<Effect>,
    ) -> Self {
        Self {
            value: value.to_string(),
            providers: providers
                .iter()
                .map(|&source| {
                    effects.push(ask(source, value.to_string()));
                    ProviderLookup {
                        source,
                        state: LookupState::LookingUp,
                    }
                })
                .collect(),
        }
    }

    /// Land `source`'s answer, where it is still being waited for.
    pub(super) fn answer(&mut self, source: Catalog, outcome: LookupOutcome) {
        if let Some(lookup) = self
            .providers
            .iter_mut()
            .find(|l| l.source == source && l.state == LookupState::LookingUp)
        {
            lookup.state = match outcome {
                Ok(results) => LookupState::Done { results },
                Err(failure) => LookupState::Failed { failure },
            };
        }
    }

    fn is_settled(&self) -> bool {
        self.providers.iter().all(|l| l.state.is_settled())
    }

    fn results(&self) -> LookupResults {
        self.providers
            .iter()
            .filter_map(|l| match &l.state {
                LookupState::Done { results } => Some(results.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn failures(&self) -> Vec<SourceFailure> {
        self.providers
            .iter()
            .filter_map(|l| match &l.state {
                LookupState::Failed { failure } => Some(SourceFailure {
                    source: l.source,
                    failure: failure.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

/// The chosen catalog numbers' lookups, each number on its own.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogProgress {
    /// No catalog number chosen, so nothing to look up.
    Skipped,
    /// One lookup per chosen number, in the order they were chosen.
    Lookups { values: Vec<ValueLookup> },
}

impl CatalogProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            CatalogProgress::Skipped => true,
            CatalogProgress::Lookups { values } => values.iter().all(ValueLookup::is_settled),
        }
    }

    /// Every chosen number's results, in chosen order.
    pub fn results(&self) -> LookupResults {
        match self {
            CatalogProgress::Lookups { values } => {
                values.iter().flat_map(ValueLookup::results).collect()
            }
            CatalogProgress::Skipped => Vec::new(),
        }
    }

    /// Every provider that failed any chosen number's lookup.
    pub fn failures(&self) -> Vec<SourceFailure> {
        match self {
            CatalogProgress::Lookups { values } => {
                values.iter().flat_map(ValueLookup::failures).collect()
            }
            CatalogProgress::Skipped => Vec::new(),
        }
    }

    fn lookup_of(&self, value: &str) -> Option<&ValueLookup> {
        match self {
            CatalogProgress::Lookups { values } => values.iter().find(|l| l.value == value),
            CatalogProgress::Skipped => None,
        }
    }

    /// One chosen number's results.
    pub fn results_for(&self, value: &str) -> LookupResults {
        self.lookup_of(value)
            .map(ValueLookup::results)
            .unwrap_or_default()
    }

    /// The providers that failed one chosen number's lookup.
    pub fn failures_for(&self, value: &str) -> Vec<SourceFailure> {
        self.lookup_of(value)
            .map(ValueLookup::failures)
            .unwrap_or_default()
    }

    /// The chosen numbers' lookups, in chosen order.
    pub fn lookups(&self) -> &[ValueLookup] {
        match self {
            CatalogProgress::Lookups { values } => values,
            CatalogProgress::Skipped => &[],
        }
    }

    /// This progress with only the lookups `keep` admits.
    pub(super) fn keeping(self, keep: impl Fn(&ValueLookup) -> bool) -> Self {
        match self {
            CatalogProgress::Lookups { mut values } => {
                values.retain(|lookup| keep(lookup));
                if values.is_empty() {
                    CatalogProgress::Skipped
                } else {
                    CatalogProgress::Lookups { values }
                }
            }
            CatalogProgress::Skipped => CatalogProgress::Skipped,
        }
    }
}

// ── Toolbar badge states ────────────────────────────────────────────────────

pub(super) fn discid_progress_state(progress: &DiscidProgress) -> SignalState {
    match progress {
        DiscidProgress::Computing | DiscidProgress::LookingUp => SignalState::LookingUp,
        DiscidProgress::Done { results, .. } => found_or_no_match(results.len() as u32),
        DiscidProgress::Skipped => SignalState::Skipped,
        DiscidProgress::NotAsked { reason, .. } => SignalState::NotAsked { reason: *reason },
        DiscidProgress::Failed { failure, .. } => SignalState::Failed {
            failure: failure.clone(),
        },
    }
}

pub(super) fn isrc_progress_state(progress: &IsrcProgress) -> SignalState {
    match progress {
        IsrcProgress::Reading | IsrcProgress::LookingUp => SignalState::LookingUp,
        IsrcProgress::Done { results } => found_or_no_match(results.len() as u32),
        IsrcProgress::Skipped => SignalState::Skipped,
        IsrcProgress::NotAsked { reason } => SignalState::NotAsked { reason: *reason },
        IsrcProgress::Failed { failure } => SignalState::Failed {
            failure: failure.clone(),
        },
    }
}

/// Results beat failures: only a lookup that found nothing reads as failed.
pub(super) fn barcode_progress_state(progress: &BarcodeProgress) -> SignalState {
    match progress {
        BarcodeProgress::Scanning => SignalState::LookingUp,
        BarcodeProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        BarcodeProgress::Lookups { .. } => {
            settled_lookup_state(progress.results().len(), &progress.failures())
        }
        BarcodeProgress::NoCodes => SignalState::NoMatch,
        BarcodeProgress::NotAsked { reason, .. } => SignalState::NotAsked { reason: *reason },
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

/// A settled lookup's badge: what it found, else the first provider's
/// failure, else no match.
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

// ── The badges a settled run wears, read off what it recorded ───────────────

/// The disc-ID badge of a settled run. The lookup's failure comes first, since
/// the signal only says whether a disc ID could be computed.
pub(super) fn settled_identity_state(context: &SignalsContext) -> SignalState {
    if let Some(failure) = &context.disc.failure {
        return SignalState::Failed {
            failure: failure.clone(),
        };
    }
    match &context.disc.signal {
        DiscIdSignal::Absent | DiscIdSignal::NotCdAudio => SignalState::Skipped,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose disc ID could not be derived ended as an error")
        }
        DiscIdSignal::Computed { .. } => match context.disc.not_asked {
            Some(reason) => SignalState::NotAsked { reason },
            None => found_or_no_match(context.disc.results.len() as u32),
        },
    }
}

/// The ISRC badge of a settled run.
pub(super) fn isrc_settled_state(context: &SignalsContext) -> SignalState {
    let isrc = &context.isrc;
    if let Some(failure) = &isrc.failure {
        return SignalState::Failed {
            failure: failure.clone(),
        };
    }
    if isrc.tagged.is_empty() {
        return SignalState::Skipped;
    }
    match isrc.not_asked {
        Some(reason) => SignalState::NotAsked { reason },
        None => found_or_no_match(isrc.results.len() as u32),
    }
}

/// The barcode badge of a settled run.
pub(super) fn barcode_settled_state(context: &SignalsContext) -> SignalState {
    let barcode = &context.barcode;
    if barcode.codes.is_empty() {
        return if barcode.had_source {
            SignalState::NoMatch
        } else {
            SignalState::Skipped
        };
    }
    if let Some(reason) = barcode.not_asked {
        return SignalState::NotAsked { reason };
    }
    settled_lookup_state(barcode.results.len(), &barcode.failures)
}

/// The catalog badge of a settled run, over every chosen number.
pub(super) fn catalog_settled_state(context: &SignalsContext) -> SignalState {
    if context.catalog.chosen.is_empty() {
        return SignalState::Skipped;
    }
    settled_lookup_state(
        context.catalog.active_results().len(),
        &context.catalog.recorded_failures(),
    )
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
    excluded: bool,
    look_up: bool,
    providers: &[Catalog],
    effects: &mut Vec<Effect>,
) -> DiscidProgress {
    match signal {
        DiscIdSignal::Computed { disc_id, .. } => {
            // The reason nearest the value wins: see `NotAskedReason`.
            let reason = if excluded {
                Some(NotAskedReason::LeftOut)
            } else if !look_up {
                Some(NotAskedReason::SwitchedOff)
            } else if !providers.contains(&Catalog::DISC_ID_CATALOG) {
                Some(NotAskedReason::NoCatalog)
            } else {
                None
            };
            if let Some(reason) = reason {
                return DiscidProgress::NotAsked { reason };
            }
            effects.push(Effect::LookupDiscid {
                disc_id: disc_id.clone(),
            });
            DiscidProgress::LookingUp
        }
        DiscIdSignal::Absent | DiscIdSignal::NotCdAudio => DiscidProgress::Skipped,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose disc ID could not be derived ended as an error")
        }
    }
}

/// Ask MusicBrainz about every code the audio's tags carry, in one search.
pub(super) fn start_isrc_progress(
    codes: Vec<String>,
    providers: &[Catalog],
    effects: &mut Vec<Effect>,
) -> IsrcProgress {
    if codes.is_empty() {
        return IsrcProgress::Skipped;
    }
    if !providers.contains(&Catalog::ISRC_CATALOG) {
        return IsrcProgress::NotAsked {
            reason: NotAskedReason::NoCatalog,
        };
    }
    effects.push(Effect::LookupIsrcs { isrcs: codes });
    IsrcProgress::LookingUp
}

/// Ask every provider about every code in `codes` the person did not leave out.
#[allow(clippy::too_many_arguments)]
pub(super) fn start_barcode_progress(
    codes: Vec<String>,
    excluded: &[String],
    had_source: bool,
    look_up: bool,
    providers: &[Catalog],
    effects: &mut Vec<Effect>,
) -> BarcodeProgress {
    if codes.is_empty() {
        // Whether there was a source is what tells "found none" from "never looked".
        return if had_source {
            BarcodeProgress::NoCodes
        } else {
            BarcodeProgress::Skipped
        };
    }
    let asked: Vec<String> = codes
        .iter()
        .filter(|code| !excluded.contains(code))
        .cloned()
        .collect();
    // The reason nearest the value wins: see `NotAskedReason`.
    let reason = if asked.is_empty() {
        Some(NotAskedReason::LeftOut)
    } else if !look_up {
        Some(NotAskedReason::SwitchedOff)
    } else {
        None
    };
    if let Some(reason) = reason {
        return BarcodeProgress::NotAsked { codes, reason };
    }
    BarcodeProgress::Lookups {
        codes: asked
            .iter()
            .map(|code| {
                ValueLookup::started(
                    code,
                    providers,
                    |source, barcode| Effect::LookupBarcode { source, barcode },
                    effects,
                )
            })
            .collect(),
    }
}

/// Ask every provider about one chosen catalog number.
pub(super) fn start_catalog_lookup(
    catalog: &str,
    providers: &[Catalog],
    effects: &mut Vec<Effect>,
) -> ValueLookup {
    ValueLookup::started(
        catalog,
        providers,
        |source, catalog| Effect::LookupCatalog { source, catalog },
        effects,
    )
}

/// Ask every provider about every chosen catalog number.
pub(super) fn start_catalog_progress(
    chosen: &[String],
    providers: &[Catalog],
    effects: &mut Vec<Effect>,
) -> CatalogProgress {
    if chosen.is_empty() {
        return CatalogProgress::Skipped;
    }
    CatalogProgress::Lookups {
        values: chosen
            .iter()
            .map(|value| start_catalog_lookup(value, providers, effects))
            .collect(),
    }
}

/// Where a run's title search starts.
pub(super) fn search_progress_at_start(search_by_title: bool) -> SearchProgress {
    if search_by_title {
        SearchProgress::Pending
    } else {
        SearchProgress::NotAsked {
            reason: NotAskedReason::SwitchedOff,
        }
    }
}

/// Ask every provider the candidate's title, unless the identifiers already
/// answered or there is no title. The ISRCs are no identifier here: a
/// recording is on every compilation that reissued it, and MusicBrainz lists
/// few of an album's codes, so what they return does not stand for the album
/// the way a disc ID, a barcode or a catalog number does — and Discogs, which
/// no ISRC reaches, is still asked the title.
pub(super) fn start_search_progress(
    context: &SignalsContext,
    effects: &mut Vec<Effect>,
) -> SearchProgress {
    let identifiers_answered = !context.disc.results.is_empty()
        || !context.barcode.results.is_empty()
        || !context.catalog.active_results().is_empty();
    let Some(query) = context.search.query.clone() else {
        return SearchProgress::Skipped;
    };
    if identifiers_answered || context.providers.is_empty() {
        return SearchProgress::Skipped;
    }
    SearchProgress::Lookups {
        providers: context
            .providers
            .iter()
            .map(|&source| {
                effects.push(Effect::SearchTitle {
                    source,
                    query: query.clone(),
                });
                ProviderLookup {
                    source,
                    state: LookupState::LookingUp,
                }
            })
            .collect(),
    }
}
