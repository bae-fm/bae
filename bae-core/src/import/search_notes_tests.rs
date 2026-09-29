//! The notes a search result carries before any document is read.

use super::*;


/// A search result carries the free text its catalog writes about the
/// pressing before any document is read: a MusicBrainz release's
/// disambiguation, and what a Discogs format entry writes beside its name.
#[test]
fn a_search_result_carries_its_notes() {
    let release: crate::musicbrainz::SearchRelease = serde_json::from_value(serde_json::json!({
        "id": "mb-release-1",
        "title": "Album Title",
        "disambiguation": "made in Nordland",
    }))
    .expect("search release parses");
    assert_eq!(
        search_release_to_metadata(release, None).notes,
        vec!["made in Nordland".to_string()]
    );

    let result = crate::discogs::client::DiscogsSearchResult {
        id: 1,
        title: "Artist - Album".to_string(),
        year: None,
        formats: vec![crate::discogs::DiscogsFormat {
            name: "Vinyl".to_string(),
            qty: "1".to_string(),
            descriptions: vec!["LP".to_string()],
            text: Some("Small label".to_string()),
        }],
        country: None,
        label: None,
        catno: None,
        barcode: Vec::new(),
        cover_image: None,
        thumb: None,
        master_id: None,
        result_type: "release".to_string(),
    };
    assert_eq!(
        discogs_search_result_to_metadata(result).notes,
        vec!["Small label".to_string()]
    );
}
