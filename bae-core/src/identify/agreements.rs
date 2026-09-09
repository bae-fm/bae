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

use super::combine::LookupProvenance;
use crate::import::search::MetadataResult;
use crate::signals::TextLine;
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
    let states = |field: &Option<String>| field.as_deref().is_some_and(|value| text.states(value));
    Agreements {
        disc_id: lookup.by_disc_id,
        barcode: lookup.by_barcode,
        catalog: lookup.by_catalog || states(&result.catalog_number),
        label: states(&result.label),
        year: result
            .year
            .is_some_and(|year| text.states(&year.to_string())),
        country: states(&result.country),
    }
}

/// The candidate's own text, normalized once so a result's fields can be
/// looked up in it.
///
/// A line is held as its words run together, with where each word begins and
/// ends. A value is normalized the same way — folded to lowercase, diacritics
/// dropped, and everything that is not a letter or a digit removed — and it is
/// stated by the text when it spans whole words of a line. That is what lets
/// `16033-2` in a folder name state a catalog number written `16033 2`, while
/// keeping a country of `US` out of "blues" and a catalog of `531 2` out of a
/// barcode's digits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateText {
    lines: Vec<NormalizedLine>,
}

impl CandidateText {
    /// The candidate's pooled lines, normalized for lookup. Lines that carry
    /// no letter or digit state nothing and are left out.
    pub fn of(pool: &[TextLine]) -> Self {
        Self {
            lines: pool
                .iter()
                .filter_map(|line| NormalizedLine::of(&line.text))
                .collect(),
        }
    }

    /// Whether the text states `value` — whole words of one of its lines.
    pub fn states(&self, value: &str) -> bool {
        let value = squash(value);
        if value.is_empty() {
            return false;
        }
        self.lines.iter().any(|line| line.states(&value))
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
fn squash(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// The line's words, each squashed. A word is a run of letters and digits.
fn words(text: &str) -> Vec<String> {
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
