use super::*;

fn release(source: Catalog, release_id: &str) -> MetadataResult {
    MetadataResult::for_test(source, release_id, None)
}

fn evidence(a: &MetadataResult, b: &MetadataResult) -> PressingEvidence {
    PressingEvidence::between(&ComparedPressing::of(a), &ComparedPressing::of(b))
}

/// A stated barcode that is not a code is skipped, which leaves the record
/// with no barcode to compare rather than a fabricated one.
#[test]
fn an_unusable_barcode_leaves_the_comparison_unknown() {
    let mut a = release(Catalog::MusicBrainz, "mb-1");
    a.barcodes = vec!["none".to_string()];
    let mut b = release(Catalog::Discogs, "dg-1");
    b.barcodes = vec!["012345678905".to_string()];
    assert_eq!(evidence(&a, &b).barcode, Comparison::Unknown);

    b.barcodes = vec!["5051961234567".to_string(), "012345678905".to_string()];
    a.barcodes = vec!["0 01234 56789 05".to_string()];
    assert_eq!(
        evidence(&a, &b).barcode,
        Comparison::Same,
        "any usable key on either side"
    );
}

/// Two countries are one or two; a region matches only itself and
/// contradicts nothing, since it spans countries and overlaps other regions.
#[test]
fn countries_and_regions_compare_by_what_they_name() {
    use crate::pressing::{area, Region, ReleaseArea};
    let mut a = release(Catalog::MusicBrainz, "mb-1");
    let mut b = release(Catalog::Discogs, "dg-1");
    a.area = Some(area("JP"));
    b.area = ReleaseArea::discogs("Japan");
    assert_eq!(evidence(&a, &b).country, Comparison::Same);
    b.area = Some(area("US"));
    assert_eq!(evidence(&a, &b).country, Comparison::Different);
    a.area = Some(area("XE"));
    b.area = ReleaseArea::discogs("Europe");
    assert_eq!(evidence(&a, &b).country, Comparison::Same);
    b.area = Some(ReleaseArea::Region(Region::UkAndEurope));
    assert_eq!(
        evidence(&a, &b).country,
        Comparison::Unknown,
        "two regions that overlap"
    );
    b.area = Some(area("XW"));
    assert_eq!(
        evidence(&a, &b).country,
        Comparison::Unknown,
        "a worldwide release is sold in Europe too"
    );
    b.area = Some(area("DE"));
    assert_eq!(
        evidence(&a, &b).country,
        Comparison::Unknown,
        "a region against a country"
    );
    b.area = None;
    assert_eq!(evidence(&a, &b).country, Comparison::Unknown);
}

/// The trade word a label trails is dropped; anything else different is
/// inconclusive, never a different label.
#[test]
fn labels_agree_by_name_and_never_disagree() {
    let mut a = release(Catalog::MusicBrainz, "mb-1");
    let mut b = release(Catalog::Discogs, "dg-1");
    a.label = Some("Label Name Records".to_string());
    b.label = Some("Label Name".to_string());
    assert_eq!(evidence(&a, &b).label, Comparison::Same);
    b.label = Some("LN".to_string());
    assert_eq!(evidence(&a, &b).label, Comparison::Unknown);
    b.label = None;
    assert_eq!(evidence(&a, &b).label, Comparison::Unknown);
}

/// A catalog number is compared as its letters and digits; "none" states
/// that there is none.
#[test]
fn catalog_numbers_compare_squashed_and_never_disagree() {
    let mut a = release(Catalog::MusicBrainz, "mb-1");
    let mut b = release(Catalog::Discogs, "dg-1");
    a.catalog_number = Some("WPCR-80001".to_string());
    b.catalog_number = Some("wpcr 80001".to_string());
    assert_eq!(evidence(&a, &b).catalog, Comparison::Same);
    b.catalog_number = Some("WPCR-80002".to_string());
    assert_eq!(evidence(&a, &b).catalog, Comparison::Unknown);
    a.catalog_number = Some("[none]".to_string());
    b.catalog_number = Some("none".to_string());
    assert_eq!(evidence(&a, &b).catalog, Comparison::Unknown);
}

/// A MusicBrainz record's media, one format name per medium, read in
/// MusicBrainz's list.
fn per_medium(names: &[&str]) -> StatedMedia {
    crate::pressing::musicbrainz::media("mb-1", names.iter().map(|name| Some(*name)))
}

/// A Discogs record's format entries, each a format name and its
/// descriptions, read in Discogs's lists.
fn formats(entries: &[(&str, &[&str])]) -> StatedMedia {
    let entries: Vec<crate::discogs::DiscogsFormat> = entries
        .iter()
        .map(|(name, descriptions)| crate::discogs::DiscogsFormat {
            name: name.to_string(),
            qty: "1".to_string(),
            descriptions: descriptions.iter().map(|d| d.to_string()).collect(),
        })
        .collect();
    crate::pressing::discogs_formats::read("dg-1", &entries).media
}

/// What a MusicBrainz record stating `a` and a Discogs record stating `b`
/// say about the pressing's media.
fn media(a: StatedMedia, b: StatedMedia) -> Comparison {
    let mut one = release(Catalog::MusicBrainz, "mb-1");
    one.media = a;
    let mut other = release(Catalog::Discogs, "dg-1");
    other.media = b;
    evidence(&one, &other).medium
}

/// Each catalog's words are read in that catalog's own list of format names,
/// and two records that each say what they are made of either name the same
/// carriers or contradict each other.
#[test]
fn media_compare_as_the_carriers_each_catalogs_words_name() {
    assert_eq!(
        media(per_medium(&["CD"]), formats(&[("CD", &["Album"])])),
        Comparison::Same
    );
    assert_eq!(
        media(
            per_medium(&["CD"]),
            formats(&[("File", &["FLAC", "Album", "Reissue"])])
        ),
        Comparison::Different,
        "a download is not a CD"
    );
    assert_eq!(
        media(
            per_medium(&["CD", "DVD-Video"]),
            formats(&[("CD", &["Album"]), ("DVD", &["DVD-Video", "NTSC"])])
        ),
        Comparison::Same
    );
    assert_eq!(
        media(
            per_medium(&["CD", "DVD-Video"]),
            formats(&[("CD", &["Album"])])
        ),
        Comparison::Different,
        "the Discogs record names what it is made of, and there is no DVD in it"
    );
    assert_eq!(
        media(
            per_medium(&["Hybrid SACD"]),
            formats(&[("SACD", &["Hybrid", "Multichannel"])])
        ),
        Comparison::Same,
        "a hybrid SACD is an SACD on both catalogs"
    );
}

/// The case a catalog writes its own name in is not evidence of anything.
#[test]
fn a_format_name_is_the_same_name_in_any_case() {
    assert_eq!(
        media(per_medium(&["cd"]), formats(&[("CD", &[])])),
        Comparison::Same
    );
}

/// A Discogs description outside its list says nothing bae reads, so it is
/// passed over; a MusicBrainz medium names a format, so a word outside that
/// list leaves the medium unknown. Either way the word is logged when it is
/// read, because what needs fixing is the vocabulary.
#[test]
fn a_word_outside_the_vocabulary_is_logged_and_settles_by_catalog() {
    let logs = crate::test_logs::capture_warn_logs(|| {
        assert_eq!(
            media(
                per_medium(&["CD"]),
                formats(&[("CD", &["Album", "Zorblax"])])
            ),
            Comparison::Same
        );
    });
    assert!(
        logs.contains("Zorblax"),
        "the unknown word is logged: {logs}"
    );

    let logs = crate::test_logs::capture_warn_logs(|| {
        assert_eq!(
            media(
                per_medium(&["CD", "Zorblax Disc"]),
                formats(&[("CD", &["Album"])])
            ),
            Comparison::Unknown,
            "what the second MusicBrainz medium is cannot be said"
        );
    });
    assert!(
        logs.contains("Zorblax Disc"),
        "the unknown word is logged: {logs}"
    );
}

/// A record with a medium it does not name still contradicts a record that
/// accounts for everything it is made of and does not hold what this one
/// knows; what it says nothing about is what leaves the comparison open.
#[test]
fn a_medium_left_unstated_is_weighed_against_a_complete_account() {
    assert_eq!(
        media(
            StatedMedia::PerMedium(vec![Some(crate::pressing::Medium::Cd), None]),
            formats(&[("Cassette", &[])])
        ),
        Comparison::Different,
        "the Discogs entries are the release's formats, and a CD is not among them"
    );
    assert_eq!(
        media(
            StatedMedia::PerMedium(vec![Some(crate::pressing::Medium::Cd), None]),
            formats(&[("CD", &["Album"])])
        ),
        Comparison::Unknown,
        "the Discogs record lacks nothing the MusicBrainz one knows, and what \
         the unstated medium is nobody says"
    );
    assert_eq!(
        media(per_medium(&["CD"]), StatedMedia::Undescribed),
        Comparison::Unknown
    );
    assert_eq!(
        media(
            StatedMedia::Undescribed,
            formats(&[("Hybrid", &["Album", "Reissue"])])
        ),
        Comparison::Unknown,
        "a hybrid disc whose kind is not stated names no carrier"
    );
}

/// A shared catalog number is never a candidate, however much else agrees:
/// a label can keep one number on every reissue. A contradiction removes an
/// inferred candidate but not a linked one.
#[test]
fn support_needs_identity_evidence_and_no_contradiction() {
    let mut a = release(Catalog::MusicBrainz, "mb-1");
    let mut b = release(Catalog::Discogs, "dg-1");
    a.catalog_number = Some("CAT-7".to_string());
    b.catalog_number = Some("CAT-7".to_string());
    a.year = Some(1992);
    b.year = Some(1992);
    assert_eq!(evidence(&a, &b).support(), None);
    b.year = Some(1993);

    a.links = vec![MetadataRef::new(Catalog::Discogs, "dg-1")];
    let linked = evidence(&a, &b)
        .support()
        .expect("a link stands despite the year");
    a.links.clear();
    a.barcodes = vec!["012345678905".to_string()];
    b.barcodes = vec!["012345678905".to_string()];
    b.year = Some(1992);
    let barcoded = evidence(&a, &b).support().expect("a shared barcode");
    assert!(linked > barcoded, "a stated link outranks an inferred pair");
}

/// Two records of one catalog are never one pressing, even sharing a
/// barcode: the catalog's editors kept them apart for a difference these
/// facts do not read, such as a matrix or a pressing plant.
#[test]
fn two_records_of_one_catalog_are_never_one_pressing() {
    let mut a = release(Catalog::Discogs, "dg-1");
    let mut b = release(Catalog::Discogs, "dg-2");
    a.barcodes = vec!["012345678905".to_string()];
    b.barcodes = vec!["012345678905".to_string()];
    a.catalog_number = Some("CAT-7".to_string());
    b.catalog_number = Some("CAT-7".to_string());
    assert_eq!(evidence(&a, &b).support(), None);
}
