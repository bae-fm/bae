use super::*;
use crate::import::Catalog;
use crate::signals::TextOrigin;

fn line(text: &str) -> TextLine {
    TextLine {
        text: text.to_string(),
        origin: TextOrigin::FolderName,
        file: None,
        region: None,
    }
}

fn text(lines: &[&str]) -> CandidateText {
    CandidateText::of(&lines.iter().copied().map(line).collect::<Vec<_>>(), &[])
}

fn result() -> MetadataResult {
    MetadataResult::for_test(Catalog::MusicBrainz, "rel-1", None)
}

const NO_LOOKUP: LookupProvenance = LookupProvenance {
    by_disc_id: false,
    by_barcode: false,
    by_catalog: false,
    by_search: false,
    named_by: None,
};

/// The separators a catalog number is printed with vary between the sleeve,
/// the folder name and the provider's record, so they take no part.
#[test]
fn a_catalog_number_is_stated_however_its_separators_fall() {
    let folder = text(&["Artist - Album [10101-2]"]);
    for stated in ["10101 2", "10101-2", "101012", "10101.2"] {
        let judged = agreements_of(
            &MetadataResult {
                catalog_number: Some(stated.to_string()),
                ..result()
            },
            &folder,
            &NO_LOOKUP,
        );
        assert!(judged.catalog, "{stated} is what the folder name states");
    }
}

/// A catalog number the folder never states is no agreement, even when its
/// digits happen to sit inside a longer run of them.
#[test]
fn digits_inside_a_longer_run_state_no_catalog_number() {
    let judged = agreements_of(
        &MetadataResult {
            catalog_number: Some("531 2".to_string()),
            ..result()
        },
        &text(&["0731453120"]),
        &NO_LOOKUP,
    );
    assert!(!judged.catalog);
}

/// A field the result does not state cannot be agreed with.
#[test]
fn a_field_the_result_leaves_out_is_no_agreement() {
    let judged = agreements_of(&result(), &text(&["Harbor 1976 US"]), &NO_LOOKUP);
    assert_eq!(judged, Agreements::NONE);
}

#[test]
fn the_label_the_year_and_the_country_are_read_out_of_the_text() {
    let judged = agreements_of(
        &MetadataResult {
            label: Some("Harbor Records".to_string()),
            year: Some(1976),
            area: Some(crate::pressing::area("US")),
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            ..result()
        },
        &text(&["Harbor Records, Inc.", "Made in US · 1976"]),
        &NO_LOOKUP,
    );
    assert!(judged.label && judged.year && judged.country);
    assert_eq!(judged.count(), 3);
}

/// A country code is two letters, so it has to land on whole words or every
/// folder would agree with every country.
#[test]
fn a_country_code_inside_a_word_states_nothing() {
    let judged = agreements_of(
        &MetadataResult {
            area: Some(crate::pressing::area("US")),
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            ..result()
        },
        &text(&["The House That Blues Built"]),
        &NO_LOOKUP,
    );
    assert!(!judged.country);
}

/// The lookups state the disc ID and the barcode; the text is never asked
/// about them.
#[test]
fn the_disc_id_and_the_barcode_come_from_the_lookups_alone() {
    let judged = agreements_of(
        &result(),
        &CandidateText::default(),
        &LookupProvenance {
            by_disc_id: true,
            by_barcode: true,
            by_catalog: false,
            by_search: false,
            named_by: None,
        },
    );
    assert!(judged.disc_id && judged.barcode);
    assert_eq!(judged.count(), 2);
}

/// The lookup path and the text path land on one badge.
#[test]
fn a_catalog_lookup_agrees_whether_or_not_the_text_states_the_number() {
    let judged = agreements_of(
        &result(),
        &CandidateText::default(),
        &LookupProvenance {
            by_disc_id: false,
            by_barcode: false,
            by_catalog: true,
            by_search: false,
            named_by: None,
        },
    );
    assert!(judged.catalog);
}

/// A barcode that came back naming a release nothing else stands behind read
/// the wrong digits.
#[test]
fn a_barcode_is_the_one_agreement_that_does_not_stand_alone() {
    let barcode_only = Agreements {
        barcode: true,
        ..Agreements::NONE
    };
    assert!(!barcode_only.offered());
    for standing in [
        Agreements {
            disc_id: true,
            ..Agreements::NONE
        },
        Agreements {
            catalog: true,
            ..Agreements::NONE
        },
        Agreements {
            label: true,
            ..Agreements::NONE
        },
        Agreements {
            year: true,
            ..Agreements::NONE
        },
        Agreements {
            country: true,
            ..Agreements::NONE
        },
    ] {
        assert!(standing.offered(), "{standing:?}");
    }
    assert!(!Agreements::NONE.offered());
}

/// A catalog number the person struck out of the text is not read out of it
/// any more: the result that carries it agrees with the folder about nothing.
#[test]
fn a_struck_out_catalog_number_states_nothing() {
    let folder = CandidateText::of(
        &[line("Artist - Album [10101-2]")],
        &["10101-2".to_string()],
    );
    let judged = agreements_of(
        &MetadataResult {
            catalog_number: Some("10101-2".to_string()),
            ..result()
        },
        &folder,
        &NO_LOOKUP,
    );
    assert!(!judged.catalog);
}

/// Striking out a value is striking it out as a catalog number. The same
/// digits read as a year are the year the folder states, which is a different
/// thing about the release and stands on its own.
#[test]
fn striking_out_a_value_leaves_the_other_fields_alone() {
    let folder = CandidateText::of(&[line("Harbor 1976 US")], &["1976".to_string()]);
    let judged = agreements_of(
        &MetadataResult {
            catalog_number: Some("1976".to_string()),
            year: Some(1976),
            ..result()
        },
        &folder,
        &NO_LOOKUP,
    );
    assert!(!judged.catalog);
    assert!(judged.year);
}

/// A catalog lookup that returned the result states its number itself, so
/// striking the number out of the text leaves that agreement standing.
#[test]
fn striking_out_a_number_a_lookup_asked_leaves_its_agreement() {
    let folder = CandidateText::of(&[line("[LBL-1]")], &["LBL-1".to_string()]);
    let judged = agreements_of(
        &MetadataResult {
            catalog_number: Some("LBL-1".to_string()),
            ..result()
        },
        &folder,
        &LookupProvenance {
            by_disc_id: false,
            by_barcode: false,
            by_catalog: true,
            by_search: false,
            named_by: None,
        },
    );
    assert!(judged.catalog);
}

/// A catalog states a country as a code or a name and a folder writes it
/// either way, so the two have to meet: Japan is what a folder saying "Japan"
/// or "JP" states, and a region is stated by any name a catalog writes it as.
#[test]
fn an_area_agrees_whichever_way_the_folder_spells_it() {
    for (area, folder) in [
        ("JP", "1979 - Album (Label, CAT-1, Japan)"),
        ("JP", "1979 - Album (Label, CAT-1, JP)"),
        ("US", "Album (United States)"),
        ("US", "Album (US)"),
        ("XE", "Album (Europe)"),
    ] {
        let judged = agreements_of(
            &MetadataResult {
                area: Some(crate::pressing::area(area)),
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(judged.country, "{area} against {folder}");
    }
}

fn read_off(origin: TextOrigin, text: &str) -> TextLine {
    TextLine {
        origin,
        ..line(text)
    }
}

fn states_country(code: &str, lines: &[TextLine]) -> bool {
    agreements_of(
        &MetadataResult {
            area: Some(crate::pressing::area(code)),
            ..result()
        },
        &CandidateText::of(lines, &[]),
        &NO_LOOKUP,
    )
    .country
}

/// A two-letter code is a country only where a person tagged the folder with
/// it, or where a sleeve says the product was made there. Printed prose is
/// full of two-letter words — "for all of us" in reprinted liner notes — so
/// elsewhere a sleeve, a CUE or a document states a country only by name.
#[test]
fn a_country_code_is_a_tag_in_a_name_and_a_word_everywhere_else() {
    for origin in [
        TextOrigin::Artwork,
        TextOrigin::TextFile,
        TextOrigin::CueSheet,
    ] {
        for printed in ["the band played for all of us.", "ALL OF US"] {
            assert!(
                !states_country("US", &[read_off(origin, printed)]),
                "{origin:?}: {printed}"
            );
        }
        assert!(
            states_country("JP", &[read_off(origin, "Made in Japan")]),
            "{origin:?}"
        );
        assert!(
            states_country("US", &[read_off(origin, "MADE IN US")]),
            "{origin:?}: a statement of where it was made names its code"
        );
        assert!(
            states_country("XE", &[read_off(origin, "Made in the E.U.")]),
            "{origin:?}: and its abbreviation"
        );
    }
    assert!(states_country(
        "US",
        &[read_off(TextOrigin::Filename, "01 - Song (US).flac")]
    ));
}

/// A code tags a folder written as a code is: in capitals. The same letters
/// in lower case are a word in a title.
#[test]
fn a_country_code_in_a_name_is_written_in_capitals() {
    assert!(!states_country("US", &[line("Artist - Songs For Us")]));
    assert!(!states_country("IT", &[line("Artist - Make it Last")]));
    assert!(states_country("IT", &[line("Artist - Album (IT)")]));
}

/// The other spellings are one area's, not any area's: a folder that names a
/// different one states nothing about this release.
#[test]
fn an_area_the_folder_does_not_name_is_no_agreement() {
    for (area, folder) in [
        ("JP", "1979 - Album (Label, CAT-1, Germany)"),
        ("JP", "1979 - Album (Label, CAT-1, DE)"),
        ("XW", "1979 - Album (Label, CAT-1, Japan)"),
    ] {
        let judged = agreements_of(
            &MetadataResult {
                area: Some(crate::pressing::area(area)),
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(!judged.country, "{area} against {folder}");
    }
}

/// A row is one physical object however many sources carry it, so what either
/// source's record of it agrees with is what the row agrees with.
#[test]
fn a_rows_agreements_are_its_records_together() {
    let musicbrainz = Agreements {
        disc_id: true,
        barcode: true,
        ..Agreements::NONE
    };
    let discogs = Agreements {
        barcode: true,
        catalog: true,
        country: true,
        ..Agreements::NONE
    };
    assert_eq!(
        musicbrainz.with(discogs),
        Agreements {
            disc_id: true,
            barcode: true,
            catalog: true,
            country: true,
            ..Agreements::NONE
        }
    );
    assert_eq!(musicbrainz.with(discogs).count(), 4);
    assert_eq!(musicbrainz.with(Agreements::NONE), musicbrainz);
}

/// The trade word a label trails its name with says what kind of business it
/// is, not which one, so the two name one label however either of them writes
/// it: a folder saying "North Star" states a result's "North Star Records",
/// and a folder saying "Harbor Records" states a "Harbor".
#[test]
fn a_label_agrees_without_the_trade_word_either_of_them_prints() {
    for (stated, folder) in [
        (
            "North Star Records",
            "1979 - Album (North Star, AB1-2031, JP)",
        ),
        ("Harbor", "Harbor Records, Inc."),
        ("Meridian Music", "Meridian"),
        ("Meridian", "Meridian Music"),
        ("Grey Stone Records", "Grey Stone Recordings"),
        ("Lantern Record Co.", "Lantern"),
        ("Paper Kite", "Paper Kite"),
    ] {
        let judged = agreements_of(
            &MetadataResult {
                label: Some(stated.to_string()),
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(judged.label, "{stated} against {folder}");
    }
}

/// A name that is nothing but trade words names no label. Every folder that
/// prints "Records" anywhere would otherwise agree with it.
#[test]
fn a_label_that_is_only_trade_words_states_nothing() {
    for stated in ["Records", "Music", "Record Co.", "Music Entertainment"] {
        let judged = agreements_of(
            &MetadataResult {
                label: Some(stated.to_string()),
                ..result()
            },
            &text(&["Harbor Records, Inc.", "Meridian Music Entertainment"]),
            &NO_LOOKUP,
        );
        assert!(!judged.label, "{stated} names no label");
    }
}

/// Dropping the trade word does not make one label another: what is left is
/// still looked for whole.
#[test]
fn a_label_the_folder_does_not_name_is_no_agreement() {
    for (stated, folder) in [
        ("Summit Records", "Harbor Records, Inc."),
        ("North Star Records", "North Music"),
        ("Grey Stone", "Stone Records"),
    ] {
        let judged = agreements_of(
            &MetadataResult {
                label: Some(stated.to_string()),
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(!judged.label, "{stated} against {folder}");
    }
}
