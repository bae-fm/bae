use super::*;
use bae_catalogs::catalog::LookupCatalog::{Discogs, MusicBrainz};
use bae_types::pressing::{Country, Medium, ReleaseLabel, StatedMedia};

fn record(source: LookupCatalog, id: &str) -> MetadataResult {
    MetadataResult::for_test(source, id, None)
}

fn pressing(releases: Vec<MetadataResult>) -> Pressing {
    Pressing { releases }
}

fn statement<T>(value: T, catalogs: &[LookupCatalog]) -> Statement<T> {
    Statement { value, catalogs: catalogs.to_vec() }
}

/// Records that state one year state it once, naming no catalog.
#[test]
fn records_agreeing_on_a_year_list_it_once_with_no_catalog() {
    let row = pressing(vec![
        MetadataResult { year: Some(1992), ..record(MusicBrainz, "mb") },
        MetadataResult { year: Some(1992), ..record(Discogs, "dg") },
    ]);
    assert_eq!(row.statements().years, vec![statement(1992, &[])]);
}

/// Each record's year is listed with the catalog that states it.
#[test]
fn records_stating_two_years_list_both_with_their_catalogs() {
    let row = pressing(vec![
        MetadataResult { year: Some(1992), ..record(MusicBrainz, "mb") },
        MetadataResult { year: Some(1993), ..record(Discogs, "dg") },
    ]);
    assert_eq!(
        row.statements().years,
        vec![statement(1992, &[MusicBrainz]), statement(1993, &[Discogs])]
    );
}

/// A year only one record states is listed as stated.
#[test]
fn a_year_one_record_states_alone_names_no_catalog() {
    let row = pressing(vec![
        record(MusicBrainz, "mb"),
        MetadataResult { year: Some(1993), ..record(Discogs, "dg") },
    ]);
    assert_eq!(row.statements().years, vec![statement(1993, &[])]);
}

/// The lead's silence on a country does not hide the partner's.
#[test]
fn a_country_only_the_partner_states_is_listed() {
    let us = ReleaseArea::Country(Country::from_code("US").expect("a country"));
    let row = pressing(vec![
        record(MusicBrainz, "mb"),
        MetadataResult { area: Some(us), ..record(Discogs, "dg") },
    ]);
    assert_eq!(row.statements().areas, vec![statement(us, &[])]);
}

/// Two spellings of one catalog number are one number, so records writing it
/// two ways do not differ; a second number one of them adds does.
#[test]
fn catalog_numbers_differ_by_what_they_say_not_how_they_are_spelled() {
    let numbered = |source, id: &str, numbers: &[&str]| MetadataResult {
        labels: numbers.iter().map(|n| ReleaseLabel::of(Some("Harbor Records"), Some(n))).collect(),
        ..record(source, id)
    };
    let alike = pressing(vec![
        numbered(MusicBrainz, "mb", &["HR-1042"]),
        numbered(Discogs, "dg", &["HR 1042"]),
    ]);
    let lines = alike.statements().labels;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].catalog_numbers, vec![statement("HR-1042".to_string(), &[])]);

    let differing = pressing(vec![
        numbered(MusicBrainz, "mb", &["HR-1042"]),
        numbered(Discogs, "dg", &["HR 1042", "HRX-7"]),
    ]);
    let lines = differing.statements().labels;
    assert_eq!(
        lines[0].catalog_numbers,
        vec![
            statement("HR-1042".to_string(), &[MusicBrainz, Discogs]),
            statement("HRX-7".to_string(), &[Discogs]),
        ]
    );
}

/// A barcode is told apart by its digits, not its spacing.
#[test]
fn barcodes_differ_by_digits_not_spacing() {
    let coded = |source, id: &str, code: &str| MetadataResult {
        barcodes: vec![code.to_string()],
        ..record(source, id)
    };
    let alike = pressing(vec![
        coded(MusicBrainz, "mb", "041234567892"),
        coded(Discogs, "dg", "0 41234 56789 2"),
    ]);
    assert_eq!(alike.statements().barcodes, vec![statement("041234567892".to_string(), &[])]);

    let differing = pressing(vec![
        coded(MusicBrainz, "mb", "041234567892"),
        coded(Discogs, "dg", "5012345678900"),
    ]);
    assert_eq!(
        differing.statements().barcodes,
        vec![
            statement("041234567892".to_string(), &[MusicBrainz]),
            statement("5012345678900".to_string(), &[Discogs]),
        ]
    );
}

/// Records stating different formats list each.
#[test]
fn records_stating_different_formats_list_each() {
    let made_of = |source, id: &str, media: Vec<Option<Medium>>| MetadataResult {
        media: StatedMedia::PerMedium(media),
        ..record(source, id)
    };
    let row = pressing(vec![
        made_of(MusicBrainz, "mb", vec![Some(Medium::Cd)]),
        made_of(Discogs, "dg", vec![Some(Medium::Cd), Some(Medium::Cd)]),
    ]);
    let media = row.statements().media;
    assert_eq!(media.len(), 2);
    assert_eq!(media[0].catalogs, vec![MusicBrainz]);
    assert_eq!(media[1].catalogs, vec![Discogs]);
}

/// A note a row's records write differently lists each, a note both write
/// once.
#[test]
fn notes_list_every_record_s_with_its_catalog() {
    let notes = |mb: Option<&str>, dg: Option<&str>| {
        pressing(vec![record(MusicBrainz, "mb"), record(Discogs, "dg")]).noted(|release| {
            match release.source {
                MusicBrainz => mb,
                Discogs => dg,
            }
            .map(str::to_string)
        })
    };
    assert_eq!(
        notes(Some("first pressing"), Some("club edition")),
        vec![
            statement("first pressing".to_string(), &[MusicBrainz]),
            statement("club edition".to_string(), &[Discogs]),
        ]
    );
    assert_eq!(
        notes(Some("first pressing"), Some("first pressing")),
        vec![statement("first pressing".to_string(), &[])]
    );
    assert_eq!(notes(None, Some("club edition")), vec![statement("club edition".to_string(), &[])]);
}
