//! What a pressing row says where its records state a fact differently.
//!
//! A row is one physical pressing under every record that names it, and each
//! record keeps its own facts. Where the records state a fact differently
//! (the year, a catalog number, a barcode, the country, the format) the row
//! lists every record's value and names the catalogs that state each. A fact
//! one record states alone is listed as stated, with no catalog named: there
//! is nothing to tell apart.

use super::Pressing;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;
use bae_types::barcode::comparison_key;
use bae_types::pressing::{MediaCount, ReleaseArea};
use bae_types::text_match::catalog_key;

/// One value the records of a row state for a fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement<T> {
    pub value: T,
    /// The catalogs of the records that state the value, in the order
    /// surfaces list catalogs. Empty unless the records state the fact
    /// differently, so a fact they agree on, or one record states alone, names
    /// no catalog.
    pub catalogs: Vec<LookupCatalog>,
}

/// A label line of a row with each of its catalog numbers as the records
/// state it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatedLabelLine {
    pub names: Vec<String>,
    pub catalog_numbers: Vec<Statement<String>>,
}

/// Every fact a row's records can state differently, as the row lists it. An
/// empty list is a fact no record states, one entry a fact every record that
/// states it states alike, and several the values that differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statements {
    pub years: Vec<Statement<i32>>,
    pub labels: Vec<StatedLabelLine>,
    pub barcodes: Vec<Statement<String>>,
    pub areas: Vec<Statement<ReleaseArea>>,
    pub media: Vec<Statement<Vec<MediaCount>>>,
}

/// The values the records state for one fact, each under the key that says
/// when two spellings are one value, with the catalogs stating it.
struct Stated<K, V> {
    values: Vec<(K, V, Vec<LookupCatalog>)>,
    /// Whether the records that state anything state different sets of
    /// values.
    differ: bool,
}

impl<K: PartialEq, V> Stated<K, V> {
    /// `records` are each record's catalog and the values it states, each
    /// with its key.
    fn of(records: impl IntoIterator<Item = (LookupCatalog, Vec<(K, V)>)>) -> Self {
        let mut values: Vec<(K, V, Vec<LookupCatalog>)> = Vec::new();
        let mut sets: Vec<Vec<usize>> = Vec::new();
        for (catalog, stated) in records {
            let mut set: Vec<usize> = Vec::new();
            for (key, value) in stated {
                let at = match values.iter().position(|(known, ..)| *known == key) {
                    Some(at) => at,
                    None => {
                        values.push((key, value, Vec::new()));
                        values.len() - 1
                    }
                };
                if !values[at].2.contains(&catalog) {
                    values[at].2.push(catalog);
                }
                if !set.contains(&at) {
                    set.push(at);
                }
            }
            if !set.is_empty() {
                set.sort_unstable();
                sets.push(set);
            }
        }
        for (.., catalogs) in &mut values {
            catalogs.sort();
        }
        let differ = sets.windows(2).any(|pair| pair[0] != pair[1]);
        Self { values, differ }
    }

    /// The catalogs that state the value under `key`, named only where the
    /// records differ.
    fn naming(&self, key: &K) -> Vec<LookupCatalog> {
        match self.values.iter().find(|(known, ..)| known == key) {
            Some((_, _, catalogs)) if self.differ => catalogs.clone(),
            _ => Vec::new(),
        }
    }

    fn statements(self) -> Vec<Statement<V>> {
        let differ = self.differ;
        self.values
            .into_iter()
            .map(|(_, value, catalogs)| Statement {
                value,
                catalogs: if differ { catalogs } else { Vec::new() },
            })
            .collect()
    }
}

impl Pressing {
    /// What the row's records state for each fact the row lists them for.
    pub fn statements(&self) -> Statements {
        let records = |stated: fn(&MetadataResult) -> Vec<(String, String)>| {
            Stated::of(self.releases.iter().map(|release| (release.source, stated(release))))
        };
        let numbers = records(|release| {
            release
                .labels
                .iter()
                .filter_map(|label| label.catalog_number())
                .filter_map(|number| Some((catalog_key(number)?, number.to_string())))
                .collect()
        });
        let barcodes = records(|release| {
            release
                .barcodes
                .iter()
                .filter_map(|stated| Some((comparison_key(stated).ok()?, stated.clone())))
                .collect()
        });
        let labels = self
            .label_lines()
            .into_iter()
            .map(|line| StatedLabelLine {
                names: line.names,
                catalog_numbers: line
                    .catalog_numbers
                    .into_iter()
                    .map(|number| Statement {
                        catalogs: catalog_key(&number)
                            .map(|key| numbers.naming(&key))
                            .unwrap_or_default(),
                        value: number,
                    })
                    .collect(),
            })
            .collect();
        Statements {
            years: Stated::of(self.releases.iter().map(|release| {
                (release.source, release.year.map(|y| (y, y)).into_iter().collect())
            }))
            .statements(),
            labels,
            barcodes: barcodes.statements(),
            areas: Stated::of(self.releases.iter().map(|release| {
                (release.source, release.area.map(|a| (a, a)).into_iter().collect())
            }))
            .statements(),
            media: Stated::of(self.releases.iter().map(|release| {
                let counts = release.media.counts();
                (
                    release.source,
                    if counts.is_empty() { Vec::new() } else { vec![(counts.clone(), counts)] },
                )
            }))
            .statements(),
        }
    }

    /// The note each of the row's records has, as the row lists them: the
    /// notes of the records that state one, each naming its catalog where the
    /// notes differ. `note_of` is the one note a record has to show.
    pub fn noted(
        &self,
        note_of: impl Fn(&MetadataResult) -> Option<String>,
    ) -> Vec<Statement<String>> {
        Stated::of(self.releases.iter().map(|release| {
            (
                release.source,
                note_of(release).map(|note| (note.clone(), note)).into_iter().collect(),
            )
        }))
        .statements()
    }
}

#[cfg(test)]
#[path = "statements_tests.rs"]
mod tests;
