use super::*;
use crate::text_match::squash;
use std::collections::HashMap;

/// Every spelling reaches exactly one country: a code or a name that two
/// rows share would silently answer for whichever came last.
#[test]
fn no_spelling_names_two_countries() {
    let mut seen: HashMap<String, &str> = HashMap::new();
    for country in COUNTRIES {
        for spelling in std::iter::once(country.code)
            .chain(country.names.iter().copied())
            .chain(country.abbreviations.iter().copied())
        {
            let key = squash(spelling);
            assert!(!key.is_empty(), "{spelling} squashes to nothing");
            if let Some(other) = seen.insert(key, country.code) {
                panic!("{spelling} names both {other} and {}", country.code);
            }
        }
    }
}

#[test]
fn every_code_is_two_letters_in_order() {
    let codes: Vec<&str> = COUNTRIES.iter().map(|country| country.code).collect();
    assert!(codes
        .iter()
        .all(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_uppercase())));
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    assert_eq!(codes, sorted, "the table is in code order");
}

/// The table's codes are the standard's officially assigned ones, code
/// for code.
#[test]
fn the_codes_are_the_standards_own() {
    assert_eq!(
        COUNTRIES.iter().map(|row| row.code).collect::<Vec<_>>(),
        crate::pressing::recorded(include_str!(
            "../../test-fixtures/pressing-vocabulary/iso-3166-1-alpha-2.txt"
        ))
    );
}

/// Abbreviations are matched in capitals, so they are written in them.
#[test]
fn every_abbreviation_is_capital_letters() {
    for country in COUNTRIES {
        for abbreviation in country.abbreviations {
            assert!(abbreviation.chars().all(|c| c.is_ascii_uppercase()), "{abbreviation}");
        }
    }
}

#[test]
fn a_code_reads_in_any_case() {
    let japan = Country::from_code("JP").expect("JP is a country");
    assert_eq!(japan.code(), "JP");
    assert_eq!(japan.names(), &["Japan"]);
    assert_eq!(Country::from_code("jp"), Some(japan));
}

/// A code the standard does not assign names no country — MusicBrainz's
/// `XE` and `XW` and the codes of states that no longer exist among them.
#[test]
fn a_code_outside_the_standard_names_no_country() {
    assert!(Country::from_code("XE").is_none());
    assert!(Country::from_code("XW").is_none());
    assert!(Country::from_code("SU").is_none());
    assert!(Country::from_code("").is_none());
}
