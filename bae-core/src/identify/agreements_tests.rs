use super::*;
use crate::import::MetadataSource;
use crate::signals::SignalOrigin;

fn line(text: &str) -> TextLine {
    TextLine {
        text: text.to_string(),
        origin: SignalOrigin::FolderName,
        file: None,
        region: None,
    }
}

fn text(lines: &[&str]) -> CandidateText {
    CandidateText::of(&lines.iter().copied().map(line).collect::<Vec<_>>(), &[])
}

fn result() -> MetadataResult {
    MetadataResult::for_test(MetadataSource::MusicBrainz, "rel-1", None)
}

const NO_LOOKUP: LookupProvenance = LookupProvenance {
    by_disc_id: false,
    by_barcode: false,
    by_catalog: false,
};

/// The separators a catalog number is printed with vary between the sleeve,
/// the folder name and the provider's record, so they take no part.
#[test]
fn a_catalog_number_is_stated_however_its_separators_fall() {
    let folder = text(&["AC-DC - Dirty Deeds [16033-2]"]);
    for stated in ["16033 2", "16033-2", "160332", "16033.2"] {
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
    let judged = agreements_of(&result(), &text(&["Atlantic 1976 US"]), &NO_LOOKUP);
    assert_eq!(judged, Agreements::NONE);
}

#[test]
fn the_label_the_year_and_the_country_are_read_out_of_the_text() {
    let judged = agreements_of(
        &MetadataResult {
            label: Some("Atlantic Records".to_string()),
            year: Some(1976),
            country: Some("US".to_string()),
            ..result()
        },
        &text(&["Atlantic Records, Inc.", "Made in US · 1976"]),
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
            country: Some("US".to_string()),
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
        },
    );
    assert!(judged.catalog);
}

/// A barcode that came back naming a release the folder says nothing else
/// about read the wrong digits.
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
        &[line("AC-DC - Dirty Deeds [16033-2]")],
        &["16033-2".to_string()],
    );
    let judged = agreements_of(
        &MetadataResult {
            catalog_number: Some("16033-2".to_string()),
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
    let folder = CandidateText::of(&[line("Atlantic 1976 US")], &["1976".to_string()]);
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
        },
    );
    assert!(judged.catalog);
}

/// A provider answers a country as a code and a folder writes it out, so the
/// two have to meet: `JP` is what a folder saying "Japan" states, and `Japan`
/// is what one saying "JP" states.
#[test]
fn a_country_agrees_whichever_of_them_spells_it_out() {
    for (stated, folder) in [
        ("JP", "1979 - Van Halen II (Warner Bros., 20P2-2031, Japan)"),
        ("Japan", "1979 - Van Halen II (Warner Bros., 20P2-2031, JP)"),
        (
            "Japan",
            "1979 - Van Halen II (Warner Bros., 20P2-2031, Japan)",
        ),
        ("US", "Dirty Deeds Done Dirt Cheap (United States)"),
        ("United States", "Dirty Deeds Done Dirt Cheap (US)"),
    ] {
        let judged = agreements_of(
            &MetadataResult {
                country: Some(stated.to_string()),
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(judged.country, "{stated} against {folder}");
    }
}

/// The other spellings are one country's, not any country's: a folder that
/// names a different one states nothing about this release.
#[test]
fn a_country_the_folder_does_not_name_is_no_agreement() {
    for (stated, folder) in [
        (
            "JP",
            "1979 - Van Halen II (Warner Bros., 20P2-2031, Germany)",
        ),
        ("Japan", "1979 - Van Halen II (Warner Bros., 20P2-2031, DE)"),
        ("XW", "1979 - Van Halen II (Warner Bros., 20P2-2031, Japan)"),
    ] {
        let judged = agreements_of(
            &MetadataResult {
                country: Some(stated.to_string()),
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(!judged.country, "{stated} against {folder}");
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
