//! What one fact about a row says against what the folder says: it agrees,
//! the row states nothing about it, or it states something different. A row
//! that states nothing never loses to one that disagrees, and one that agrees
//! outranks both.

use super::agreements::CandidateText;
use crate::import::search::MetadataResult;
use crate::pressing::{Country, ReleaseArea};

/// What a row's fact says against the folder's, worst first, so the derived
/// order ranks rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Fact {
    /// The row states something the folder contradicts.
    Disagrees,
    /// The row, or the folder, states nothing about it.
    StatesNothing,
    Agrees,
}

/// What the folder's own text states that rows are weighed against, and the
/// year each album on the list first came out, read once for the run.
pub(crate) struct FolderFacts {
    /// Every year its text writes as a word of its own.
    years: Vec<i32>,
    /// The year each album first came out, by its catalog and group, as any
    /// record of it whose full document was read states it: every pressing of
    /// one album shares it, read or not.
    album_years: Vec<((crate::import::Catalog, String), i32)>,
    /// Every country its text writes out by name. Codes are left out: a
    /// two-letter capital word is as often something else ("CD" is the
    /// Congo's), and a country the folder does not name contradicts nothing.
    countries: Vec<Country>,
}

impl FolderFacts {
    pub(crate) fn of<'a>(
        text: &CandidateText,
        results: impl IntoIterator<Item = &'a MetadataResult>,
    ) -> Self {
        let mut album_years: Vec<((crate::import::Catalog, String), i32)> = Vec::new();
        for result in results {
            if let (Some(group), Some(year)) = (&result.source_group_id, result.album_first_year) {
                let album = (result.source, group.clone());
                if !album_years.iter().any(|(known, _)| *known == album) {
                    album_years.push((album, year));
                }
            }
        }
        Self {
            years: text.years(),
            album_years,
            countries: Country::all()
                .filter(|country| country.names().iter().any(|name| text.states(name)))
                .collect(),
        }
    }

    /// Whether the folder's years include the year the row's album first came
    /// out: it confirms the album, as its title and artist do.
    pub(crate) fn names_the_album_year(&self, records: &[MetadataResult]) -> bool {
        self.album_first_year(records)
            .is_some_and(|first| self.years.contains(&first))
    }

    /// The year the row's album first came out, as a record of it states or
    /// another pressing of the album on the list does.
    pub(crate) fn album_first_year(&self, records: &[MetadataResult]) -> Option<i32> {
        records.iter().find_map(|record| {
            record.album_first_year.or_else(|| {
                let group = record.source_group_id.as_ref()?;
                self.album_years
                    .iter()
                    .find(|((catalog, key), _)| *catalog == record.source && key == group)
                    .map(|(_, year)| *year)
            })
        })
    }

    /// Whether the row was released in the year the folder names its edition
    /// by.
    ///
    /// A folder year later than the album's first year names an edition — the
    /// latest of them, where it writes two ("1963 … 2005 remaster"). A row
    /// stating that year agrees, one stating another disagrees, and an undated
    /// row states nothing. A year its own title writes ("1990-2000") is the
    /// title's, not an edition's. Where the row's album year is not known, no
    /// year here is known to name an edition, and the row states nothing — its
    /// year counts as before, toward whether it is offered.
    pub(crate) fn edition_year(&self, records: &[MetadataResult]) -> Fact {
        let stated: Vec<i32> = records.iter().filter_map(|record| record.year).collect();
        let Some(first) = self.album_first_year(records) else {
            return Fact::StatesNothing;
        };
        let titled: Vec<i32> = records
            .iter()
            .flat_map(|record| title_years(&record.title))
            .collect();
        let edition = self
            .years
            .iter()
            .copied()
            .filter(|year| *year > first && !titled.contains(year))
            .max();
        match edition {
            None => Fact::StatesNothing,
            Some(_) if stated.is_empty() => Fact::StatesNothing,
            Some(edition) if stated.contains(&edition) => Fact::Agrees,
            Some(_) => Fact::Disagrees,
        }
    }

    /// Whether the row was released where the folder says: in a country or
    /// region the text writes, or — where the folder names a country and the
    /// row states another country — somewhere else. A region the row states
    /// and the folder does not write says nothing: which countries a region
    /// spans is not known here.
    pub(crate) fn country(&self, records: &[MetadataResult], text: &CandidateText) -> Fact {
        let areas: Vec<ReleaseArea> = records.iter().filter_map(|record| record.area).collect();
        if areas.iter().any(|area| text.states_area(*area)) {
            return Fact::Agrees;
        }
        let states_a_country = areas
            .iter()
            .any(|area| matches!(area, ReleaseArea::Country(_)));
        if states_a_country && !self.countries.is_empty() {
            Fact::Disagrees
        } else {
            Fact::StatesNothing
        }
    }
}

/// Whether the row was released where the folder's recordings were registered,
/// as their ISRCs say: the same area agrees, another country disagrees, and a
/// region, or no area, states nothing.
pub(crate) fn registration(records: &[MetadataResult], registered: Option<ReleaseArea>) -> Fact {
    let Some(registered) = registered else {
        return Fact::StatesNothing;
    };
    let areas: Vec<ReleaseArea> = records.iter().filter_map(|record| record.area).collect();
    if areas.contains(&registered) {
        Fact::Agrees
    } else if matches!(registered, ReleaseArea::Country(_))
        && areas
            .iter()
            .any(|area| matches!(area, ReleaseArea::Country(_)))
    {
        Fact::Disagrees
    } else {
        Fact::StatesNothing
    }
}

/// The years a title writes as words of their own.
fn title_years(title: &str) -> Vec<i32> {
    super::agreements::words(title)
        .iter()
        .filter_map(|word| year_of(word))
        .collect()
}

/// `word` as a year, when it is one: four digits, from 1900 to 2099.
pub(crate) fn year_of(word: &str) -> Option<i32> {
    (word.len() == 4 && word.chars().all(|c| c.is_ascii_digit()))
        .then(|| word.parse::<i32>().ok())
        .flatten()
        .filter(|year| (1900..=2099).contains(year))
}

#[cfg(test)]
#[path = "row_facts_tests.rs"]
mod tests;
