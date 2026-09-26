use super::*;

/// One item's entity document, shaped the way `Special:EntityData` answers: a
/// single entity under `entities`, its claims keyed by property, each statement
/// stating its value in `mainsnak.datavalue.value`.
///
/// Trimmed to the claims — an item's labels, descriptions, aliases and
/// sitelinks describe the album in prose, which bae reads from the catalogs it
/// asks. What is kept is one statement per property the table maps, plus the
/// three kinds of claim that name no catalog page: a property bae has no
/// catalog for, a value that is an object rather than an identifier, and a snak
/// that states no value at all.
const ENTITY_DOCUMENT: &str = r#"{
  "entities": {
    "Q424242": {
      "type": "item",
      "id": "Q424242",
      "claims": {
        "P31": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P31",
              "datavalue": {
                "value": { "entity-type": "item", "numeric-id": 482994, "id": "Q482994" },
                "type": "wikibase-entityid"
              },
              "datatype": "wikibase-item"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P577": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P577",
              "datavalue": {
                "value": {
                  "time": "+2024-01-01T00:00:00Z",
                  "timezone": 0,
                  "precision": 11,
                  "calendarmodel": "http://www.wikidata.org/entity/Q1985727"
                },
                "type": "time"
              },
              "datatype": "time"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P9965": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P9965",
              "datavalue": { "value": "424242", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P436": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P436",
              "datavalue": { "value": "mb-group", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P1954": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P1954",
              "datavalue": { "value": "909090", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P1729": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P1729",
              "datavalue": { "value": "mw0000424242", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal",
            "qualifiers": {
              "P50": [
                {
                  "snaktype": "value",
                  "property": "P50",
                  "datavalue": {
                    "value": { "entity-type": "item", "numeric-id": 424242, "id": "Q424242" },
                    "type": "wikibase-entityid"
                  },
                  "datatype": "wikibase-item"
                }
              ]
            }
          }
        ],
        "P8392": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P8392",
              "datavalue": { "value": "album/artist-name/album-title", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P6217": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P6217",
              "datavalue": { "value": "Artist-name/Album-title", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P2205": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P2205",
              "datavalue": { "value": "4242424242424242424242", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P2281": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P2281",
              "datavalue": { "value": "424242424", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P2723": [
          {
            "mainsnak": {
              "snaktype": "value",
              "property": "P2723",
              "datavalue": { "value": "424242", "type": "string" },
              "datatype": "external-id"
            },
            "type": "statement",
            "rank": "normal"
          }
        ],
        "P11354": [
          {
            "mainsnak": { "snaktype": "somevalue", "property": "P11354", "datatype": "external-id" },
            "type": "statement",
            "rank": "normal"
          }
        ]
      }
    }
  }
}"#;

fn group(catalog: Catalog, key: &str) -> CatalogPage {
    CatalogPage::Group {
        catalog,
        key: key.to_string(),
    }
}

/// An item's own page, then one page per identifier it states for a catalog
/// bae knows. Each page describes an album, without claiming a pressing.
/// A claim bae's catalogs have no property for, one whose value is an object
/// rather than an identifier, and
/// one stating no value each name no page.
#[test]
fn an_item_names_one_page_per_catalog_it_identifies() {
    let entity = parse_entity(ENTITY_DOCUMENT).expect("the entity document parses");

    assert_eq!(
        entity.catalog_pages(),
        vec![
            group(Catalog::Wikidata, "Q424242"),
            group(Catalog::MusicBrainz, "mb-group"),
            group(Catalog::Discogs, "909090"),
            group(Catalog::AllMusic, "mw0000424242"),
            group(Catalog::RateYourMusic, "album/artist-name/album-title"),
            group(Catalog::Genius, "Artist-name/Album-title"),
            group(Catalog::Spotify, "4242424242424242424242"),
            group(Catalog::AppleMusic, "424242424"),
            group(Catalog::Deezer, "424242"),
        ]
    );
}

/// Every key an item states builds the address its catalog publishes, which is
/// what makes a record read off Wikidata reach the same page as one read off
/// MusicBrainz's own link.
#[test]
fn every_identified_page_builds_its_catalogs_address() {
    let entity = parse_entity(ENTITY_DOCUMENT).expect("the entity document parses");

    let addresses: Vec<String> = entity
        .catalog_pages()
        .into_iter()
        .map(|page| match page {
            CatalogPage::Release { catalog, key } => catalog.release_url(&key),
            CatalogPage::Group { catalog, key } => catalog.album_url(&key),
        })
        .collect();

    assert_eq!(
        addresses,
        vec![
            "https://www.wikidata.org/wiki/Q424242",
            "https://musicbrainz.org/release-group/mb-group",
            "https://www.discogs.com/master/909090",
            "https://www.allmusic.com/album/mw0000424242",
            "https://rateyourmusic.com/release/album/artist-name/album-title",
            "https://genius.com/albums/Artist-name/Album-title",
            "https://open.spotify.com/album/4242424242424242424242",
            "https://music.apple.com/album/424242424",
            "https://www.deezer.com/album/424242",
        ]
    );
}

/// An item states a property more than once when two of a catalog's pages
/// describe the same album — a reissue with its own streaming entry. Each value
/// is a page; which one becomes the release's record is settled where the
/// records are folded together.
#[test]
fn a_property_stated_twice_names_both_pages() {
    let document = serde_json::json!({
        "entities": {
            "Q424242": {
                "claims": {
                    "P2205": [
                        { "mainsnak": { "datavalue": { "value": "4242424242424242424242" } } },
                        { "mainsnak": { "datavalue": { "value": "9090909090909090909090" } } }
                    ]
                }
            }
        }
    })
    .to_string();

    let entity = parse_entity(&document).expect("the entity document parses");

    assert_eq!(
        entity.catalog_pages(),
        vec![
            group(Catalog::Wikidata, "Q424242"),
            group(Catalog::Spotify, "4242424242424242424242"),
            group(Catalog::Spotify, "9090909090909090909090"),
        ]
    );
}

/// The item the document names is the item the pages are read for, so a
/// document naming none is not an entity document.
#[test]
fn a_document_naming_no_item_does_not_parse() {
    parse_entity(r#"{ "entities": {} }"#).expect_err("a document with no item is not an entity");
}

/// Only a failure a retry could fix is retried. `NotFound` is Wikidata's answer
/// about an item — the ordinary one for an id an editor typed wrong — and a
/// parse failure returns the same verdict however many times the same bytes are
/// read.
#[test]
fn only_transient_wikidata_failures_are_retried() {
    let provider = |status: Option<u16>, told_wait: Option<Duration>| {
        repeat(&WikidataError::Provider { status, told_wait })
    };
    assert_eq!(repeat(&WikidataError::Timeout), Repeat::AfterBackoff);
    assert_eq!(
        repeat(&WikidataError::Network("refused".into())),
        Repeat::AfterBackoff
    );
    assert_eq!(provider(None, None), Repeat::AfterBackoff);
    assert_eq!(provider(Some(503), None), Repeat::AfterBackoff);
    assert_eq!(
        provider(Some(429), Some(Duration::from_secs(5))),
        Repeat::AfterToldWait(Duration::from_secs(5)),
        "a stated wait replaces the backoff"
    );

    assert_eq!(
        repeat(&WikidataError::NotFound("Q424242".into())),
        Repeat::Never
    );
    assert_eq!(provider(Some(404), None), Repeat::Never);
    assert_eq!(
        provider(Some(400), Some(Duration::from_secs(5))),
        Repeat::Never
    );
    assert_eq!(
        repeat(&WikidataError::Other("Failed to parse JSON".into())),
        Repeat::Never
    );
}
