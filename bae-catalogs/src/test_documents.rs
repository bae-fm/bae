//! Catalog documents the catalog model's tests and the stored release's tests
//! read: what each catalog would answer for one imaginary release.

use serde_json::json;

/// A Discogs vinyl release, 7811, on master 7822: its own year, label and
/// two sided tracks.
pub fn linked_discogs_release() -> String {
    json!({
        "id": 7811, "master_id": 7822, "title": "Selected Album", "year": 2005,
        "artists": [{"id": 7833, "name": "Selected Artist"}],
        "labels": [{"name": "Selected Label", "catno": "S-7811"}],
        "formats": [{"name": "Vinyl"}],
        "tracklist": [
            {"position": "A1", "title": "First Track", "type_": "track"},
            {"position": "B1", "title": "Second Track", "type_": "track"}
        ]
    })
    .to_string()
}

/// The MusicBrainz release group the Discogs master 7822 names back, naming
/// the album on Discogs and AllMusic.
pub fn linked_release_group() -> String {
    json!({
        "id": "album-group", "title": "Linked Album", "first-release-date": "1979",
        "artist-credit": [{"name": "Selected Artist", "artist": {"id": "artist-id", "name": "Selected Artist"}}],
        "relations": [
            {"url": {"resource": "https://www.discogs.com/master/7822"}},
            {"url": {"resource": "https://www.allmusic.com/album/mw0000007811"}}
        ]
    }).to_string()
}

/// The Discogs master 7822: the album's title, first year and artist.
pub fn linked_discogs_master() -> String {
    json!({"id":7822,"title":"Master Album","year":1980,"artists":[{"id":7833,"name":"Selected Artist"}]}).to_string()
}

/// The MusicBrainz release cross-linked to the Discogs release 7811, in its
/// release group `album-group`.
pub fn linked_musicbrainz_release() -> String {
    json!({
        "id": "linked-release", "title": "Linked Album", "date": "1999", "country": "JP", "barcode": "1234567890123",
        "artist-credit": [{"name":"Selected Artist", "artist":{"id":"artist-id", "name":"Selected Artist"}}],
        "release-group":{"id":"album-group","first-release-date":"1979"},
        "label-info":[{"label":{"name":"Linked Label"},"catalog-number":"OTHER-1"}],
        "media":[{"format":"CD", "tracks":[{"number":"1", "title":"Different Track"}]}],
        "relations":[{"url":{"resource":"https://www.discogs.com/release/7811"}}],
        "cover-art-archive":{"front":false,"darkened":false}
    }).to_string()
}

/// A MusicBrainz release in `group_id` whose url-rels name each of `urls`.
pub fn musicbrainz_release_with_relations(
    id: &str,
    group_id: &str,
    urls: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": "Album Title",
        "artist-credit": [],
        "label-info": [],
        "media": [],
        "release-group": { "id": group_id },
        "cover-art-archive": { "front": false, "darkened": false },
        "relations": urls
            .iter()
            .map(|url| serde_json::json!({
                "target-type": "url",
                "type": "other databases",
                "url": { "resource": url }
            }))
            .collect::<Vec<_>>(),
    })
}

/// A release-group document whose url-rels name a Wikidata item, which is
/// where MusicBrainz editors file the link.
pub fn wikidata_linked_release_group(group: &str, item: &str) -> String {
    serde_json::json!({
        "id": group,
        "relations": [
            { "url": { "resource": format!("https://www.wikidata.org/wiki/{item}") } }
        ]
    })
    .to_string()
}

/// One item's entity document, stating the album's key in three catalogs: the
/// Discogs master nothing else names, plus two catalogs MusicBrainz links to
/// neither of.
pub fn wikidata_item(item: &str) -> String {
    serde_json::json!({
        "entities": {
            item: {
                "claims": {
                    "P1954": [{ "mainsnak": { "datavalue": { "value": "909090" } } }],
                    "P1729": [{ "mainsnak": { "datavalue": { "value": "mw0000424242" } } }],
                    "P2205": [{
                        "mainsnak": { "datavalue": { "value": "4242424242424242424242" } }
                    }]
                }
            }
        }
    })
    .to_string()
}
