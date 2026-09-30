//! What one fact about a row says against what the folder says: it agrees,
//! the row states nothing about it, or it states something different. A row
//! that states nothing never loses to one that disagrees, and one that agrees
//! outranks both.

use super::agreements::CandidateText;
use crate::import::search::MetadataResult;
use crate::pressing::{Country, ReleaseArea};
use crate::text_match::{squash, track_title_key};

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
    /// Every year its highest-standing text writing any writes as a word of
    /// its own: a sleeve prints copyright years of other editions as readily
    /// as its own.
    years: Vec<i32>,
    /// The year each album first came out, by its catalog and group, as any
    /// record of it whose full document was read states it: every pressing of
    /// one album shares it, read or not.
    album_years: Vec<((crate::import::Catalog, String), i32)>,
    /// Where the folder says the copy was released: the one area its
    /// highest-standing text naming any names (see
    /// [`CandidateText::highest_stating`]). None where that text names two:
    /// one copy was released in one place, so two rows of different countries
    /// never both agree. An area is named by a country's name, or by any way
    /// of writing an area a record on the list states — codes count only
    /// there, since a two-letter capital word is as often something else
    /// ("CD" is the Congo's). A country no record states still counts: it is
    /// a fact about the copy, and every row of another country disagrees.
    area: Option<ReleaseArea>,
    /// The catalog numbers the folder states, squashed: those of the records
    /// on the list its highest-standing text stating any of them states. A
    /// label, a matrix or a second label can each print a number, so a copy
    /// may state several, where it has one country. Only a list's numbers are
    /// looked for — a number no record carries, or a short form of one,
    /// states nothing and leaves the next standing to speak.
    catalogs: Vec<String>,
}

impl FolderFacts {
    pub(crate) fn of<'a>(
        text: &CandidateText,
        results: impl IntoIterator<Item = &'a MetadataResult>,
    ) -> Self {
        let results: Vec<&MetadataResult> = results.into_iter().collect();
        let listed: Vec<ReleaseArea> = results.iter().filter_map(|result| result.area).collect();
        let named = text.highest_stating(|lines| {
            let mut named: Vec<ReleaseArea> = Country::all()
                .filter(|country| country.names().iter().any(|name| lines.states(name)))
                .map(ReleaseArea::Country)
                .collect();
            for area in &listed {
                if !named.contains(area) && lines.states_area(*area) {
                    named.push(*area);
                }
            }
            named
        });
        let numbers: Vec<&str> = results
            .iter()
            .flat_map(|result| &result.labels)
            .filter_map(crate::pressing::ReleaseLabel::catalog_number)
            .collect();
        let catalogs = text.highest_stating(|lines| {
            let mut stated: Vec<String> = Vec::new();
            for number in &numbers {
                let key = squash(number);
                if !stated.contains(&key) && lines.states_catalog(number) {
                    stated.push(key);
                }
            }
            stated
        });
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
            years: text.highest_stating(CandidateText::years),
            album_years,
            area: match named.as_slice() {
                [one] => Some(*one),
                _ => None,
            },
            catalogs,
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

    /// The year the folder names the row's pressing by, or none: one answer,
    /// which the Year badge, whether a row is offered, and the ranking all
    /// read.
    ///
    /// A year the row's own title writes ("1990-2000") is the title's. Of the
    /// rest, one no later than the album's first year names the album, and
    /// the latest later one names the pressing. Where the album's first year
    /// is not known, only a folder writing two years ("1963 … 2005
    /// remaster") names a pressing, by the later: most folders are named with
    /// the year the album first came out, so a lone year cannot be read as the
    /// pressing's.
    pub(crate) fn pressing_year(&self, records: &[MetadataResult]) -> Option<i32> {
        let titled: Vec<i32> = records
            .iter()
            .flat_map(|record| title_years(&record.title))
            .collect();
        let years: Vec<i32> = self
            .years
            .iter()
            .copied()
            .filter(|year| !titled.contains(year))
            .collect();
        match self.album_first_year(records) {
            Some(first) => years.into_iter().filter(|year| *year > first).max(),
            None if years.len() >= 2 => years.into_iter().max(),
            None => None,
        }
    }

    /// Whether the row was released in the year the folder names its pressing
    /// by — see [`Self::pressing_year`]. A row stating that year agrees, one
    /// stating another disagrees, and an undated row, or a folder naming no
    /// pressing year, states nothing.
    pub(crate) fn edition_year(&self, records: &[MetadataResult]) -> Fact {
        let Some(year) = self.pressing_year(records) else {
            return Fact::StatesNothing;
        };
        let stated: Vec<i32> = records.iter().filter_map(|record| record.year).collect();
        if stated.is_empty() {
            Fact::StatesNothing
        } else if stated.contains(&year) {
            Fact::Agrees
        } else {
            Fact::Disagrees
        }
    }

    /// Whether the folder states `number` as its catalog number.
    pub(crate) fn states_catalog(&self, number: &str) -> bool {
        self.catalogs.contains(&squash(number))
    }

    /// Whether `area` is where the folder says the copy was released.
    pub(crate) fn names_area(&self, area: ReleaseArea) -> bool {
        self.area == Some(area)
    }

    /// Whether the row was released where the folder says: there, or — where
    /// the folder names a country and the row states another country —
    /// somewhere else. A region the row states and the folder does not name
    /// says nothing: which countries a region spans is not known here.
    pub(crate) fn country(&self, records: &[MetadataResult]) -> Fact {
        let Some(folder) = self.area else {
            return Fact::StatesNothing;
        };
        let areas: Vec<ReleaseArea> = records.iter().filter_map(|record| record.area).collect();
        if areas.contains(&folder) {
            Fact::Agrees
        } else if matches!(folder, ReleaseArea::Country(_))
            && areas
                .iter()
                .any(|area| matches!(area, ReleaseArea::Country(_)))
        {
            Fact::Disagrees
        } else {
            Fact::StatesNothing
        }
    }
}

/// The year the pressing `records` name came out, for a draft read from them:
/// the first year one of them states — the records of one row are one
/// pressing, so another catalog's record of it speaks for the one the draft
/// is read from — or, where none states one, the year the folder names the
/// pressing by (see [`FolderFacts::pressing_year`]).
pub(crate) fn pressing_year(text: &CandidateText, records: &[MetadataResult]) -> Option<i32> {
    records
        .iter()
        .find_map(|record| record.year)
        .or_else(|| folder_pressing_year(text, records))
}

/// The year the folder names the pressing `records` are of by — see
/// [`FolderFacts::pressing_year`].
pub(crate) fn folder_pressing_year(
    text: &CandidateText,
    records: &[MetadataResult],
) -> Option<i32> {
    FolderFacts::of(text, records).pressing_year(records)
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

/// Whether the titles a row's read document lists run in the order the
/// folder's tracks do.
///
/// It agrees when every track's title matches the document's at its position,
/// compared as [`title_spellings`] spells them. It disagrees only when the
/// document lists the same titles as the folder, every one, at other
/// positions: a spelling, a language or a title missing on either side leaves
/// the lists apart for reasons that say nothing about the edition, so it
/// states nothing — as does a row whose document was not read. Of a row's
/// records, one that agrees decides it.
pub(crate) fn track_titles(records: &[MetadataResult], folder: &[String]) -> Fact {
    let folder: Vec<Vec<String>> = folder.iter().map(|title| title_spellings(title)).collect();
    if folder.is_empty() || folder.iter().any(Vec::is_empty) {
        return Fact::StatesNothing;
    }
    let mut fact = Fact::StatesNothing;
    for record in records {
        let listed: Vec<String> = record
            .track_titles
            .iter()
            .map(|title| track_title_key(title))
            .collect();
        if listed.len() != folder.len() || listed.iter().any(String::is_empty) {
            continue;
        }
        if listed
            .iter()
            .zip(&folder)
            .all(|(listed, spellings)| spellings.contains(listed))
        {
            return Fact::Agrees;
        }
        let mut stated: Vec<&String> = folder.iter().map(|spellings| &spellings[0]).collect();
        let mut listed: Vec<&String> = listed.iter().collect();
        stated.sort();
        listed.sort();
        if stated == listed {
            fact = Fact::Disagrees;
        }
    }
    fact
}

/// The ways a track title the folder gives is compared: whole, and — for a
/// file named "Artist - Title" — after its first " - ". Empty when nothing
/// of the title is left to compare.
fn title_spellings(title: &str) -> Vec<String> {
    let whole = track_title_key(title);
    if whole.is_empty() {
        return Vec::new();
    }
    let mut spellings = vec![whole];
    if let Some((_, rest)) = title.split_once(" - ") {
        let rest = track_title_key(rest);
        if !rest.is_empty() && !spellings.contains(&rest) {
            spellings.push(rest);
        }
    }
    spellings
}

/// The years a title writes as words of their own.
fn title_years(title: &str) -> Vec<i32> {
    crate::text_match::words(title)
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
