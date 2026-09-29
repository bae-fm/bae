//! Which of a result's fields the candidate's own text states. The disc ID and
//! the barcode come from the lookups that returned the result, not the text.

use super::combine::LookupProvenance;
use crate::import::search::MetadataResult;
use crate::pressing::{ReleaseArea, ReleaseLabel};
use crate::signals::TextLine;
use crate::text_match::{bare_album_title, is_stop_word, squash, words, written_words, LabelName};
use std::collections::HashSet;

/// Which of one result's fields the folder confirms. A field the result does
/// not state is no agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Agreements {
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
    /// The album's title and artist, which name the album rather than the
    /// pressing: they rank rows (see [`Self::names_album`]) but are not
    /// among [`Self::count`] or [`Self::offered`].
    pub title: bool,
    pub artist: bool,
}

impl Agreements {
    /// Nothing agrees, as for a typed search.
    pub const NONE: Self = Self {
        disc_id: false,
        barcode: false,
        catalog: false,
        label: false,
        year: false,
        country: false,
        title: false,
        artist: false,
    };

    /// Both together: a pressing row is one object, so what any of its records
    /// agrees with is true of it.
    pub fn with(self, other: Self) -> Self {
        Self {
            disc_id: self.disc_id || other.disc_id,
            barcode: self.barcode || other.barcode,
            catalog: self.catalog || other.catalog,
            label: self.label || other.label,
            year: self.year || other.year,
            country: self.country || other.country,
            title: self.title || other.title,
            artist: self.artist || other.artist,
        }
    }

    /// How many of the pressing's fields agree, which orders the records in a
    /// row, the rows in a card, and the cards.
    pub fn count(&self) -> u32 {
        [
            self.disc_id,
            self.barcode,
            self.catalog,
            self.label,
            self.year,
            self.country,
        ]
        .into_iter()
        .filter(|agreed| *agreed)
        .count() as u32
    }

    /// How many of the album's title and artist agree.
    pub fn names_album(&self) -> u32 {
        u32::from(self.title) + u32::from(self.artist)
    }

    /// Whether anything but the barcode and the year agrees, which is what
    /// shows a release on the list rather than under "N more": a barcode read
    /// off a photo can be misread into some other release, and a folder's
    /// year is as often the album's as its edition's, which only the album's
    /// first year tells apart — the ranking weighs the year against that.
    pub fn offered(&self) -> bool {
        self.disc_id || self.catalog || self.label || self.country
    }
}

/// What the candidate's text agrees with about `result`, given the lookups
/// that returned it.
pub fn agreements_of(
    result: &MetadataResult,
    text: &CandidateText,
    lookup: &LookupProvenance,
) -> Agreements {
    Agreements {
        disc_id: lookup.by_disc_id,
        barcode: lookup.by_barcode,
        catalog: result
            .labels
            .iter()
            .filter_map(ReleaseLabel::catalog_number)
            .any(|value| text.states_catalog(value)),
        label: result
            .labels
            .iter()
            .filter_map(ReleaseLabel::name)
            .any(|value| text.states_label(value)),
        year: result
            .year
            .is_some_and(|year| text.states(&year.to_string())),
        country: result.area.is_some_and(|area| text.states_area(area)),
        title: text.states_title(&result.title),
        artist: result
            .artist
            .as_deref()
            .is_some_and(|artist| text.states(artist)),
    }
}

/// Each match paired with its agreements, for
/// [`crate::import::release_group::group_results`]. `provenance` is
/// index-aligned with `matches`.
pub fn judged_results(
    matches: Vec<MetadataResult>,
    provenance: &[LookupProvenance],
    text: &CandidateText,
) -> Vec<crate::import::release_group::Judged> {
    matches
        .into_iter()
        .zip(provenance)
        .map(|(result, lookup)| {
            let agreements = agreements_of(&result, text, lookup);
            (result, agreements)
        })
        .collect()
}

/// The candidate's own text, normalized once, and the catalog numbers the
/// person struck out. A value is stated when, lowercased and stripped of all
/// but letters and digits, it spans whole words of a line: `16033-2` states
/// `16033 2`, and "blues" does not state `US`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateText {
    lines: Vec<NormalizedLine>,
    /// Normalized, so the comparison is the one `states` makes.
    struck_out: HashSet<String>,
}

impl CandidateText {
    /// The pooled lines and the struck-out catalog numbers.
    pub fn of(pool: &[TextLine], struck_out: &[String]) -> Self {
        Self {
            lines: pool
                .iter()
                .filter_map(|line| NormalizedLine::of(&line.text))
                .collect(),
            struck_out: struck_out
                .iter()
                .map(|value| squash(value))
                .filter(|value| !value.is_empty())
                .collect(),
        }
    }

    /// Whether the text states `value` — whole words of one of its lines.
    pub fn states(&self, value: &str) -> bool {
        self.states_run(&squash(value))
    }

    /// Whether the text states `title` as an album's title: every word of it
    /// in one of its lines, in any order. A catalog writes an album's words
    /// in the order its sleeve does, which one edition's sleeve may not —
    /// "Album 1999" and "1999 Album" are one album. The bracketed tails the
    /// title ends on name an edition, not the album, and are not asked for:
    /// see `text_match::bare_album_title`.
    pub fn states_title(&self, title: &str) -> bool {
        let words = words(&bare_album_title(title));
        !words.is_empty() && self.lines.iter().any(|line| line.holds_words(&words))
    }

    /// Whether the text states an already-normalized value.
    fn states_run(&self, run: &str) -> bool {
        !run.is_empty() && self.lines.iter().any(|line| line.states(run))
    }

    /// Whether the text states `value` as a catalog number: printed there,
    /// and not struck out.
    pub fn states_catalog(&self, value: &str) -> bool {
        !self.is_struck_out(value) && self.states(value)
    }

    /// Whether the text states `value` as a label name, without the trade
    /// word it may trail: "Warner Bros." states "Warner Bros. Records". A
    /// label written as its initials states the name they are the initials
    /// of, either way round: "DFC" and "Dance Floor Corporation".
    pub fn states_label(&self, value: &str) -> bool {
        let Some(name) = LabelName::of(value) else {
            return false;
        };
        self.states_run(name.stated())
            || name.initials().is_some_and(|initials| {
                let written = initials.to_uppercase();
                self.lines.iter().any(|line| line.writes_code(&written))
            })
            || name.written_initials().is_some_and(|initials| {
                self.lines.iter().any(|line| line.spells_initials(initials))
            })
    }

    /// Every year the text writes as a word of its own, each once.
    pub fn years(&self) -> Vec<i32> {
        let mut years: Vec<i32> = Vec::new();
        for line in &self.lines {
            for (start, end) in line.starts.iter().zip(&line.ends) {
                if let Some(year) = super::row_facts::year_of(&line.run[*start..*end]) {
                    if !years.contains(&year) {
                        years.push(year);
                    }
                }
            }
        }
        years
    }

    /// Whether the text writes `area` any way it is written — see
    /// `ReleaseArea::names`.
    pub fn states_area(&self, area: ReleaseArea) -> bool {
        area.names().iter().any(|name| self.states(name))
            || area
                .codes()
                .any(|code| self.lines.iter().any(|line| line.writes_code(code)))
    }

    /// Whether the person struck `value` out as a catalog number.
    pub fn is_struck_out(&self, value: &str) -> bool {
        self.struck_out.contains(&squash(value))
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// One line: its words run together, where each begins and ends, and the
/// words it writes in capitals.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedLine {
    run: String,
    starts: Vec<usize>,
    ends: Vec<usize>,
    /// Dots between letters dropped: "E.U." is `EU`.
    capitals: Vec<String>,
}

impl NormalizedLine {
    fn of(text: &str) -> Option<Self> {
        let mut run = String::new();
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for word in words(text) {
            starts.push(run.len());
            run.push_str(&word);
            ends.push(run.len());
        }
        let capitals = written_words(text)
            .into_iter()
            .filter(|word| word.chars().all(char::is_uppercase))
            .collect();
        (!run.is_empty()).then_some(Self {
            run,
            starts,
            ends,
            capitals,
        })
    }

    /// Whether this line writes `code` as a whole word in capitals.
    fn writes_code(&self, code: &str) -> bool {
        self.capitals.iter().any(|word| word == code)
    }

    /// Whether a run of this line's words has `initials` as the first letters
    /// of its words, its stop words left out.
    fn spells_initials(&self, initials: &str) -> bool {
        let words: Vec<&str> = self
            .starts
            .iter()
            .zip(&self.ends)
            .map(|(&start, &end)| &self.run[start..end])
            .collect();
        (0..words.len()).any(|start| {
            if is_stop_word(words[start]) {
                return false;
            }
            let mut spelled = String::new();
            for word in &words[start..] {
                if is_stop_word(word) {
                    continue;
                }
                spelled.extend(word.chars().next());
                if !initials.starts_with(spelled.as_str()) {
                    return false;
                }
                if spelled == initials {
                    return true;
                }
            }
            false
        })
    }

    /// Whether every one of `words` — each squashed — is a word of this line.
    fn holds_words(&self, words: &[String]) -> bool {
        words.iter().all(|word| {
            self.starts
                .iter()
                .zip(&self.ends)
                .any(|(&start, &end)| &self.run[start..end] == word)
        })
    }

    /// Whether `value` — already squashed — spans whole words of this line.
    fn states(&self, value: &str) -> bool {
        self.run.match_indices(value).any(|(at, _)| {
            self.starts.binary_search(&at).is_ok()
                && self.ends.binary_search(&(at + value.len())).is_ok()
        })
    }
}

#[cfg(test)]
#[path = "agreements_tests.rs"]
mod tests;
