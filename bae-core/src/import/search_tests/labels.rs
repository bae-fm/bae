//! A release's labels as each search reads them, and the catalog-number
//! lookup that compares against every one of their numbers.

use super::*;

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
        ..MetadataResult::for_test(Catalog::Discogs, release_id, None)
    };
    let mut results = vec![
        numbered("exact", Some("LBL 719")),
        numbered("hyphenated", Some("lbl-719")),
        numbered("longer", Some("LBL 1719")),
        numbered("prefixed", Some("XLBL-719")),
        numbered("unnumbered", None),
    ];
    SearchQuery::CatalogNumber {
        catalog_number: "LBL 719".to_string(),
    }
    .keep_answers(&mut results);
    assert_eq!(
        results
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
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
            ..MetadataResult::for_test(Catalog::MusicBrainz, "two-labels", None)
        },
        MetadataResult {
            labels: vec![ReleaseLabel::of(Some("Label B"), Some("CL 7190"))],
            ..MetadataResult::for_test(Catalog::MusicBrainz, "longer", None)
        },
    ];
    SearchQuery::CatalogNumber {
        catalog_number: "CL-719".to_string(),
    }
    .keep_answers(&mut results);
    assert_eq!(
        results
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["two-labels"]
    );
}

/// A MusicBrainz search response lists every label with its number, and the
/// result keeps each pair in order.
#[test]
fn a_musicbrainz_search_result_keeps_every_label() {
    let release: crate::musicbrainz::SearchRelease = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album",
        "label-info": [
            { "catalog-number": "AB 100", "label": { "name": "Label A" } },
            { "catalog-number": "CL 719", "label": { "name": "Label B" } }
        ]
    }))
    .expect("search release parses");
    assert_eq!(
        search_release_to_metadata(release, None).labels,
        vec![
            ReleaseLabel::of(Some("Label A"), Some("AB 100")),
            ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ]
    );
}

/// A Discogs search result states the first label's number and no other, and
/// lists label and company names together: the result reads the first label
/// alone, its name with its number.
#[test]
fn a_discogs_search_result_reads_its_first_label() {
    let mut result = result_with_title("Artist - Album");
    result.label = Some(vec![
        "Label A".to_string(),
        "Label B".to_string(),
        "Pressing Plant".to_string(),
    ]);
    result.catno = Some("AB 100".to_string());
    assert_eq!(
        discogs_search_result_to_metadata(result).labels,
        vec![ReleaseLabel::of(Some("Label A"), Some("AB 100"))]
    );
}
