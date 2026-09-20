use super::*;
use std::str::FromStr;

#[test]
fn every_catalog_round_trips_its_stored_string() {
    for catalog in Catalog::ALL {
        assert_eq!(
            Catalog::from_str(catalog.as_str()),
            Ok(catalog),
            "{} reads back as itself",
            catalog.as_str()
        );
    }
}

#[test]
fn stored_strings_and_names_are_distinct() {
    let stored: std::collections::BTreeSet<&str> =
        Catalog::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(stored.len(), Catalog::ALL.len());
    let names: std::collections::BTreeSet<&str> =
        Catalog::ALL.iter().map(|c| c.display_name()).collect();
    assert_eq!(names.len(), Catalog::ALL.len());
}

#[test]
fn an_unknown_stored_string_is_an_error() {
    assert!(Catalog::from_str("bandcamp_but_not").is_err());
}

#[test]
fn the_two_asked_catalogs_lead_the_list() {
    assert_eq!(&Catalog::ALL[..2], &Catalog::LOOKUP[..]);
}

#[test]
fn only_the_asked_catalogs_group_their_releases() {
    for catalog in Catalog::ALL {
        let grouped = catalog.group_url("g").is_some();
        assert_eq!(
            grouped,
            Catalog::LOOKUP.contains(&catalog),
            "{} groups: {grouped}",
            catalog.as_str()
        );
    }
}

/// Every address a record links out to is rebuilt from the catalog and the key
/// alone, so the key a relation is read into has to be the one the address is
/// rebuilt from.
#[test]
fn a_parsed_page_rebuilds_the_address_it_was_read_from() {
    for (url, catalog, key) in [
        (
            "https://musicbrainz.org/release/11111111-2222-3333-4444-555555555555",
            Catalog::MusicBrainz,
            "11111111-2222-3333-4444-555555555555",
        ),
        (
            "https://www.discogs.com/release/4242",
            Catalog::Discogs,
            "4242",
        ),
        (
            "https://www.wikidata.org/wiki/Q424242",
            Catalog::Wikidata,
            "Q424242",
        ),
        (
            "https://www.allmusic.com/album/mw0000424242",
            Catalog::AllMusic,
            "mw0000424242",
        ),
        (
            "https://rateyourmusic.com/release/album/artist-name/album-title",
            Catalog::RateYourMusic,
            "album/artist-name/album-title",
        ),
        (
            "https://genius.com/albums/Artist-name/Album-title",
            Catalog::Genius,
            "Artist-name/Album-title",
        ),
        (
            "https://www.musik-sammler.de/album/424242",
            Catalog::MusikSammler,
            "424242",
        ),
        (
            "https://artist-name.bandcamp.com/album/album-title",
            Catalog::Bandcamp,
            "artist-name.bandcamp.com/album/album-title",
        ),
        (
            "https://open.spotify.com/album/4242424242424242424242",
            Catalog::Spotify,
            "4242424242424242424242",
        ),
        (
            "https://www.deezer.com/album/424242",
            Catalog::Deezer,
            "424242",
        ),
    ] {
        let page = parse_catalog_url(url).unwrap_or_else(|| panic!("{url} names a catalog page"));
        let expected = if Catalog::LOOKUP.contains(&catalog) {
            CatalogPage::Release {
                catalog,
                key: key.to_string(),
            }
        } else {
            CatalogPage::Group {
                catalog,
                key: key.to_string(),
            }
        };
        assert_eq!(page, expected);
        let rebuilt = match page {
            CatalogPage::Release { catalog, key } => catalog.release_url(&key),
            CatalogPage::Group { catalog, key } => catalog.album_url(&key),
        };
        assert_eq!(rebuilt, url, "{url} is rebuilt from its key");
    }
}

/// A relation states whatever address an editor typed: the storefront country,
/// the slug after a Discogs id, a trailing slash, a share query, plain http.
/// All of them name the same page.
#[test]
fn an_address_is_read_past_what_does_not_name_the_page() {
    for (url, expected) in [
        (
            "http://www.discogs.com/release/4242-Artist-Name-Album-Title/",
            CatalogPage::Release {
                catalog: Catalog::Discogs,
                key: "4242".to_string(),
            },
        ),
        (
            "https://www.discogs.com/master/909090-Album-Title",
            CatalogPage::Group {
                catalog: Catalog::Discogs,
                key: "909090".to_string(),
            },
        ),
        (
            "https://musicbrainz.org/release-group/99999999-2222-3333-4444-555555555555",
            CatalogPage::Group {
                catalog: Catalog::MusicBrainz,
                key: "99999999-2222-3333-4444-555555555555".to_string(),
            },
        ),
        (
            "https://music.apple.com/us/album/album-title/424242?i=989898",
            CatalogPage::Group {
                catalog: Catalog::AppleMusic,
                key: "424242".to_string(),
            },
        ),
        (
            "https://www.deezer.com/en/album/424242",
            CatalogPage::Group {
                catalog: Catalog::Deezer,
                key: "424242".to_string(),
            },
        ),
    ] {
        assert_eq!(parse_catalog_url(url).as_ref(), Some(&expected), "{url}");
    }
}

#[test]
fn an_address_no_catalog_publishes_names_nothing() {
    for url in [
        "https://example.com/album/424242",
        "https://www.discogs.com/artist/4242",
        "https://en.wikipedia.org/wiki/Album_Title",
        "ftp://musicbrainz.org/release/4242",
        "https://rateyourmusic.com/release",
    ] {
        assert_eq!(parse_catalog_url(url), None, "{url}");
    }
}
