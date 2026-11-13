//! The catalog numbers a run searches: the ones the person picked, and the
//! ones that come into effect as found releases carry what the folder
//! states, each searched under the artists its catalog's search names.

use super::progress::{
    answer_provider, ask_every, providers_failures, providers_results, providers_settled,
};
use super::{Effect, LookupOutcome, LookupResults, LookupState, ProviderLookup, SignalsContext};
use crate::row_facts::FolderFacts;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SourceFailure};
use bae_types::pressing::ReleaseLabel;

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
        (Some(searched), Some(wanted)) => bae_types::text_match::same_artist_name(searched, wanted),
        (Some(_), None) => false,
    }
}

/// Whether a search of `catalog` by catalog number names an artist: on
/// Discogs alone. Discogs serves a catalog-number search a page of releases
/// at a time and every page is read, so naming the artist leaves fewer
/// pages; MusicBrainz answers it in one, so naming it there saves no request
/// and adds one per artist.
pub(super) fn names_artist(catalog: LookupCatalog) -> bool {
    catalog == LookupCatalog::Discogs
}

/// The artist `result` credits, as a search names it: its artist line as the
/// record states it. `None` where the record names nobody, or credits a
/// compilation to various artists, which names no artist a catalog matches
/// its other releases by.
pub(super) fn credited_artist(result: &MetadataResult) -> Option<String> {
    let line = result.artist.as_ref()?.plain_text();
    let artist = line.trim();
    if bae_types::text_match::artist_name_key(artist).is_empty()
        || bae_types::text_match::is_various_artists(artist)
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
        providers: &[LookupCatalog],
        effects: &mut Vec<Effect>,
    ) -> Self {
        let mut lookup = Self { value: value.to_string(), searches: Vec::new() };
        lookup.search(artist, providers, effects);
        lookup
    }

    /// Search the number under `artist` as well, on `providers`.
    fn search(
        &mut self,
        artist: Option<String>,
        providers: &[LookupCatalog],
        effects: &mut Vec<Effect>,
    ) {
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
        source: LookupCatalog,
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
        self.searches.iter().all(|search| providers_settled(&search.providers))
    }

    /// Every release a search of the number returned, each once.
    pub fn results(&self) -> LookupResults {
        crate::combine::union_all(
            &self
                .searches
                .iter()
                .map(|search| providers_results(&search.providers))
                .collect::<Vec<_>>(),
        )
    }

    pub(super) fn failures(&self) -> Vec<SourceFailure> {
        self.searches.iter().flat_map(|search| providers_failures(&search.providers)).collect()
    }

    /// Whether any search of the number asked `source`.
    pub(super) fn asks(&self, source: LookupCatalog) -> bool {
        self.searches
            .iter()
            .any(|search| search.providers.iter().any(|provider| provider.source == source))
    }

    /// Whether a search of the number already holds every release one
    /// asking `source` under `artist` would return.
    pub(super) fn covers(&self, source: LookupCatalog, artist: Option<&str>) -> bool {
        self.searches.iter().any(|search| {
            search.covers(artist)
                && search.providers.iter().any(|provider| provider.source == source)
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
        self.lookups().iter().flat_map(CatalogLookup::results).collect()
    }

    /// Every provider that failed any chosen number's lookup.
    pub fn failures(&self) -> Vec<SourceFailure> {
        self.lookups().iter().flat_map(CatalogLookup::failures).collect()
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
        providers: &[LookupCatalog],
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

/// Ask every provider about every number the person picked, each as it
/// stands: no release the run found carries it yet to say whose it is.
pub(super) fn start_catalog_progress(
    chosen: &[String],
    providers: &[LookupCatalog],
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

/// The searches by catalog number the run's results call for, each with
/// the artist it names and the providers it asks. A number is in effect once
/// the folder states it and a found release carries it; it comes into effect
/// as the first release carrying it writes it, and every search of it keeps
/// that spelling. The folder states the numbers its highest-standing text
/// stating any of the found releases' numbers states — the rule the rows'
/// catalog agreement reads (see [`FolderFacts`]): a booklet prints page
/// numbers and other records' numbers as readily as its own, and a found
/// release carrying one of those is no reason to search it. A number the
/// person picked is searched as it stands, which covers every artist.
///
/// Each number in effect is searched on every provider under each artist
/// that provider's search names (see [`names_artist`]) that no search of the
/// number there covers yet. On Discogs that is the album's artist: each
/// offered row's (see [`album_artists`]), whichever release carries the
/// number — a short number is printed by hundreds of releases of other
/// artists, and searching under each of theirs is what narrowing is for not
/// to do. While no row is offered, nothing names the album's artist: a
/// number Discogs has not been asked about is searched alone, and one it has
/// is not asked again.
pub(super) fn catalog_searches_due(
    context: &SignalsContext,
    catalog: &CatalogProgress,
    rows: &[Vec<MetadataResult>],
) -> Vec<(String, Option<String>, Vec<LookupCatalog>)> {
    let answers = context.lookup_results();
    let facts = FolderFacts::of(&context.text, answers.all());
    let mut numbers: Vec<String> = context.catalog.confirmed.clone();
    for number in
        answers.all().flat_map(|result| &result.labels).filter_map(ReleaseLabel::catalog_number)
    {
        let key = bae_types::text_match::squash(number);
        if facts.states_catalog(number)
            && !context.catalog.holds(number)
            && !numbers.iter().any(|held| bae_types::text_match::squash(held) == key)
        {
            numbers.push(number.to_string());
        }
    }
    let album = album_artists(rows);
    let mut due: Vec<(String, Option<String>, Vec<LookupCatalog>)> = Vec::new();
    for number in &numbers {
        let lookup = catalog.lookups().iter().find(|lookup| &lookup.value == number);
        for &source in &context.providers {
            let artists = if !names_artist(source) {
                vec![None]
            } else if rows.is_empty() {
                if lookup.is_some_and(|lookup| lookup.asks(source)) {
                    Vec::new()
                } else {
                    vec![None]
                }
            } else {
                album.clone()
            };
            for artist in artists {
                if lookup.is_some_and(|lookup| lookup.covers(source, artist.as_deref())) {
                    continue;
                }
                match due.iter_mut().find(|(due, asked, _)| due == number && *asked == artist) {
                    Some((_, _, providers)) => providers.push(source),
                    None => due.push((number.clone(), artist, vec![source])),
                }
            }
        }
    }
    due
}

/// The artist of each of `rows`, each once however it is cased or spaced:
/// the first its records credit (see [`credited_artist`]). A row whose
/// records credit no one, or various artists, names none, and then the
/// number is searched alone, which holds every artist's releases.
fn album_artists(rows: &[Vec<MetadataResult>]) -> Vec<Option<String>> {
    let mut artists: Vec<String> = Vec::new();
    for row in rows {
        let Some(artist) = row.iter().find_map(credited_artist) else {
            return vec![None];
        };
        if !artists.iter().any(|held| bae_types::text_match::same_artist_name(held, &artist)) {
            artists.push(artist);
        }
    }
    artists.into_iter().map(Some).collect()
}

#[cfg(test)]
#[path = "numbers_tests.rs"]
mod tests;
