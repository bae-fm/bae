//! Where a sleeve says the product was made.
//!
//! A sleeve's manufacturing statement — "Made in the EU", "Printed in
//! England", "Fabriqué en France" — is the one place printed text names the
//! area a pressing belongs to on purpose. Discogs transcribes these into its
//! release notes, and a sample of 400,000 releases of its 2026-09-01 data dump
//! shows which phrases sleeves print (read 2026-09-26): "made in" (85,308),
//! "printed in" (14,727), "manufactured in" (6,647), "pressed in" (432),
//! "fabriqué en" (368), "made and printed in" (237), "printed and made in"
//! (116), "fabricado en" (36), "hergestellt in" (8), "impreso en" (6). The
//! same sample shows what follows them: country names, and the abbreviations
//! "EU", "E.U.", "EEC" and "EC" for Europe as one market, "U.S.A."/"USA", "UK"
//! and "U.K.", and the everyday names "England", "Holland" and "West
//! Germany".
//!
//! Inside such a statement an abbreviation is not a word of prose, so it
//! names its area — which it does nowhere else: a two-letter code in running
//! text is far more often a word ("for all of us") than a country. The
//! abbreviations are read only as written, in capitals.
//!
//! Rights-society and label-code marks printed beside the statement ("BIEM/
//! SABAM", "LC 15025") are not read. BIEM is the international bureau of
//! mechanical-rights societies, whose members are on several continents, and
//! a society named beside it licensed the recording's rights, which says
//! nothing of where the pressing was released; a label code identifies the
//! label to collecting societies wherever it sells.

use super::agreements::words;
use crate::pressing::{Country, ReleaseArea};

/// The phrases that state where the product was made, as words.
const STATEMENTS: &[&[&str]] = &[
    &["made", "and", "printed", "in"],
    &["printed", "and", "made", "in"],
    &["made", "in"],
    &["printed", "in"],
    &["manufactured", "in"],
    &["pressed", "in"],
    &["fabrique", "en"],
    &["fabricado", "en"],
    &["hergestellt", "in"],
    &["impreso", "en"],
];

/// The abbreviations a statement writes an area as, each as its letters in
/// capitals whatever punctuation separates them ("E.U." is "EU").
const ABBREVIATIONS: &[(&str, Abbreviated)] = &[
    ("EU", Abbreviated::Europe),
    ("EEC", Abbreviated::Europe),
    ("EC", Abbreviated::Europe),
    ("USA", Abbreviated::Country("US")),
    ("US", Abbreviated::Country("US")),
    ("UK", Abbreviated::Country("GB")),
];

/// Names a statement writes a country by that the country table does not
/// hold: a part of it, or an older name for it. The catalogs file what these
/// sleeves say under the country — Discogs lists no "England", "Holland" or
/// "West Germany" among its release countries, and MusicBrainz has an East
/// Germany of its own and no West Germany — so the statement names that
/// country.
const EVERYDAY_NAMES: &[(&str, &str)] = &[
    ("england", "GB"),
    ("holland", "NL"),
    ("westgermany", "DE"),
    ("westerngermany", "DE"),
    ("wgermany", "DE"),
];

#[derive(Debug, Clone, Copy)]
enum Abbreviated {
    Europe,
    Country(&'static str),
}

/// The areas `text` says the product was made in, one per statement that
/// names an area bae knows.
pub(crate) fn stated_origins(text: &str) -> Vec<ReleaseArea> {
    let as_written = written_words(text);
    let folded: Vec<String> = as_written.iter().map(|word| word.to_lowercase()).collect();
    let mut origins = Vec::new();
    let mut at = 0;
    while at < folded.len() {
        let Some(statement) = STATEMENTS.iter().find(|phrase| {
            phrase.len() <= folded.len() - at
                && phrase
                    .iter()
                    .zip(&folded[at..])
                    .all(|(want, word)| want == word)
        }) else {
            at += 1;
            continue;
        };
        let mut place = at + statement.len();
        if folded.get(place).is_some_and(|word| word == "the") {
            place += 1;
        }
        if let Some((area, used)) = place_at(&as_written[place..], &folded[place..]) {
            origins.push(area);
            at = place + used;
        } else {
            at = place;
        }
    }
    origins
}

/// The area the words from here on name, and how many of them it took: the
/// longest run of up to four words that names one.
fn place_at(as_written: &[String], folded: &[String]) -> Option<(ReleaseArea, usize)> {
    (1..=folded.len().min(4)).rev().find_map(|count| {
        let written: String = as_written[..count].concat();
        let squashed: String = folded[..count].concat();
        named(&squashed)
            .or_else(|| abbreviated(&written))
            .map(|area| (area, count))
    })
}

/// The area a name — any case, run together — names.
fn named(squashed: &str) -> Option<ReleaseArea> {
    let same = |name: &str| words(name).concat() == squashed;
    Country::all()
        .find(|country| country.names().iter().any(|name| same(name)))
        .map(ReleaseArea::Country)
        .or_else(|| {
            EVERYDAY_NAMES
                .iter()
                .find(|(name, _)| *name == squashed)
                .and_then(|(_, code)| Country::from_code(code))
                .map(ReleaseArea::Country)
        })
        .or_else(|| {
            crate::pressing::Region::written_as(|name| words(name).concat() == squashed)
                .map(ReleaseArea::Region)
        })
}

/// The area an abbreviation written in capitals names.
fn abbreviated(written: &str) -> Option<ReleaseArea> {
    if written.chars().any(|c| !c.is_uppercase()) {
        return None;
    }
    ABBREVIATIONS
        .iter()
        .find(|(letters, _)| *letters == written)
        .map(|(_, area)| match area {
            Abbreviated::Europe => ReleaseArea::Region(crate::pressing::Region::Europe),
            Abbreviated::Country(code) => ReleaseArea::Country(
                Country::from_code(code).expect("the abbreviations name assigned codes"),
            ),
        })
}

/// The line's words as written: the same runs of letters and digits
/// [`words`] folds, with their case kept and diacritics dropped.
fn written_words(text: &str) -> Vec<String> {
    use unicode_normalization::UnicodeNormalization;
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pressing::Region;

    fn country(code: &str) -> ReleaseArea {
        ReleaseArea::Country(Country::from_code(code).unwrap())
    }

    /// The statements sleeves print, however they write the area.
    #[test]
    fn a_statement_names_where_the_product_was_made() {
        let europe = ReleaseArea::Region(Region::Europe);
        for (text, area) in [
            (
                "Unauthorised copying prohibited. Made in the EU. LC 00000.",
                europe,
            ),
            ("Made in the E.U.", europe),
            ("Manufactured in EEC", europe),
            ("Made in U.S.A.", country("US")),
            ("Printed in England", country("GB")),
            ("Made and Printed in Holland", country("NL")),
            ("Made in W. Germany by Placeholder Pressing", country("DE")),
            ("Fabriqué en France", country("FR")),
            ("Made in Japan", country("JP")),
            ("Pressed in Europe", europe),
        ] {
            assert_eq!(stated_origins(text), vec![area], "{text}");
        }
    }

    /// An abbreviation written as a word is a word, statement or not, and
    /// text with no statement names nowhere — an address included.
    #[test]
    fn prose_and_addresses_name_nowhere() {
        for text in [
            "the music was made in us all",
            "the song was made in it",
            "Placeholder Records, 100 Placeholder Drive, Beverly Hills, CA 90210",
            "BIEM/SABAM",
            "Made in",
        ] {
            assert!(stated_origins(text).is_empty(), "{text}");
        }
    }
}
