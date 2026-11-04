//! One round of a run, taken each time everything it asked has answered.
//!
//! Every answer so far is pooled and ranked (see `combine`), and the round
//! asks what came of it: each catalog number a found release carries — on
//! any row, offered or not — that the folder states is searched on every
//! catalog, on Discogs under the artist of each row the run offers (see
//! [`super::numbers`]); each
//! offered record not read yet has its document read — from the Disc ID
//! answer that returned it, where one did; and each offered row whose records
//! are read is looked up on the lookup catalogs it has no record of (see
//! [`super::pressings`]). A round asks each of these once, and adds releases
//! without taking any away: the ranking is what narrows. A round that asks
//! nothing is the last.

use super::numbers::catalog_searches_due;
use super::pressings::{keys_of, PressingKey, PressingLookup};
use super::{
    combined, BarcodeProgress, CatalogProgress, DocumentReading, Effect, LookupState,
    SignalsContext,
};
use crate::documents::ReleaseToRead;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;

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
        context.documents.read().iter().any(|reading| reading.release == record.release())
    };
    let unread: Vec<ReleaseToRead> = rows
        .iter()
        .flatten()
        .filter(|record| !read(record))
        .map(|record| {
            let release = record.release();
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
                PressingKey::Link { release } => {
                    pool.all().any(|result| result.release() == *release)
                }
                PressingKey::Barcode { barcode: code } => barcode.lookups().iter().any(|lookup| {
                    bae_types::barcode::same_product(&lookup.value, code)
                        && asked_of(&lookup.providers, source)
                }),
                PressingKey::CatalogNumber { number, artist, .. } => {
                    catalog.lookups().iter().any(|lookup| {
                        bae_types::text_match::squash(&lookup.value)
                            == bae_types::text_match::squash(number)
                            && lookup.covers(source, artist.as_deref())
                    })
                }
            };
            if !asked && !answered {
                pressings.push(PressingLookup { source, key, state: LookupState::LookingUp });
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
        effects.push(Effect::LookupPressing { source: lookup.source, key: lookup.key.clone() });
        context.pressings.push(lookup);
    }
    (catalog, effects)
}

/// The rows the run offers as its results stand, each as its records with
/// their documents applied.
pub(super) fn offered_rows(context: &SignalsContext) -> Vec<Vec<MetadataResult>> {
    let findings = combined(context);
    let mut rows: Vec<(u32, Vec<MetadataResult>)> = Vec::new();
    for (result, row) in findings.matches.into_iter().zip(findings.pressings) {
        match rows.iter_mut().find(|(numbered, _)| *numbered == row) {
            Some((_, records)) => records.push(result),
            None => rows.push((row, vec![result])),
        }
    }
    rows.into_iter().map(|(_, records)| records).collect()
}

fn asked_of(providers: &[super::ProviderLookup], source: LookupCatalog) -> bool {
    providers.iter().any(|provider| provider.source == source)
}

#[cfg(test)]
#[path = "rounds_tests.rs"]
mod tests;
