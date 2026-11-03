//! The album a list's offered pressings are of, which a folder is linked to
//! when the person cannot tell which of them their copy is.
//!
//! A list offers several pressings of one album when its offered rows — the
//! ones above the set-aside disclosure — are at least two, all on one album's
//! card, and every lookup catalog that files them files them under one album:
//! one MusicBrainz release group, one Discogs master. The link names those
//! albums and no pressing; the copy may be one no catalog lists.

use crate::release_group::ReleaseGroup;
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::{LookupRef, MetadataRef, ReleaseRecord};

/// The album a candidate is linked to while its pressing is unknown: one
/// album per catalog that files it — a MusicBrainz release group, a Discogs
/// master — in catalog order, at least one.
///
/// It claims no pressing, not even one of the pressings it was chosen among:
/// the folder's copy may be one no catalog lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumLink {
    albums: Vec<MetadataRef>,
}

impl AlbumLink {
    /// The link naming `albums`, put in catalog order. `None` for no album,
    /// or for two albums of one catalog: a link names the album once per
    /// catalog.
    pub fn new(albums: impl IntoIterator<Item = MetadataRef>) -> Option<Self> {
        let mut albums: Vec<MetadataRef> = albums.into_iter().collect();
        albums
            .sort_by_key(|album| Catalog::ALL.iter().position(|catalog| *catalog == album.catalog));
        let repeated = albums.windows(2).any(|pair| pair[0].catalog == pair[1].catalog);
        (!albums.is_empty() && !repeated).then_some(Self { albums })
    }

    /// The album in each catalog, in catalog order.
    pub fn albums(&self) -> &[MetadataRef] {
        &self.albums
    }

    /// Every catalog's record of the album, in catalog order: what an import
    /// of a candidate linked to it stores.
    pub fn records(&self) -> Vec<ReleaseRecord> {
        self.albums.iter().map(ReleaseRecord::album).collect()
    }
}

/// The offered rows of a list that are several pressings of one album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedAlbum {
    /// Each offered row's lead — the release picking the row reads — in the
    /// list's order, most agreed-with first.
    pub leads: Vec<LookupRef>,
    /// The album the rows are pressings of.
    pub link: AlbumLink,
}

impl SharedAlbum {
    /// The album a list's cards offer several pressings of; `None` where it
    /// offers fewer than two rows, rows of two cards, or rows a catalog files
    /// under two albums, and where no catalog names their album.
    pub fn of(groups: &[ReleaseGroup]) -> Option<Self> {
        let mut offering = groups.iter().filter(|group| group.pressings().next().is_some());
        let card = offering.next()?;
        if offering.next().is_some() {
            return None;
        }
        let rows: Vec<_> = card.pressings().collect();
        if rows.len() < 2 {
            return None;
        }
        let mut albums = Vec::new();
        for catalog in LookupCatalog::ALL {
            let mut named = rows
                .iter()
                .flat_map(|row| &row.releases)
                .filter(|release| release.source == catalog)
                .filter_map(|release| release.source_group_id.as_deref());
            let Some(album) = named.next() else {
                continue;
            };
            if named.any(|other| other != album) {
                return None;
            }
            albums.push(MetadataRef::new(catalog.catalog(), album));
        }
        Some(Self {
            leads: rows.iter().map(|row| row.lead().release()).collect(),
            link: AlbumLink::new(albums)?,
        })
    }
}

#[cfg(test)]
#[path = "shared_album_tests.rs"]
mod tests;
