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
    CandidateText::of(&lines.iter().copied().map(line).collect::<Vec<_>>())
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
