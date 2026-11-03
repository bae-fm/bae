use super::*;
use crate::catalog::LookupCatalog;
use bae_types::pressing::ReleaseLabel;

/// A typed query builds one request per provider from the same fields.
#[test]
fn a_general_query_builds_both_providers_requests() {
    let query = SearchQuery::General {
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
    };
    let musicbrainz = query.musicbrainz_params();
    assert_eq!(musicbrainz.artist.as_deref(), Some("Artist Name"));
    assert_eq!(musicbrainz.album.as_deref(), Some("Album Title"));
    let discogs = query.discogs_params();
    assert_eq!(
        discogs.text.as_deref(),
        Some("Artist Name"),
        "the artist is matched as words, not as Discogs's main artist name"
    );
    assert_eq!(discogs.release_title.as_deref(), Some("Album Title"));
}

#[test]
fn catalog_and_barcode_queries_fill_their_own_provider_fields() {
    let catalog = SearchQuery::CatalogNumber { catalog_number: "CAT-7".to_string() };
    assert_eq!(catalog.musicbrainz_params().catalog_number.as_deref(), Some("CAT-7"));
    assert_eq!(catalog.discogs_params().catno.as_deref(), Some("CAT-7"));

    // A code is asked for in its one spelling: a UPC-A as its EAN-13,
    // however it was typed.
    for typed in ["012345678905", " 0 12345 67890 5 ", "0012345678905"] {
        let barcode = SearchQuery::Barcode { barcode: typed.to_string() };
        assert_eq!(
            barcode.musicbrainz_params().barcode.as_deref(),
            Some("0012345678905"),
            "{typed}"
        );
        assert_eq!(barcode.discogs_params().barcode.as_deref(), Some("0012345678905"), "{typed}");
    }

    // A value whose check digit fails is still asked for, by its digits.
    let mistyped = SearchQuery::Barcode { barcode: "0 12345 67890 6".to_string() };
    assert_eq!(mistyped.musicbrainz_params().barcode.as_deref(), Some("012345678906"));
}

#[test]
fn a_barcode_query_names_why_its_value_is_no_product_code() {
    use bae_types::barcode::Unusable;
    let unusable =
        |typed: &str| SearchQuery::Barcode { barcode: typed.to_string() }.unusable_barcode();
    assert_eq!(unusable("CAT-719"), Some(Unusable::NotACode));
    assert_eq!(unusable("12345"), Some(Unusable::TooShort));
    assert_eq!(unusable("0000000000000"), Some(Unusable::Placeholder));
    assert_eq!(unusable("2100000123457"), Some(Unusable::StoreInternal));
    assert_eq!(unusable("0 12345 67890 5"), None);
    assert_eq!(unusable("0 12345 67890 6"), None, "a check-digit slip is still a code");

    let catalog = SearchQuery::CatalogNumber { catalog_number: "CAT-719".to_string() };
    assert_eq!(catalog.unusable_barcode(), None);
}

/// A catalog-number lookup answers with the releases under that number and
/// no other, however either writes its spacing, case or hyphens: the
/// catalogs' own searches match the number inside longer ones.
#[test]
fn a_catalog_number_lookup_keeps_only_its_number() {
    let numbered = |release_id: &str, number: Option<&str>| MetadataResult {
        labels: number
            .map(|number| ReleaseLabel::of(Some("Label A"), Some(number)))
            .into_iter()
            .collect(),
        ..MetadataResult::for_test(LookupCatalog::Discogs, release_id, None)
    };
    let mut results = vec![
        numbered("exact", Some("LBL 719")),
        numbered("hyphenated", Some("lbl-719")),
        numbered("longer", Some("LBL 1719")),
        numbered("prefixed", Some("XLBL-719")),
        numbered("unnumbered", None),
    ];
    SearchQuery::CatalogNumber { catalog_number: "LBL 719".to_string() }.keep_answers(&mut results);
    assert_eq!(
        results.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["exact", "hyphenated"]
    );
}

/// A release on two labels is under both labels' numbers, so a lookup of the
/// second label's number keeps it.
#[test]
fn a_catalog_number_lookup_keeps_a_release_by_its_second_labels_number() {
    let mut results = vec![
        MetadataResult {
            labels: vec![
                ReleaseLabel::of(Some("Label A"), Some("AB 100")),
                ReleaseLabel::of(Some("Label B"), Some("CL 719")),
            ],
            ..MetadataResult::for_test(LookupCatalog::MusicBrainz, "two-labels", None)
        },
        MetadataResult {
            labels: vec![ReleaseLabel::of(Some("Label B"), Some("CL 7190"))],
            ..MetadataResult::for_test(LookupCatalog::MusicBrainz, "longer", None)
        },
    ];
    SearchQuery::CatalogNumber { catalog_number: "CL-719".to_string() }.keep_answers(&mut results);
    assert_eq!(
        results.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["two-labels"]
    );
}

/// A run's catalog-number search names the artist and the label on both
/// catalogs: MusicBrainz as query terms beside the number, Discogs with the
/// artist matched as words anywhere on a release and the label as its label
/// filter. One that names neither asks for the number alone.
#[test]
fn a_narrowed_catalog_number_search_names_the_artist_and_label_on_both_catalogs() {
    let narrowed = CatalogNumberSearch {
        number: "AB 100".to_string(),
        artist: Some("Artist Name".to_string()),
        label: Some("Label A".to_string()),
    };
    let musicbrainz = narrowed.musicbrainz_params();
    assert_eq!(musicbrainz.catalog_number.as_deref(), Some("AB 100"));
    assert_eq!(musicbrainz.artist.as_deref(), Some("Artist Name"));
    assert_eq!(musicbrainz.label.as_deref(), Some("Label A"));
    let discogs = narrowed.discogs_params();
    assert_eq!(discogs.catno.as_deref(), Some("AB 100"));
    assert_eq!(discogs.text.as_deref(), Some("Artist Name"));
    assert_eq!(discogs.label.as_deref(), Some("Label A"));

    let alone = CatalogNumberSearch { number: "AB 100".to_string(), artist: None, label: None };
    let musicbrainz = alone.musicbrainz_params();
    assert_eq!(
        (musicbrainz.artist, musicbrainz.label),
        (None, None),
        "the number alone names no artist or label on MusicBrainz"
    );
    let discogs = alone.discogs_params();
    assert_eq!((discogs.text, discogs.label), (None, None), "nor on Discogs");
}

/// A narrowed search keeps only the releases printing its number, as the
/// typed one does: the catalogs match the number inside longer ones.
#[test]
fn a_narrowed_catalog_number_search_keeps_only_its_number() {
    let mut results = vec![
        MetadataResult {
            labels: vec![ReleaseLabel::of(Some("Label A"), Some("ab-100"))],
            ..MetadataResult::for_test(LookupCatalog::Discogs, "exact", None)
        },
        MetadataResult {
            labels: vec![ReleaseLabel::of(Some("Label A"), Some("AB 1100"))],
            ..MetadataResult::for_test(LookupCatalog::Discogs, "longer", None)
        },
    ];
    CatalogNumberSearch {
        number: "AB 100".to_string(),
        artist: Some("Artist Name".to_string()),
        label: None,
    }
    .keep_answers(&mut results);
    assert_eq!(
        results.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["exact"]
    );
}
