use super::*;
use crate::import::Catalog;
use crate::signals::TextOrigin;

fn line(text: &str) -> TextLine {
    TextLine {
        text: text.to_string(),
        origin: TextOrigin::FolderName,
    }
}

fn text(lines: &[&str]) -> CandidateText {
    CandidateText::of(
        &lines.iter().copied().map(line).collect::<Vec<_>>(),
        &[],
        &[],
    )
}

fn result() -> MetadataResult {
    MetadataResult::for_test(Catalog::MusicBrainz, "rel-1", None)
}

/// What `text` agrees with about `result`, as a list of that one record.
fn judge(result: &MetadataResult, text: &CandidateText, lookup: &LookupProvenance) -> Agreements {
    agreements_of(
        result,
        text,
        &FolderFacts::of(text, std::slice::from_ref(result)),
        lookup,
    )
}

const NO_LOOKUP: LookupProvenance = LookupProvenance {
    by_disc_id: false,
    by_barcode: false,
    by_catalog: false,
    by_isrc: false,
    by_search: false,
    named_by: None,
};

/// The separators a catalog number is printed with vary between the sleeve,
/// the folder name and the provider's record, so they take no part.
#[test]
fn a_catalog_number_is_stated_however_its_separators_fall() {
    let folder = text(&["Artist - Album [10101-2]"]);
    for stated in ["10101 2", "10101-2", "101012", "10101.2"] {
        let judged = judge(
            &MetadataResult {
                labels: vec![ReleaseLabel::of(None, Some(stated))],
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
    let judged = judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(None, Some("531 2"))],
            ..result()
        },
        &text(&["0731453120"]),
        &NO_LOOKUP,
    );
    assert!(!judged.catalog);
}

/// Whether a row carrying `number` agrees with a folder of `lines` about its
/// catalog number.
fn catalog_agrees(number: &str, lines: &[&str]) -> bool {
    judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(None, Some(number))],
            ..result()
        },
        &text(lines),
        &NO_LOOKUP,
    )
    .catalog
}

/// A catalog number is read against the folder's whole numbers: a hyphen,
/// dot or slash between letters or digits keeps a number whole, so a
/// shorter number is never read out of a longer one printed there.
#[test]
fn a_catalog_number_is_never_a_piece_of_a_longer_one() {
    assert!(!catalog_agrees("AB12", &["Artist - Album [AB12-2]"]));
    assert!(!catalog_agrees("12345-2", &["Artist - Album 6789-12345-2"]));
    assert!(!catalog_agrees("12345-2", &["6789.12345/2"]));
    assert!(catalog_agrees("AB12-2", &["Artist - Album [AB12-2]"]));
    assert!(catalog_agrees("6789-12345-2", &["6789-12345-2"]));
}

/// Spaces cut a number the way brackets do, and a number printed with them
/// is read whole across them.
#[test]
fn a_catalog_number_printed_with_spaces_is_read_whole() {
    assert!(catalog_agrees("XYZ 100", &["Label One XYZ 100 (1999)"]));
    assert!(catalog_agrees("XYZ-100", &["Label One XYZ 100 (1999)"]));
    assert!(catalog_agrees("AB12-2", &["AB12 - 2"]));
    assert!(!catalog_agrees("XYZ 100", &["Label One XYZ 100-2"]));
}

/// A number is found wherever it begins in the line, including where an
/// earlier, longer run of the same characters overlaps it.
#[test]
fn a_catalog_number_overlapping_an_earlier_run_is_found() {
    assert!(catalog_agrees("121", &["12 121"]));
}

/// The Catalog # chips read the text as the agreement does, struck-out
/// numbers included.
#[test]
fn the_text_prints_a_catalog_number_only_whole() {
    let folder = CandidateText::of(
        &[line("Artist - Album [AB12-2]")],
        &["AB12-2".to_string()],
        &[],
    );
    assert!(folder.prints_catalog("AB12-2"));
    assert!(!folder.states_catalog("AB12-2"));
    assert!(!folder.prints_catalog("AB12"));
    assert!(!folder.prints_catalog("none"));
}

/// The number a line offers to be picked is the number it confirms: one
/// reader, so "AB 12345-2" is one number both ways, never "AB 12345".
#[test]
fn the_number_offered_is_the_number_confirmed() {
    let printed = "1986 Label AB 12345-2";
    let offered = crate::text_match::printed_catalog_numbers(printed);
    assert_eq!(offered, vec!["AB 12345-2"]);
    let folder = text(&[printed]);
    assert!(folder.prints_catalog(&offered[0]));
    assert!(folder.prints_catalog("AB12345-2"));
    assert!(!folder.prints_catalog("AB 12345"));
}

/// A field the result does not state cannot be agreed with.
#[test]
fn a_field_the_result_leaves_out_is_no_agreement() {
    let judged = judge(&result(), &text(&["Harbor 1976 US"]), &NO_LOOKUP);
    assert_eq!(judged, Agreements::NONE);
}

#[test]
fn the_label_the_year_and_the_country_are_read_out_of_the_text() {
    let judged = judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(Some("Harbor Records"), None)],
            year: Some(1976),
            album_first_year: Some(1971),
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
    let judged = judge(
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
    let judged = judge(
        &result(),
        &CandidateText::default(),
        &LookupProvenance {
            by_disc_id: true,
            by_barcode: true,
            by_catalog: false,
            by_isrc: false,
            by_search: false,
            named_by: None,
        },
    );
    assert!(judged.disc_id && judged.barcode);
    assert_eq!(judged.count(), 2);
}

/// A catalog lookup agrees only through the number itself, not because it
/// returned the release.
#[test]
fn a_catalog_lookup_agrees_only_through_the_number() {
    let catalog_lookup = LookupProvenance {
        by_disc_id: false,
        by_barcode: false,
        by_catalog: true,
        by_isrc: false,
        by_search: false,
        named_by: None,
    };
    let folder = text(&["Artist One - Album One [LBL-719]"]);
    let numbered = |number: &str| MetadataResult {
        labels: vec![ReleaseLabel::of(None, Some(number))],
        ..result()
    };
    assert!(judge(&numbered("LBL 719"), &folder, &catalog_lookup).catalog);
    assert!(!judge(&numbered("LBL 1719"), &folder, &catalog_lookup).catalog);
    assert!(!judge(&result(), &folder, &catalog_lookup).catalog);
}

/// A release on two labels is under both numbers: the folder printing the
/// second label's number agrees with it, and so does its second label's name.
#[test]
fn the_second_labels_number_and_name_are_agreed_with() {
    let judged = judge(
        &MetadataResult {
            labels: vec![
                ReleaseLabel::of(Some("Label A"), Some("AB 100")),
                ReleaseLabel::of(Some("Label B"), Some("CL 719")),
            ],
            ..result()
        },
        &text(&["Artist - Album [CL-719]", "Label B"]),
        &NO_LOOKUP,
    );
    assert!(judged.catalog);
    assert!(judged.label);
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
        &[],
    );
    let judged = judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(None, Some("10101-2"))],
            ..result()
        },
        &folder,
        &NO_LOOKUP,
    );
    assert!(!judged.catalog);
}

/// Striking out a value strikes it as a catalog number only; the same digits
/// still state a year.
#[test]
fn striking_out_a_value_leaves_the_other_fields_alone() {
    let folder = CandidateText::of(&[line("Harbor 1976 US")], &["1976".to_string()], &[]);
    let judged = judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(None, Some("1976"))],
            year: Some(1976),
            album_first_year: Some(1971),
            ..result()
        },
        &folder,
        &NO_LOOKUP,
    );
    assert!(!judged.catalog);
    assert!(judged.year);
}

/// A number the person struck out is not a catalog number, whatever lookup
/// returned a release under it.
#[test]
fn striking_out_a_number_takes_its_agreement_away() {
    let folder = CandidateText::of(&[line("[LBL-1]")], &["LBL-1".to_string()], &[]);
    let judged = judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(None, Some("LBL-1"))],
            ..result()
        },
        &folder,
        &LookupProvenance {
            by_disc_id: false,
            by_barcode: false,
            by_catalog: true,
            by_isrc: false,
            by_search: false,
            named_by: None,
        },
    );
    assert!(!judged.catalog);
}

/// A folder states an area by its name or its code, whichever a catalog
/// gave.
#[test]
fn an_area_agrees_whichever_way_the_folder_spells_it() {
    for (area, folder) in [
        ("JP", "1979 - Album (Label, CAT-1, Japan)"),
        ("JP", "1979 - Album (Label, CAT-1, JP)"),
        ("US", "Album (United States)"),
        ("US", "Album (US)"),
        ("XE", "Album (Europe)"),
    ] {
        let judged = judge(
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
    judge(
        &MetadataResult {
            area: Some(crate::pressing::area(code)),
            ..result()
        },
        &CandidateText::of(lines, &[], &[]),
        &NO_LOOKUP,
    )
    .country
}

/// Whether `lines` state the area MusicBrainz writes as `code`.
fn states_area_of(code: &str, lines: &[&str]) -> bool {
    states_country(
        code,
        &lines
            .iter()
            .map(|text| read_off(TextOrigin::Artwork, text))
            .collect::<Vec<_>>(),
    )
}

/// A name counts in any case; a code or an abbreviation counts in capitals,
/// with or without dots.
#[test]
fn an_area_is_stated_by_its_names_anywhere_and_its_codes_in_capitals() {
    for (code, lines) in [
        (
            "XE",
            &["Unauthorised copying prohibited. Made in the EU."][..],
        ),
        ("XE", &["Manufactured in the E.E.C."]),
        ("GB", &["Album (England)"]),
        ("GB", &["printed in england"]),
        ("GB", &["Album [UK]"]),
        ("US", &["Made in U.S.A."]),
        ("US", &["Distributed in the United States"]),
        ("NL", &["Made in Holland"]),
        ("DE", &["Made in W. Germany"]),
        ("JP", &["1979 - Album (JP)"]),
    ] {
        assert!(states_area_of(code, lines), "{code} in {lines:?}");
    }
}

/// Lowercase codes and an address that writes no US name state no US
/// release.
#[test]
fn prose_and_an_address_state_no_area_they_do_not_write() {
    for lines in [
        &["the band played for all of us."][..],
        &["Artist - Songs For Us"],
        &["Placeholder Records, 100 Placeholder Drive, Beverly Hills, CA 90210"],
        &["made in usa"],
    ] {
        assert!(!states_area_of("US", lines), "{lines:?}");
    }
    assert!(!states_area_of("IT", &["Artist - Make it Last"]));
}

/// A capital code states its area even where it is a word: "LET IT BE"
/// states Italy, which only matters for an Italian release.
#[test]
fn a_stray_capital_code_states_its_area() {
    assert!(states_area_of("IT", &["LET IT BE"]));
    assert!(!states_area_of("JP", &["LET IT BE"]));
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
        let judged = judge(
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

/// The album's title and artist are read out of the text like the other
/// fields, but they name the album, so they neither count nor offer a row.
#[test]
fn the_title_and_the_artist_are_read_out_of_the_text() {
    let judged = judge(
        &MetadataResult {
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            ..result()
        },
        &text(&["Artist Name - Album Title"]),
        &NO_LOOKUP,
    );
    assert!(judged.title && judged.artist);
    assert_eq!(judged.names_album(), 2);
    assert_eq!(judged.count(), 0);
    assert!(!judged.offered());

    let other = judge(
        &MetadataResult {
            title: "Other Title".to_string(),
            artist: Some("Other Name".to_string()),
            ..result()
        },
        &text(&["Artist Name - Album Title"]),
        &NO_LOOKUP,
    );
    assert_eq!(other.names_album(), 0);
    assert_eq!(judged.with(other), judged);
}

/// A title agrees when every word of it is in one line of the text, in any
/// order, as the text spells it; a word the text lacks, or one only another
/// line holds, keeps it from agreeing.
#[test]
fn a_title_agrees_in_any_order_of_its_words() {
    let titled = |title: &str, lines: &[&str]| {
        judge(
            &MetadataResult {
                title: title.to_string(),
                ..result()
            },
            &text(lines),
            &NO_LOOKUP,
        )
        .title
    };
    assert!(titled("Words Album 1999", &["1999 Words Album"]));
    assert!(titled(
        "Words Album 1999",
        &["Artist - 1999 words ALBUM (Label)"]
    ));
    assert!(!titled("Words Album 1999", &["1999 Album"]));
    assert!(!titled("Words Album 1999", &["1999 Album", "Words"]));
}

/// A catalog's title agrees without the bracketed tails it ends on, which
/// name an edition the folder need not write; a bracket that opens the title
/// is part of it, and a title that is nothing but brackets is its own words.
#[test]
fn a_title_agrees_without_its_trailing_brackets() {
    let titled = |title: &str, lines: &[&str]| {
        judge(
            &MetadataResult {
                title: title.to_string(),
                ..result()
            },
            &text(lines),
            &NO_LOOKUP,
        )
        .title
    };
    assert!(titled(
        "Album Title (Remastered)",
        &["Artist Name - Album Title"]
    ));
    assert!(titled(
        "Album Title [Deluxe Edition] (2009)",
        &["Album Title"]
    ));
    assert!(titled(
        "(Leading Words) Album Title?",
        &["(Leading Words) Album Title"]
    ));
    assert!(!titled("(Leading Words) Album Title?", &["Album Title"]));
    assert!(titled("[Untitled]", &["Artist Name - Untitled"]));
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

/// A label agrees whichever side trails its name with a trade word like
/// "Records".
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
        let judged = judge(
            &MetadataResult {
                labels: vec![ReleaseLabel::of(Some(stated), None)],
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(judged.label, "{stated} against {folder}");
    }
}

/// A name that is nothing but trade words names no label.
#[test]
fn a_label_that_is_only_trade_words_states_nothing() {
    for stated in ["Records", "Music", "Record Co.", "Music Entertainment"] {
        let judged = judge(
            &MetadataResult {
                labels: vec![ReleaseLabel::of(Some(stated), None)],
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
        let judged = judge(
            &MetadataResult {
                labels: vec![ReleaseLabel::of(Some(stated), None)],
                ..result()
            },
            &text(&[folder]),
            &NO_LOOKUP,
        );
        assert!(!judged.label, "{stated} against {folder}");
    }
}

fn label_agrees(stated: &str, folder: &str) -> bool {
    judge(
        &MetadataResult {
            labels: vec![ReleaseLabel::of(Some(stated), None)],
            ..result()
        },
        &text(&[folder]),
        &NO_LOOKUP,
    )
    .label
}

/// A label written as its initials agrees with the name they are the initials
/// of, whichever side writes which, and whatever trade word either trails.
#[test]
fn a_label_agrees_with_its_initials() {
    for (stated, folder) in [
        ("ABC", "1979 - Album (Alpha Beta Corporation)"),
        ("Alpha Beta Corporation", "1979 - Album (ABC, AB-1)"),
        ("ABC Records", "Alpha Beta Corporation"),
        ("Alpha Beta Corporation", "ABC Records"),
        ("A.B.C.", "The Alpha and Beta Corporation"),
        ("The Alpha Beta Records", "A.B."),
    ] {
        assert!(label_agrees(stated, folder), "{stated} against {folder}");
    }
}

/// Only a short word written in capitals is read as initials, and only the
/// initials of the other name's words agree with it.
#[test]
fn only_capital_initials_of_the_name_agree() {
    for (stated, folder) in [
        ("abc", "Alpha Beta Corporation"),
        ("Alpha Beta Corporation", "abc"),
        ("ABD", "Alpha Beta Corporation"),
        ("Alpha Beta Corporation", "ABD"),
        ("ABCDE", "Alpha Beta Corporation Delta Echo"),
        ("Alpha Beta Corporation Delta Echo", "ABCDE"),
        ("A", "Alpha"),
    ] {
        assert!(!label_agrees(stated, folder), "{stated} against {folder}");
    }
}
