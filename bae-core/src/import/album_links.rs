//! The links a catalog states between its albums and another catalog's.
//!
//! MusicBrainz's editors link a release group to the Discogs master that is
//! the same album, on the release group's own page. That link is what puts a
//! MusicBrainz album and a Discogs album on one card: it is stated rather than
//! guessed from how the two catalogs spell the album.
//!
//! Reading it costs a request per release group, and MusicBrainz answers about
//! one a second, so a list reads it only when it holds both catalogs'
//! releases — a link joins nothing otherwise. What was read is carried on each
//! record, so a list read back from the store groups as it did when it was
//! found.

use crate::import::search::MetadataResult;
use crate::import::types::{parse_catalog_url, Catalog, CatalogPage, MetadataRef};
use crate::musicbrainz::{self, MusicBrainz};
use crate::util::rate_limiter::CallPriority;
use tracing::warn;

/// What a record's catalog says its album is on the other lookup catalog.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AlbumLinks {
    /// Never read: the list the record came with held no other catalog's
    /// release to join, or the record is a Discogs one, whose documents name
    /// no counterpart.
    NotAsked,
    /// The album's own page, read: the other catalog's albums it names, which
    /// may be none.
    Read(Vec<MetadataRef>),
    /// The album's own page could not be read, so whether it names another
    /// catalog's album is not known.
    Unread,
}

impl AlbumLinks {
    /// The other catalog's albums this names — none unless the page was read.
    pub fn named(&self) -> &[MetadataRef] {
        match self {
            AlbumLinks::Read(albums) => albums,
            AlbumLinks::NotAsked | AlbumLinks::Unread => &[],
        }
    }
}

/// What reading one MusicBrainz release group's links answered.
pub type GroupLinks = (String, AlbumLinks);

/// The MusicBrainz release groups among `results` whose links are to be read:
/// every one not yet asked, in first-seen order, when the results hold both
/// catalogs' releases — and none otherwise. `asked` names a group whose links
/// were already read, or are being read.
pub(crate) fn groups_to_read<'a>(
    results: impl IntoIterator<Item = &'a MetadataResult>,
    asked: impl Fn(&str) -> bool,
) -> Vec<String> {
    let results: Vec<&MetadataResult> = results.into_iter().collect();
    let holds = |catalog: Catalog| results.iter().any(|result| result.source == catalog);
    if !(holds(Catalog::MusicBrainz) && holds(Catalog::Discogs)) {
        return Vec::new();
    }
    let mut groups: Vec<String> = Vec::new();
    for result in results {
        if result.source != Catalog::MusicBrainz
            || !matches!(result.album_links, AlbumLinks::NotAsked)
        {
            continue;
        }
        let Some(group_id) = &result.source_group_id else {
            continue;
        };
        if !asked(group_id) && !groups.contains(group_id) {
            groups.push(group_id.clone());
        }
    }
    groups
}

/// Put what was read about each group onto the MusicBrainz records of it.
/// A record of a group nothing was read about keeps what it had.
pub(crate) fn apply(result: &mut MetadataResult, read: &[GroupLinks]) {
    if result.source != Catalog::MusicBrainz {
        return;
    }
    let Some(group_id) = &result.source_group_id else {
        return;
    };
    if let Some((_, links)) = read.iter().find(|(group, _)| group == group_id) {
        result.album_links = links.clone();
    }
}

/// Read each group's links from its own page, one request per group through
/// the client's response cache — the same page a pick's documents archive.
pub(crate) async fn read(
    musicbrainz: &MusicBrainz,
    groups: &[String],
    priority: CallPriority,
) -> Vec<GroupLinks> {
    let mut read = Vec::with_capacity(groups.len());
    for group_id in groups {
        read.push((
            group_id.clone(),
            links_of_group(musicbrainz, group_id, priority).await,
        ));
    }
    read
}

async fn links_of_group(
    musicbrainz: &MusicBrainz,
    group_id: &str,
    priority: CallPriority,
) -> AlbumLinks {
    let json = match musicbrainz
        .fetch_release_group_json(group_id, priority)
        .await
    {
        Ok(json) => json,
        Err(error) => {
            warn!(
                musicbrainz_release_group_id = group_id,
                %error,
                "MusicBrainz release group page unread; its album links are not known"
            );
            return AlbumLinks::Unread;
        }
    };
    let group = musicbrainz::parse_release_group(&json)
        .expect("the client returns a release group page only once it parses");
    let mut albums: Vec<MetadataRef> = Vec::new();
    for page in musicbrainz::relation_urls(&group.relations).filter_map(parse_catalog_url) {
        let CatalogPage::Group { catalog, key } = page else {
            continue;
        };
        let album = MetadataRef::new(catalog, key);
        if catalog != Catalog::MusicBrainz
            && Catalog::LOOKUP.contains(&catalog)
            && !albums.contains(&album)
        {
            albums.push(album);
        }
    }
    AlbumLinks::Read(albums)
}

#[cfg(test)]
#[path = "album_links_tests.rs"]
mod tests;
