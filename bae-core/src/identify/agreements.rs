//! How much of a candidate's own text agrees with one result.
//!
//! Nothing is extracted from the folder to judge a result with. The result
//! arrives with its own fields — a catalog number, a label, a year, a country
//! — and each of them is looked for in the text the folder carries: its name,
//! its file names, its CUE fields, its `.txt` contents, and the OCR of its
//! artwork. Every field the text states is one agreement, and the agreements
//! are what order the rows and what each row's badges say.
//!
//! The disc ID and the barcode are not looked for: they are exact codes, and
//! the lookup that returned the result is what states them.
//!
//! A country is one field the two write differently: a provider answers `JP`
//! and a folder writes `Japan`. Both spellings are looked for, through
//! [`super::country`]. A label is the other: a folder writes "Warner Bros."
//! and a source writes "Warner Bros. Records", so the trade word a label's
//! name trails is dropped from it first, through [`super::label`].
//!
//! Not every number printed on a folder is a catalog number — a phone number
//! on a sleeve, a serial on a label, the year twice — so a person can strike
//! one out. A struck-out value is not read as a catalog number any more,
//! however plainly the text prints it; it is still text, and still states
//! whatever else it happens to be.

use super::combine::LookupProvenance;
use crate::import::search::MetadataResult;
use crate::signals::TextLine;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

/// What the candidate's own text agrees with about one result — one badge per
/// field, and the count is what orders the rows.
///
/// `disc_id` and `barcode` are the lookups that returned it. `catalog` is
/// either: the catalog lookup returned it, or its catalog number is printed in
/// the folder's text. `label`, `year` and `country` are the text alone. A
/// field the result does not state is no agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Agreements {
    pub disc_id: bool,
    pub barcode: bool,
    pub catalog: bool,
    pub label: bool,
    pub year: bool,
    pub country: bool,
}

impl Agreements {
    /// Nothing agrees — what a result carries where there is nothing to rank
    /// by, as in a typed search.
    pub const NONE: Self = Self {
        disc_id: false,
        barcode: false,
        catalog: false,
        label: false,
        year: false,
        country: false,
    };

    /// Both together — what a pressing row agrees with, since a row is one
    /// physical object however many sources carry it and is picked whole. A
    /// catalog number only Discogs prints, and a disc ID only MusicBrainz
    /// answers, are both true of the object.
    pub fn with(self, other: Self) -> Self {
        Self {
            disc_id: self.disc_id || other.disc_id,
            barcode: self.barcode || other.barcode,
            catalog: self.catalog || other.catalog,
            label: self.label || other.label,
            year: self.year || other.year,
            country: self.country || other.country,
        }
    }

    /// How many badges the row carries. What the rows are ordered by.
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

    /// Whether this release belongs on the list rather than under "N more".
    ///
    /// Every agreement but the barcode says something about *which* release
    /// this is: the disc ID is computed from the audio itself, and the catalog
    /// number, label, year and country are printed in the folder's own text. A
    /// barcode is read off a photograph of a sleeve, so a barcode lookup that
    /// comes back naming a release the folder says nothing else about has read
    /// the wrong digits — a real answer to the wrong question, which is what
    /// "N more" is for.
    pub fn offered(&self) -> bool {
        self.disc_id || self.catalog || self.label || self.year || self.country
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
        catalog: lookup.by_catalog
            || result
                .catalog_number
                .as_deref()
                .is_some_and(|value| text.states_catalog(value)),
        label: result
            .label
            .as_deref()
            .is_some_and(|value| text.states_label(value)),
        year: result
            .year
            .is_some_and(|year| text.states(&year.to_string())),
        country: result
            .country
            .as_deref()
            .is_some_and(|value| text.states_country(value)),
    }
}

/// What the text agrees with about each of a verdict's matches, paired with
/// the matches themselves — what [`crate::import::release_group::group_results`]
/// ranks the rows and the records within a row by.
///
/// `provenance` is index-aligned with `matches`, as a verdict stores the two.
/// One definition, because the pane and the sweep's settle step both ask it:
/// the record a person sees leading a row has to be the record whose document
/// fills the draft.
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

/// The candidate's own text as ranking reads it: its lines, normalized once
/// so a result's fields can be looked up in them, and the catalog numbers the
/// person has struck out of them.
///
/// A line is held as its words run together, with where each word begins and
/// ends. A value is normalized the same way — folded to lowercase, diacritics
/// dropped, and everything that is not a letter or a digit removed — and it is
/// stated by the text when it spans whole words of a line. That is what lets
/// `16033-2` in a folder name state a catalog number written `16033 2`, while
/// keeping a country of `US` out of "blues" and a catalog of `531 2` out of a
/// barcode's digits. A struck-out number is normalized the same way, so it is
/// struck out however either of them is punctuated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateText {
    lines: Vec<NormalizedLine>,
    /// Normalized, so the comparison is the one `states` makes.
    struck_out: HashSet<String>,
}

impl CandidateText {
    /// The candidate's pooled lines, normalized for lookup, with the catalog
    /// numbers the person struck out of them. Lines that carry no letter or
    /// digit state nothing and are left out.
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

    /// Whether the text states an already-normalized value.
    fn states_run(&self, run: &str) -> bool {
        !run.is_empty() && self.lines.iter().any(|line| line.states(run))
    }

    /// Whether the text states `value` as a catalog number: printed there,
    /// and not struck out.
    pub fn states_catalog(&self, value: &str) -> bool {
        !self.is_struck_out(value) && self.states(value)
    }

    /// Whether the text states `value` as a label name, whichever of them
    /// trails it with a trade word: a folder saying "Warner Bros." states a
    /// result's "Warner Bros. Records", and one saying "Atlantic Records"
    /// states an "Atlantic". A name that is nothing but trade words names no
    /// label and is stated by nothing.
    ///
    /// Only the name is stripped. A line keeps its own trade words, because
    /// the question asked of it is whether the name spans whole words of it,
    /// and a word the name never reaches cannot answer that either way.
    pub fn states_label(&self, value: &str) -> bool {
        super::label::stated(value).is_some_and(|name| self.states_run(&name))
    }

    /// Whether the text states `value` as a country, however either of them
    /// writes it. A provider answers a code and a folder writes the name out,
    /// so a result saying `JP` is stated by a folder saying `Japan`, and one
    /// saying `Japan` by a folder saying `JP`.
    pub fn states_country(&self, value: &str) -> bool {
        if self.states(value) {
            return true;
        }
        let Some(country) = super::country::named(value) else {
            return false;
        };
        self.states(country.code) || country.names.iter().any(|name| self.states(name))
    }

    /// Whether the person struck `value` out as a catalog number.
    pub fn is_struck_out(&self, value: &str) -> bool {
        self.struck_out.contains(&squash(value))
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// One line as it is looked up in: its words run together, and the byte
/// offsets where words begin and end.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedLine {
    run: String,
    /// Ascending, one per word.
    starts: Vec<usize>,
    /// Ascending, one per word.
    ends: Vec<usize>,
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
        (!run.is_empty()).then_some(Self { run, starts, ends })
    }

    /// Whether `value` — already squashed — spans whole words of this line.
    fn states(&self, value: &str) -> bool {
        self.run.match_indices(value).any(|(at, _)| {
            self.starts.binary_search(&at).is_ok()
                && self.ends.binary_search(&(at + value.len())).is_ok()
        })
    }
}

/// A value as it is looked up: lowercase, diacritics folded away, and
/// everything that is not a letter or a digit dropped — which is what makes
/// `WPCR-80001`, `WPCR 80001` and `wpcr80001` one value.
pub(super) fn squash(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// The line's words, each squashed. A word is a run of letters and digits.
pub(super) fn words(text: &str) -> Vec<String> {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "agreements_tests.rs"]
mod tests;
