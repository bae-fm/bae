//! One round of a run, taken each time everything it asked has answered.
//!
//! Every answer so far is pooled and ranked (see `combine`), and the round
//! asks what came of it: each catalog number a found release carries — on
//! any row, offered or not — that the folder states is searched on every
//! catalog, on Discogs under the artist of each row the run offers; each
//! offered record not read yet has its document read — from the Disc ID
//! answer that returned it, where one did; and each offered row whose records
//! are read is looked up on the lookup catalogs it has no record of (see
//! [`super::pressings`]). A round asks each of these once, and adds releases
//! without taking any away: the ranking is what narrows. A round that asks
//! nothing is the last.

use super::pressings::{keys_of, PressingKey, PressingLookup};
use super::progress::{credited_artist, names_artist};
use super::{
    combined, BarcodeProgress, CatalogProgress, DocumentReading, Effect, LookupState,
    SignalsContext,
};
use crate::identify::documents::ReleaseToRead;
use crate::identify::row_facts::FolderFacts;
use crate::import::search::MetadataResult;
use crate::import::MetadataRef;
use bae_catalogs::catalog::Catalog;
use bae_types::pressing::ReleaseLabel;

/// Take the round the results in hand call for: `catalog` with the numbers
/// that came into effect, and the lookups and reads to dispatch — none when
/// there is nothing left to ask.
pub(super) fn next_round(
    context: &mut SignalsContext,
    mut catalog: CatalogProgress,
    barcode: &BarcodeProgress,
) -> (CatalogProgress, Vec<Effect>) {
    let mut effects = Vec::new();

    let rows = offered_rows(context);
    for (number, artist, providers) in catalog_searches_due(context, &catalog, &rows) {
        if !context.catalog.holds(&number) {
            context.catalog.confirmed.push(number.clone());
        }
        catalog = catalog.searching(&number, artist, &providers, &mut effects);
    }
    let read = |record: &MetadataResult| {
        context.documents.read().iter().any(|reading| {
            reading.release.catalog == record.source && reading.release.key == record.release_id
        })
    };
    let unread: Vec<ReleaseToRead> = rows
        .iter()
        .flatten()
        .filter(|record| !read(record))
        .map(|record| {
            let release = MetadataRef::new(record.source, record.release_id.clone());
            match context.disc.document_of(&release) {
                Some(answered) => ReleaseToRead::DiscIdAnswer(answered.clone()),
                None => ReleaseToRead::ReleaseLookup(release),
            }
        })
        .collect();

    let mut pressings: Vec<PressingLookup> = Vec::new();
    let pool = context.lookup_results();
    for row in rows.iter().filter(|row| row.iter().all(read)) {
        for (source, key) in keys_of(row, &context.providers) {
            let asked = context
                .pressings
                .iter()
                .chain(&pressings)
                .any(|lookup| lookup.source == source && lookup.key == key);
            let answered = match &key {
                PressingKey::Link { release } => pool.all().any(|(result, _)| {
                    result.source == release.catalog && result.release_id == release.key
                }),
                PressingKey::Barcode { barcode: code } => barcode.lookups().iter().any(|lookup| {
                    same_barcode(&lookup.value, code) && asked_of(&lookup.providers, source)
                }),
                PressingKey::CatalogNumber { number, artist, .. } => {
                    catalog.lookups().iter().any(|lookup| {
                        super::number_key(&lookup.value) == super::number_key(number)
                            && lookup.covers(source, artist.as_deref())
                    })
                }
            };
            if !asked && !answered {
                pressings.push(PressingLookup {
                    source,
                    key,
                    state: LookupState::LookingUp,
                });
            }
        }
    }

    if !unread.is_empty() {
        context.documents = DocumentReading::Reading(context.documents.read().to_vec());
        effects.push(Effect::ReadReleases {
            releases: unread,
            audio: context
                .read_audio()
                .expect("a run reads documents only once its audio is measured"),
        });
    }
    for lookup in pressings {
        effects.push(Effect::LookupPressing {
            source: lookup.source,
            key: lookup.key.clone(),
        });
        context.pressings.push(lookup);
    }
    (catalog, effects)
}

/// The rows the run offers as its results stand, each as its records with
/// their documents applied.
pub(super) fn offered_rows(context: &SignalsContext) -> Vec<Vec<MetadataResult>> {
    let findings = combined(context).0;
    let mut rows: Vec<(u32, Vec<MetadataResult>)> = Vec::new();
    for (result, row) in findings.matches.into_iter().zip(findings.pressings) {
        match rows.iter_mut().find(|(numbered, _)| *numbered == row) {
            Some((_, records)) => records.push(result),
            None => rows.push((row, vec![result])),
        }
    }
    rows.into_iter().map(|(_, records)| records).collect()
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
fn catalog_searches_due(
    context: &SignalsContext,
    catalog: &CatalogProgress,
    rows: &[Vec<MetadataResult>],
) -> Vec<(String, Option<String>, Vec<Catalog>)> {
    let answers = context.lookup_results();
    let facts = FolderFacts::of(&context.text, answers.all().map(|(result, _)| result));
    let mut numbers: Vec<String> = context.catalog.confirmed.clone();
    for number in answers
        .all()
        .flat_map(|(result, _)| &result.labels)
        .filter_map(ReleaseLabel::catalog_number)
    {
        let key = super::number_key(number);
        if facts.states_catalog(number)
            && !context.catalog.holds(number)
            && !numbers.iter().any(|held| super::number_key(held) == key)
        {
            numbers.push(number.to_string());
        }
    }
    let album = album_artists(rows);
    let mut due: Vec<(String, Option<String>, Vec<Catalog>)> = Vec::new();
    for number in &numbers {
        let lookup = catalog
            .lookups()
            .iter()
            .find(|lookup| &lookup.value == number);
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
                match due
                    .iter_mut()
                    .find(|(due, asked, _)| due == number && *asked == artist)
                {
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
        if !artists
            .iter()
            .any(|held| bae_types::text_match::same_artist_name(held, &artist))
        {
            artists.push(artist);
        }
    }
    artists.into_iter().map(Some).collect()
}

fn asked_of(providers: &[super::ProviderLookup], source: Catalog) -> bool {
    providers.iter().any(|provider| provider.source == source)
}

/// Whether two printed barcodes are one code.
fn same_barcode(a: &str, b: &str) -> bool {
    a == b
        || bae_types::barcode::comparison_key(a)
            .ok()
            .is_some_and(|key| bae_types::barcode::comparison_key(b).ok() == Some(key))
}
