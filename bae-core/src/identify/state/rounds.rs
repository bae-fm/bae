//! One round of a run, taken each time everything it asked has answered.
//!
//! Every answer so far is pooled and ranked (see `combine`), and the round
//! asks what came of it: each catalog number a found release carries — on
//! any row, offered or not — that the folder states is searched on every
//! catalog; each offered record not read yet has its full document read; and
//! each offered row whose records are read is looked up on the lookup
//! catalogs it has no record of (see [`super::pressings`]). A round asks
//! each of these once, and adds releases without taking any away: the
//! ranking is what narrows. A round that asks nothing is the last.

use super::pressings::{keys_of, PressingKey, PressingLookup};
use super::progress::start_catalog_lookup;
use super::{
    combined, BarcodeProgress, CatalogProgress, DocumentReading, Effect, LookupState,
    SignalsContext,
};
use crate::identify::row_facts::FolderFacts;
use crate::import::search::MetadataResult;
use crate::import::{Catalog, MetadataRef};
use crate::pressing::ReleaseLabel;

/// Take the round the results in hand call for: `catalog` with the numbers
/// that came into effect, and the lookups and reads to dispatch — none when
/// there is nothing left to ask.
pub(super) fn next_round(
    context: &mut SignalsContext,
    mut catalog: CatalogProgress,
    barcode: &BarcodeProgress,
) -> (CatalogProgress, Vec<Effect>) {
    let mut effects = Vec::new();

    for number in newly_confirmed(context) {
        catalog = catalog.with(start_catalog_lookup(
            &number,
            &context.providers,
            &mut effects,
        ));
        context.catalog.confirmed.push(number);
    }

    let rows = offered_rows(context);
    let read = |record: &MetadataResult| {
        context.documents.read().iter().any(|reading| {
            reading.release.catalog == record.source && reading.release.key == record.release_id
        })
    };
    let unread: Vec<MetadataRef> = rows
        .iter()
        .flatten()
        .filter(|record| !read(record))
        .map(|record| MetadataRef::new(record.source, record.release_id.clone()))
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
                PressingKey::CatalogNumber { number, .. } => {
                    catalog.lookups().iter().any(|lookup| {
                        super::number_key(&lookup.value) == super::number_key(number)
                            && asked_of(&lookup.providers, source)
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
            track_lengths_ms: context.audio.track_lengths_ms.clone(),
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

/// The catalog numbers not yet in effect that a release the run found
/// carries and the folder states, each once, as the first release carrying
/// it writes it. The folder states the numbers its highest-standing text
/// stating any of the found releases' numbers states — the rule the rows'
/// catalog agreement reads (see [`FolderFacts`]): a booklet prints page
/// numbers and other records' numbers as readily as its own, and a found
/// release carrying one of those is no reason to search it.
fn newly_confirmed(context: &SignalsContext) -> Vec<String> {
    let answers = context.lookup_results();
    let facts = FolderFacts::of(&context.text, answers.all().map(|(result, _)| result));
    let mut numbers: Vec<String> = Vec::new();
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
    numbers
}

fn asked_of(providers: &[super::ProviderLookup], source: Catalog) -> bool {
    providers.iter().any(|provider| provider.source == source)
}

/// Whether two printed barcodes are one code.
fn same_barcode(a: &str, b: &str) -> bool {
    a == b
        || crate::barcode::comparison_key(a)
            .ok()
            .is_some_and(|key| crate::barcode::comparison_key(b).ok() == Some(key))
}
