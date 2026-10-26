//! How each step's lookup progresses, and the toolbar badge each one shows.
//!
//! Every provider answers for itself, so a lookup holds one entry per provider
//! and settles once all of them have.

use super::{Effect, LookupOutcome, SignalState, SignalsContext};
use crate::catalog::Catalog;
use crate::db::LibraryStatus;
use crate::identify::NotAskedReason;
use crate::import::search::{MetadataResult, SourceFailure};
use crate::signals::{DiscIdSignal, LookupFailure};

/// What one lookup produced: each match paired with its library status.
pub type LookupResults = Vec<(MetadataResult, LibraryStatus)>;

/// What MusicBrainz answered about one Disc ID: the releases holding the
/// disc, and each one's document as the answer states it, which the run reads
/// the release from.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscIdAnswer {
    pub results: LookupResults,
    pub documents: Vec<crate::import::payloads::DiscIdRelease>,
}

#[cfg(test)]
impl DiscIdAnswer {
    /// An answer of `results` whose documents a test does not read.
    pub(crate) fn for_test(results: LookupResults) -> Self {
        Self {
            results,
            documents: Vec::new(),
        }
    }
}

/// The disc-ID lookups' progress. Only MusicBrainz answers disc IDs, so each
/// of the candidate's disc IDs has one lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum DiscidProgress {
    Computing,
    /// One lookup per disc ID, in the signal's order.
    Lookups {
        discs: Vec<DiscLookup>,
    },
    /// No disc ID was derived.
    Skipped,
    /// Disc IDs were derived and nobody was asked about them, for `reason`.
    NotAsked {
        reason: NotAskedReason,
    },
}

/// One disc ID's lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscLookup {
    pub disc_id: String,
    pub state: LookupState,
}

impl DiscidProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            DiscidProgress::Computing => false,
            DiscidProgress::Lookups { discs } => discs.iter().all(|disc| disc.state.is_settled()),
            DiscidProgress::Skipped | DiscidProgress::NotAsked { .. } => true,
        }
    }

    /// What each disc ID's lookup found, in disc order; a lookup that has
    /// not answered, or failed, found nothing.
    pub fn answers(&self) -> Vec<LookupResults> {
        self.lookups()
            .iter()
            .map(|disc| match &disc.state {
                LookupState::Done { results } => results.clone(),
                LookupState::LookingUp | LookupState::Failed { .. } => Vec::new(),
            })
            .collect()
    }

    /// Every disc ID lookup's failure, each distinct one once.
    pub fn failures(&self) -> Vec<LookupFailure> {
        let mut failures: Vec<LookupFailure> = Vec::new();
        for disc in self.lookups() {
            if let LookupState::Failed { failure } = &disc.state {
                if !failures.contains(failure) {
                    failures.push(failure.clone());
                }
            }
        }
        failures
    }

    pub fn lookups(&self) -> &[DiscLookup] {
        match self {
            DiscidProgress::Lookups { discs } => discs,
            DiscidProgress::Computing
            | DiscidProgress::Skipped
            | DiscidProgress::NotAsked { .. } => &[],
        }
    }

    /// Land the answer about `disc_id`, where it is still being waited for,
    /// returning the documents it answered with; `None` where nothing was
    /// waiting for it.
    pub(super) fn answer(
        &mut self,
        disc_id: &str,
        outcome: Result<DiscIdAnswer, LookupFailure>,
    ) -> Option<Vec<crate::import::payloads::DiscIdRelease>> {
        let DiscidProgress::Lookups { discs } = self else {
            return None;
        };
        let disc = discs
            .iter_mut()
            .find(|disc| disc.disc_id == disc_id && disc.state == LookupState::LookingUp)?;
        let (state, documents) = match outcome {
            Ok(DiscIdAnswer { results, documents }) => (LookupState::Done { results }, documents),
            Err(failure) => (LookupState::Failed { failure }, Vec::new()),
        };
        disc.state = state;
        Some(documents)
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
    /// One lookup per provider in the run.
    Lookups { providers: Vec<ProviderLookup> },
}

impl SearchProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            SearchProgress::Pending => false,
            SearchProgress::Skipped => true,
            SearchProgress::Lookups { providers } => providers.iter().all(|l| l.state.is_settled()),
        }
    }

    /// What every provider that answered found, in provider order.
    pub fn results(&self) -> LookupResults {
        providers_results(self.lookups())
    }

    /// The providers that failed.
    pub fn failures(&self) -> Vec<SourceFailure> {
        providers_failures(self.lookups())
    }

    /// The lookups, in provider order.
    pub fn lookups(&self) -> &[ProviderLookup] {
        match self {
            SearchProgress::Lookups { providers } => providers,
            SearchProgress::Pending | SearchProgress::Skipped => &[],
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
            providers: ask_every(providers, |source| ask(source, value.to_string()), effects),
        }
    }

    /// Land `source`'s answer, where it is still being waited for.
    pub(super) fn answer(&mut self, source: Catalog, outcome: LookupOutcome) {
        answer_provider(&mut self.providers, source, outcome);
    }

    fn is_settled(&self) -> bool {
        providers_settled(&self.providers)
    }

    fn results(&self) -> LookupResults {
        providers_results(&self.providers)
    }

    fn failures(&self) -> Vec<SourceFailure> {
        providers_failures(&self.providers)
    }
}

/// Ask every one of `providers`, each through the effect `ask` makes.
fn ask_every(
    providers: &[Catalog],
    ask: impl Fn(Catalog) -> Effect,
    effects: &mut Vec<Effect>,
) -> Vec<ProviderLookup> {
    providers
        .iter()
        .map(|&source| {
            effects.push(ask(source));
            ProviderLookup {
                source,
                state: LookupState::LookingUp,
            }
        })
        .collect()
}

/// Land `source`'s answer among `providers`, where it is still being waited
/// for.
fn answer_provider(providers: &mut [ProviderLookup], source: Catalog, outcome: LookupOutcome) {
    if let Some(lookup) = providers
        .iter_mut()
        .find(|l| l.source == source && l.state == LookupState::LookingUp)
    {
        lookup.state = match outcome {
            Ok(results) => LookupState::Done { results },
            Err(failure) => LookupState::Failed { failure },
        };
    }
}

fn providers_settled(providers: &[ProviderLookup]) -> bool {
    providers.iter().all(|l| l.state.is_settled())
}

/// What every provider that answered found, in provider order.
fn providers_results(providers: &[ProviderLookup]) -> LookupResults {
    providers
        .iter()
        .filter_map(|l| match &l.state {
            LookupState::Done { results } => Some(results.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn providers_failures(providers: &[ProviderLookup]) -> Vec<SourceFailure> {
    providers
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

/// Every search of one catalog number in effect. A number the person picked
/// is searched as it stands, once on every provider. A number a found
/// release carries is searched as it stands on MusicBrainz, and on Discogs
/// under the artist of each row the run offers: a short number is printed by
/// hundreds of unrelated releases, which Discogs serves a page at a time, and
/// the ones the run looks for share the album's artist.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogLookup {
    pub value: String,
    /// In the order asked.
    pub searches: Vec<CatalogSearch>,
}

/// The search of a catalog number under one artist, on the providers asked
/// it.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogSearch {
    /// The artist the search names; `None` searches the number alone.
    pub artist: Option<String>,
    pub providers: Vec<ProviderLookup>,
}

impl CatalogSearch {
    /// Whether what this search returns holds every release a search under
    /// `artist` would.
    pub(super) fn covers(&self, artist: Option<&str>) -> bool {
        search_covers(self.artist.as_deref(), artist)
    }
}

/// Whether a search naming `searched` returns every release one naming
/// `wanted` would: it names no artist, or the same one.
fn search_covers(searched: Option<&str>, wanted: Option<&str>) -> bool {
    match (searched, wanted) {
        (None, _) => true,
        (Some(searched), Some(wanted)) => crate::text_match::same_artist_name(searched, wanted),
        (Some(_), None) => false,
    }
}

/// Whether a search of `catalog` by catalog number names an artist: on
/// Discogs alone. Discogs serves a catalog-number search a page of releases
/// at a time and every page is read, so naming the artist leaves fewer
/// pages; MusicBrainz answers it in one, so naming it there saves no request
/// and adds one per artist.
pub(super) fn names_artist(catalog: Catalog) -> bool {
    catalog == Catalog::Discogs
}

/// The artist `result` credits, as a search names it: its artist line as the
/// record states it. `None` where the record names nobody, or credits a
/// compilation to various artists, which names no artist a catalog matches
/// its other releases by.
pub(super) fn credited_artist(result: &MetadataResult) -> Option<String> {
    let artist = result.artist.as_deref()?.trim();
    if crate::text_match::artist_name_key(artist).is_empty()
        || crate::text_match::is_various_artists(artist)
    {
        return None;
    }
    Some(artist.to_string())
}

impl CatalogLookup {
    /// Search `value` under `artist`, on `providers`.
    fn started(
        value: &str,
        artist: Option<String>,
        providers: &[Catalog],
        effects: &mut Vec<Effect>,
    ) -> Self {
        let mut lookup = Self {
            value: value.to_string(),
            searches: Vec::new(),
        };
        lookup.search(artist, providers, effects);
        lookup
    }

    /// Search the number under `artist` as well, on `providers`.
    fn search(&mut self, artist: Option<String>, providers: &[Catalog], effects: &mut Vec<Effect>) {
        let providers = ask_every(
            providers,
            |source| Effect::LookupCatalog {
                source,
                catalog: self.value.clone(),
                artist: artist.clone(),
            },
            effects,
        );
        self.searches.push(CatalogSearch { artist, providers });
    }

    /// Land `source`'s answer to the search under `artist`, where it is still
    /// being waited for.
    pub(super) fn answer(
        &mut self,
        source: Catalog,
        artist: &Option<String>,
        outcome: LookupOutcome,
    ) {
        if let Some(search) = self.searches.iter_mut().find(|search| {
            &search.artist == artist
                && search.providers.iter().any(|provider| {
                    provider.source == source && provider.state == LookupState::LookingUp
                })
        }) {
            answer_provider(&mut search.providers, source, outcome);
        }
    }

    fn is_settled(&self) -> bool {
        self.searches
            .iter()
            .all(|search| providers_settled(&search.providers))
    }

    /// Every release a search of the number returned, each once.
    pub fn results(&self) -> LookupResults {
        crate::identify::combine::union_all(
            &self
                .searches
                .iter()
                .map(|search| providers_results(&search.providers))
                .collect::<Vec<_>>(),
        )
    }

    pub(super) fn failures(&self) -> Vec<SourceFailure> {
        self.searches
            .iter()
            .flat_map(|search| providers_failures(&search.providers))
            .collect()
    }

    /// Whether any search of the number asked `source`.
    pub(super) fn asks(&self, source: Catalog) -> bool {
        self.searches.iter().any(|search| {
            search
                .providers
                .iter()
                .any(|provider| provider.source == source)
        })
    }

    /// Whether a search of the number already holds every release one
    /// asking `source` under `artist` would return.
    pub(super) fn covers(&self, source: Catalog, artist: Option<&str>) -> bool {
        self.searches.iter().any(|search| {
            search.covers(artist)
                && search
                    .providers
                    .iter()
                    .any(|provider| provider.source == source)
        })
    }
}

/// The searches of the catalog numbers in effect, each number on its own.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogProgress {
    /// No catalog number in effect, so nothing to search.
    Skipped,
    /// One lookup per number in effect, in the order each came into effect.
    Lookups { values: Vec<CatalogLookup> },
}

impl CatalogProgress {
    pub fn is_settled(&self) -> bool {
        match self {
            CatalogProgress::Skipped => true,
            CatalogProgress::Lookups { values } => values.iter().all(CatalogLookup::is_settled),
        }
    }

    /// Every chosen number's results, in chosen order.
    pub fn results(&self) -> LookupResults {
        self.lookups()
            .iter()
            .flat_map(CatalogLookup::results)
            .collect()
    }

    /// Every provider that failed any chosen number's lookup.
    pub fn failures(&self) -> Vec<SourceFailure> {
        self.lookups()
            .iter()
            .flat_map(CatalogLookup::failures)
            .collect()
    }

    /// The chosen numbers' lookups, in chosen order.
    pub fn lookups(&self) -> &[CatalogLookup] {
        match self {
            CatalogProgress::Lookups { values } => values,
            CatalogProgress::Skipped => &[],
        }
    }

    /// This progress with `value` searched under `artist` on `providers` as
    /// well: one more search of a number already searched, or the number's
    /// first.
    pub(super) fn searching(
        self,
        value: &str,
        artist: Option<String>,
        providers: &[Catalog],
        effects: &mut Vec<Effect>,
    ) -> Self {
        let mut values = match self {
            CatalogProgress::Lookups { values } => values,
            CatalogProgress::Skipped => Vec::new(),
        };
        match values.iter_mut().find(|lookup| lookup.value == value) {
            Some(lookup) => lookup.search(artist, providers, effects),
            None => values.push(CatalogLookup::started(value, artist, providers, effects)),
        }
        CatalogProgress::Lookups { values }
    }

    /// This progress with only the lookups `keep` admits.
    pub(super) fn keeping(self, keep: impl Fn(&CatalogLookup) -> bool) -> Self {
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

/// Results beat failures: only lookups that found nothing read as failed.
pub(super) fn discid_progress_state(progress: &DiscidProgress) -> SignalState {
    match progress {
        DiscidProgress::Computing => SignalState::LookingUp,
        DiscidProgress::Lookups { .. } if !progress.is_settled() => SignalState::LookingUp,
        DiscidProgress::Lookups { .. } => {
            let found = crate::identify::combine::union_all(&progress.answers()).len();
            match progress.failures().first() {
                Some(failure) if found == 0 => SignalState::Failed {
                    failure: failure.clone(),
                },
                _ => found_or_no_match(found as u32),
            }
        }
        DiscidProgress::Skipped => SignalState::Skipped,
        DiscidProgress::NotAsked { reason } => SignalState::NotAsked { reason: *reason },
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

/// The disc-ID badge of a settled run. A lookup's failure comes first where
/// no disc ID found anything, since the signal only says whether disc IDs
/// could be computed.
pub(super) fn settled_identity_state(context: &SignalsContext) -> SignalState {
    let found = context.disc.results().len();
    if let Some(failure) = context.disc.failures.first().filter(|_| found == 0) {
        return SignalState::Failed {
            failure: failure.clone(),
        };
    }
    match &context.disc.signal {
        DiscIdSignal::Absent | DiscIdSignal::NotCdAudio => SignalState::Skipped,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose Disc ID could not be derived ended as an error")
        }
        DiscIdSignal::Computed { .. } => match context.disc.not_asked {
            Some(reason) => SignalState::NotAsked { reason },
            None => found_or_no_match(found as u32),
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

/// The catalog badge of a settled run, over every number searched.
pub(super) fn catalog_settled_state(context: &SignalsContext) -> SignalState {
    if context.catalog.searched.is_empty() {
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
    providers: &[Catalog],
    effects: &mut Vec<Effect>,
) -> DiscidProgress {
    match signal {
        DiscIdSignal::Computed { discs } => {
            // The reason nearest the value wins: see `NotAskedReason`.
            let reason = if excluded {
                Some(NotAskedReason::LeftOut)
            } else if !providers.contains(&Catalog::DISC_ID_CATALOG) {
                Some(NotAskedReason::NoCatalog)
            } else {
                None
            };
            if let Some(reason) = reason {
                return DiscidProgress::NotAsked { reason };
            }
            DiscidProgress::Lookups {
                discs: discs
                    .iter()
                    .map(|disc| {
                        effects.push(Effect::LookupDiscid {
                            disc_id: disc.disc_id.clone(),
                        });
                        DiscLookup {
                            disc_id: disc.disc_id.clone(),
                            state: LookupState::LookingUp,
                        }
                    })
                    .collect(),
            }
        }
        DiscIdSignal::Absent | DiscIdSignal::NotCdAudio => DiscidProgress::Skipped,
        DiscIdSignal::Failed { .. } => {
            unreachable!("a run whose Disc ID could not be derived ended as an error")
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
pub(super) fn start_barcode_progress(
    codes: Vec<String>,
    excluded: &[String],
    had_source: bool,
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
    if asked.is_empty() {
        return BarcodeProgress::NotAsked {
            codes,
            reason: NotAskedReason::LeftOut,
        };
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

/// Ask every provider about every number the person picked, each as it
/// stands: no release the run found carries it yet to say whose it is.
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
            .map(|value| CatalogLookup::started(value, None, providers, effects))
            .collect(),
    }
}

/// Ask every provider the candidate's title, unless the identifiers and the
/// picked numbers already answered or there is no title. The ISRCs are no identifier here: a
/// recording is on every compilation that reissued it, and MusicBrainz lists
/// few of an album's codes, so what they return does not stand for the album
/// the way a disc ID, a barcode or a catalog number does — and Discogs, which
/// no ISRC reaches, is still asked the title.
pub(super) fn start_search_progress(
    context: &SignalsContext,
    effects: &mut Vec<Effect>,
) -> SearchProgress {
    let identifiers_answered = !context.disc.results().is_empty()
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
