//! The catalogs that describe releases, and the pages they publish.
//!
//! Two of them answer questions — a search, a disc ID, a barcode — and the
//! rest are reached only by following a link one of those two states. Both
//! kinds publish metadata, but an album page does not name a particular
//! pressing. [`Catalog::LOOKUP`] names the catalogs the application can query.

use serde::{Deserialize, Serialize};

/// A service that publishes descriptions of releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Catalog {
    MusicBrainz,
    Discogs,
    AllMusic,
    AppleMusic,
    Bandcamp,
    Deezer,
    Genius,
    MusikSammler,
    RateYourMusic,
    Spotify,
    Wikidata,
}

impl Catalog {
    /// Every catalog, in the order surfaces list them: the two that answer
    /// questions first, then the rest by name.
    pub const ALL: [Catalog; 11] = [
        Self::MusicBrainz,
        Self::Discogs,
        Self::AllMusic,
        Self::AppleMusic,
        Self::Bandcamp,
        Self::Deezer,
        Self::Genius,
        Self::MusikSammler,
        Self::RateYourMusic,
        Self::Spotify,
        Self::Wikidata,
    ];

    /// The catalogs that answer a search, a disc ID, or a barcode, in the order
    /// surfaces list them and runs ask them. The one list behind "an entry per
    /// source": a preference, an availability, a typed search's per-source
    /// part, a ledger column. Neither is the main one.
    ///
    /// The others hold pages a record links out to; nothing asks them
    /// anything, so nothing enumerates them as something to ask.
    pub const LOOKUP: [Catalog; 2] = [Self::MusicBrainz, Self::Discogs];

    /// The one catalog that answers a disc ID. Disc IDs are a MusicBrainz
    /// identifier, so no other catalog has an endpoint to ask; with this one
    /// not asked, a disc ID read off a LOG or CUE stands with nothing looked
    /// up against it.
    pub const DISC_ID_CATALOG: Catalog = Self::MusicBrainz;

    /// The stored `catalog` column value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::Discogs => "discogs",
            Self::AllMusic => "allmusic",
            Self::AppleMusic => "apple_music",
            Self::Bandcamp => "bandcamp",
            Self::Deezer => "deezer",
            Self::Genius => "genius",
            Self::MusikSammler => "musik_sammler",
            Self::RateYourMusic => "rateyourmusic",
            Self::Spotify => "spotify",
            Self::Wikidata => "wikidata",
        }
    }

    /// The catalog's own name. A proper noun, so it is the same in every
    /// language and needs no catalog entry.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "MusicBrainz",
            Self::Discogs => "Discogs",
            Self::AllMusic => "AllMusic",
            Self::AppleMusic => "Apple Music",
            Self::Bandcamp => "Bandcamp",
            Self::Deezer => "Deezer",
            Self::Genius => "Genius",
            Self::MusikSammler => "Musik-Sammler",
            Self::RateYourMusic => "Rate Your Music",
            Self::Spotify => "Spotify",
            Self::Wikidata => "Wikidata",
        }
    }

    /// The name of the service a cover image came from. MusicBrainz release
    /// covers are served by its sister project, the Cover Art Archive, so the
    /// cover label differs from [`Self::display_name`].
    ///
    /// Only the asked catalogs return artwork, so only they have one.
    pub fn cover_source_label(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "Cover Art Archive",
            Self::Discogs => "Discogs",
            other => unreachable!("{} returns no cover artwork", other.as_str()),
        }
    }

    /// The page this catalog publishes for one release, built from the key the
    /// record holds.
    pub fn release_url(&self, key: &str) -> String {
        format!("{}{key}", self.release_url_prefix())
    }

    /// The page this catalog publishes for the group a release belongs to — a
    /// release group on MusicBrainz, a master on Discogs. `None` for the
    /// catalogs that file releases without grouping them.
    pub fn group_url(&self, group_key: &str) -> Option<String> {
        Some(format!("{}{group_key}", self.group_url_prefix()?))
    }

    /// The page for an album, independent of any particular pressing.
    pub fn album_url(&self, key: &str) -> String {
        let prefix = match self.group_url_prefix() {
            Some(prefix) => prefix,
            None => self.release_url_prefix(),
        };
        format!("{prefix}{key}")
    }

    /// What a release key is appended to. A key is exactly the part of the
    /// page's address this prefix does not fix: an id where the catalog has
    /// one, the rest of the path where it does not.
    fn release_url_prefix(&self) -> &'static str {
        match self {
            Self::MusicBrainz => "https://musicbrainz.org/release/",
            Self::Discogs => "https://www.discogs.com/release/",
            Self::AllMusic => "https://www.allmusic.com/album/",
            Self::AppleMusic => "https://music.apple.com/album/",
            // Every Bandcamp page sits on its artist's own subdomain, so the
            // host is part of what names the release.
            Self::Bandcamp => "https://",
            Self::Deezer => "https://www.deezer.com/album/",
            Self::Genius => "https://genius.com/albums/",
            Self::MusikSammler => "https://www.musik-sammler.de/album/",
            Self::RateYourMusic => "https://rateyourmusic.com/release/",
            Self::Spotify => "https://open.spotify.com/album/",
            Self::Wikidata => "https://www.wikidata.org/wiki/",
        }
    }

    fn group_url_prefix(&self) -> Option<&'static str> {
        match self {
            Self::MusicBrainz => Some("https://musicbrainz.org/release-group/"),
            Self::Discogs => Some("https://www.discogs.com/master/"),
            _ => None,
        }
    }
}

impl std::str::FromStr for Catalog {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|catalog| catalog.as_str() == s)
            .ok_or_else(|| format!("unknown catalog: {s}"))
    }
}

impl std::fmt::Display for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which of a catalog's two kinds of page an address names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogPage {
    /// One release.
    Release { catalog: Catalog, key: String },
    /// An album, including a MusicBrainz release group or Discogs master.
    Group { catalog: Catalog, key: String },
}

/// The catalog page `url` names, or `None` when no catalog bae knows publishes
/// at that address.
///
/// This is the inverse of [`Catalog::release_url`] and [`Catalog::album_url`]:
/// the key it reads back is the one those two rebuild the address from.
pub fn parse_catalog_url(url: &str) -> Option<CatalogPage> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    // Neither the fragment nor the query names the page: Apple Music appends
    // `?i=` for a track within an album, and either can be a share tag.
    let rest = rest
        .split(['?', '#'])
        .next()
        .expect("split always yields a first part");
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, path.trim_end_matches('/')),
        None => (rest, ""),
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Bandcamp keys carry the artist's subdomain, so the host is matched by
    // suffix and kept.
    if host.ends_with(".bandcamp.com") {
        return match segments.as_slice() {
            ["album", slug] => Some(CatalogPage::Group {
                catalog: Catalog::Bandcamp,
                key: format!("{host}/album/{slug}"),
            }),
            _ => None,
        };
    }

    match (host.as_str(), segments.as_slice()) {
        ("musicbrainz.org", ["release", key]) => Some(CatalogPage::Release {
            catalog: Catalog::MusicBrainz,
            key: (*key).to_string(),
        }),
        ("musicbrainz.org", ["release-group", key]) => Some(CatalogPage::Group {
            catalog: Catalog::MusicBrainz,
            key: (*key).to_string(),
        }),
        // A Discogs address carries a slug after the id — `/release/42-Album-Title`.
        ("discogs.com", ["release", slug]) => {
            leading_digits(slug).map(|key| CatalogPage::Release {
                catalog: Catalog::Discogs,
                key,
            })
        }
        ("discogs.com", ["master", slug]) => leading_digits(slug).map(|key| CatalogPage::Group {
            catalog: Catalog::Discogs,
            key,
        }),
        ("wikidata.org", ["wiki", key]) if key.starts_with('Q') => Some(CatalogPage::Group {
            catalog: Catalog::Wikidata,
            key: (*key).to_string(),
        }),
        ("allmusic.com", ["album", key]) => Some(CatalogPage::Group {
            catalog: Catalog::AllMusic,
            key: (*key).to_string(),
        }),
        ("musik-sammler.de", ["album", key]) => Some(CatalogPage::Group {
            catalog: Catalog::MusikSammler,
            key: (*key).to_string(),
        }),
        ("open.spotify.com", ["album", key]) => Some(CatalogPage::Group {
            catalog: Catalog::Spotify,
            key: (*key).to_string(),
        }),
        // Apple Music and Deezer both prefix the storefront country onto the
        // path, and both leave it off the canonical address. Apple also puts a
        // slug ahead of the id.
        ("music.apple.com", ["album", key]) | ("music.apple.com", [.., "album", _, key]) => {
            Some(CatalogPage::Group {
                catalog: Catalog::AppleMusic,
                key: (*key).to_string(),
            })
        }
        ("deezer.com", [.., "album", key]) => Some(CatalogPage::Group {
            catalog: Catalog::Deezer,
            key: (*key).to_string(),
        }),
        // Neither of these names a release with an id; what identifies the page
        // is the rest of its path.
        ("rateyourmusic.com", ["release", rest @ ..]) if !rest.is_empty() => {
            Some(CatalogPage::Group {
                catalog: Catalog::RateYourMusic,
                key: rest.join("/"),
            })
        }
        ("genius.com", ["albums", rest @ ..]) if !rest.is_empty() => Some(CatalogPage::Group {
            catalog: Catalog::Genius,
            key: rest.join("/"),
        }),
        _ => None,
    }
}

/// The digits a Discogs path segment starts with, which is its id.
fn leading_digits(segment: &str) -> Option<String> {
    let digits: String = segment.chars().take_while(char::is_ascii_digit).collect();
    (!digits.is_empty()).then_some(digits)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
