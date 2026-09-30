//! The one key a run derives from its own rows: an offered row's pressing,
//! looked up on each lookup catalog the row has no record of.
//!
//! A row is asked for by the release its page names on that catalog, where
//! it names one; failing that, by its barcode; failing that, by its catalog
//! number under its label. Both ways round: a Discogs row is looked up on
//! MusicBrainz as a MusicBrainz row is on Discogs, though only a MusicBrainz
//! page names another catalog's release. What comes back is pooled with every
//! other key's answers and paired into rows the way they are (see
//! `release_group::group_results`), so a release that is the row's pressing
//! joins it, and one that is not stands as a row of its own.

use super::{LookupOutcome, LookupState};
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{Catalog, MetadataRef};
use crate::text_match::{catalog_key, same_label_name};

/// What an offered row's pressing is asked for by on one catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PressingKey {
    /// The release the row's page names as the same release on that
    /// catalog, read directly.
    Link { release: MetadataRef },
    /// A barcode the row prints.
    Barcode { barcode: String },
    /// A catalog number the row prints under `label`; only a release printing
    /// it under the same label answers.
    CatalogNumber { number: String, label: String },
}

/// One catalog's lookup of one offered row's pressing.
#[derive(Clone, Debug, PartialEq)]
pub struct PressingLookup {
    pub source: Catalog,
    pub key: PressingKey,
    pub state: LookupState,
}

impl PressingLookup {
    /// Land `source`'s answer about `key`, where it is still being waited
    /// for. A catalog-number search is kept to the releases printing the
    /// number under the row's label: both catalogs match numbers loosely, and
    /// a label reuses no number, where two labels may print one.
    pub(super) fn answer(&mut self, outcome: LookupOutcome) {
        if self.state != LookupState::LookingUp {
            return;
        }
        self.state = match outcome {
            Ok(mut results) => {
                if let PressingKey::CatalogNumber { number, label } = &self.key {
                    results.retain(|(result, _)| prints_under(result, number, label));
                }
                LookupState::Done { results }
            }
            Err(failure) => LookupState::Failed { failure },
        };
    }

    pub(super) fn is_settled(&self) -> bool {
        self.state != LookupState::LookingUp
    }

    pub fn results(&self) -> super::LookupResults {
        match &self.state {
            LookupState::Done { results } => results.clone(),
            LookupState::LookingUp | LookupState::Failed { .. } => Vec::new(),
        }
    }

    pub(super) fn failure(&self) -> Option<SourceFailure> {
        match &self.state {
            LookupState::Failed { failure } => Some(SourceFailure {
                source: self.source,
                failure: failure.clone(),
            }),
            LookupState::LookingUp | LookupState::Done { .. } => None,
        }
    }
}

/// Whether `result` prints `number` under the label `label` names.
fn prints_under(result: &MetadataResult, number: &str, label: &str) -> bool {
    let asked = catalog_key(number);
    result.labels.iter().any(|stated| {
        stated
            .catalog_number()
            .and_then(catalog_key)
            .is_some_and(|key| Some(key) == asked)
            && stated
                .name()
                .is_some_and(|name| same_label_name(name, label))
    })
}

/// What `row`'s pressing is asked for by on each of `providers`' lookup
/// catalogs it has no record of, where it states anything to ask by.
pub(super) fn keys_of(
    row: &[MetadataResult],
    providers: &[Catalog],
) -> Vec<(Catalog, PressingKey)> {
    Catalog::LOOKUP
        .into_iter()
        .filter(|catalog| providers.contains(catalog))
        .filter(|catalog| !row.iter().any(|record| record.source == *catalog))
        .filter_map(|catalog| {
            let link = row
                .iter()
                .flat_map(|record| &record.links)
                .find(|release| release.catalog == catalog)
                .map(|release| PressingKey::Link {
                    release: release.clone(),
                });
            let barcode = || {
                row.iter()
                    .flat_map(|record| &record.barcodes)
                    .next()
                    .map(|barcode| PressingKey::Barcode {
                        barcode: barcode.clone(),
                    })
            };
            let number = || {
                row.iter()
                    .flat_map(|record| &record.labels)
                    .find_map(|label| {
                        Some(PressingKey::CatalogNumber {
                            number: label.catalog_number()?.to_string(),
                            label: label.name()?.to_string(),
                        })
                    })
            };
            link.or_else(barcode)
                .or_else(number)
                .map(|key| (catalog, key))
        })
        .collect()
}
