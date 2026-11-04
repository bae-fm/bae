//! The keys a run derives from its own rows: an offered row's pressing,
//! looked up on each lookup catalog the row has no record of.
//!
//! A row is asked for by the release its page names on that catalog, where
//! it names one: that page says which release it is. Otherwise by every
//! barcode it prints and every catalog number it prints under a label, each
//! a key of its own, a number on Discogs under the artist its record
//! credits.
//! Both ways round: a Discogs row is looked up on MusicBrainz as a
//! MusicBrainz row is on Discogs, though only a MusicBrainz page names
//! another catalog's release. What comes back is pooled with every other
//! key's answers and paired into rows the way they are (see
//! `release_group::group_results`), so a release that is the row's pressing
//! joins it, and one that is not stands as a row of its own.

use super::{LookupOutcome, LookupState};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::MetadataRef;
use bae_catalogs::search::{MetadataResult, SourceFailure};
use bae_types::pressing::ReleaseLabel;

/// What an offered row's pressing is asked for by on one catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PressingKey {
    /// The release the row's page names as the same release on that
    /// catalog, read directly.
    Link { release: bae_catalogs::records::LookupRef },
    /// A barcode the row prints.
    Barcode { barcode: String },
    /// A catalog number the row prints under `label`; only a release printing
    /// it under the same label answers. On Discogs the search names
    /// `artist`, the one the record printing it credits; `None` on
    /// MusicBrainz, and where that record credits no one, or various
    /// artists.
    CatalogNumber { number: String, label: String, artist: Option<String> },
}

/// One catalog's lookup of one offered row's pressing.
#[derive(Clone, Debug, PartialEq)]
pub struct PressingLookup {
    pub source: LookupCatalog,
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
                if let PressingKey::CatalogNumber { number, label, .. } = &self.key {
                    let asked = ReleaseLabel::new(Some(label.clone()), Some(number.clone()));
                    results.retain(|result| {
                        asked.as_ref().is_some_and(|asked| {
                            result.labels.iter().any(|stated| stated.prints_number_of(asked))
                        })
                    });
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
            LookupState::Failed { failure } => {
                Some(SourceFailure { source: self.source.catalog(), failure: failure.clone() })
            }
            LookupState::LookingUp | LookupState::Done { .. } => None,
        }
    }
}

/// What `row`'s pressing is asked for by on each of `providers`' lookup
/// catalogs it has no record of: the release its page names there, else
/// every barcode and every labelled catalog number it prints.
pub(super) fn keys_of(
    row: &[MetadataResult],
    providers: &[LookupCatalog],
) -> Vec<(LookupCatalog, PressingKey)> {
    LookupCatalog::ALL
        .into_iter()
        .filter(|catalog| providers.contains(catalog))
        .filter(|catalog| !row.iter().any(|record| record.source == *catalog))
        .flat_map(|catalog| {
            let link = row
                .iter()
                .flat_map(|record| &record.links)
                .filter_map(MetadataRef::lookup)
                .find(|release| release.catalog == catalog);
            let keys: Vec<PressingKey> = match link {
                Some(release) => vec![PressingKey::Link { release }],
                None => {
                    let barcodes = row
                        .iter()
                        .flat_map(|record| &record.barcodes)
                        .map(|barcode| PressingKey::Barcode { barcode: barcode.clone() });
                    let numbers = row.iter().flat_map(|record| {
                        record.labels.iter().filter_map(move |label| {
                            Some(PressingKey::CatalogNumber {
                                number: label.catalog_number()?.to_string(),
                                label: label.name()?.to_string(),
                                artist: super::numbers::names_artist(catalog)
                                    .then(|| super::numbers::credited_artist(record))
                                    .flatten(),
                            })
                        })
                    });
                    barcodes.chain(numbers).collect()
                }
            };
            keys.into_iter().map(move |key| (catalog, key))
        })
        .collect()
}

#[cfg(test)]
#[path = "pressings_tests.rs"]
mod tests;
