//! What a MusicBrainz album is on Discogs, as the catalogs state it.
//!
//! A MusicBrainz release group and a Discogs master go on one card only when
//! a statement says they are the same album — never because the catalogs
//! spell its title alike. Five statements say so, and one rule set reads them
//! wherever a list comes from:
//!
//! 1. The release group's page, or the release's own, links the master.
//! 2. That page links a Wikidata item, and the item states the master's id.
//! 3. The release links a Discogs release as the same release, and that
//!    release's own document files it under the master.
//! 4. One of the group's releases on the list and a release of the master on
//!    the list print the same barcode, and their titles share a word.
//! 5. They print the same catalog number under the same label, and their
//!    titles share a word.
//!
//! The first three are what a MusicBrainz release's documents state, read
//! when the release is fetched
//! (`ReleasePayloads::album_statements`) and
//! stored with it. A group is every album its read releases' documents name;
//! failing that, unknown where one of those documents could not be had;
//! failing that, what the list's releases print, the last two (the `on_list`
//! module says how). See `read_groups`.
//!
//! Only what is read, and when, differs by caller: an identify run reads the
//! document of every row it offers before it settles, and a typed search
//! reads a result's when the person picks it. What was read is carried on
//! each record, so a list read back from the store groups as it did when it
//! was found.

use crate::catalog::{Catalog, LookupCatalog};
use crate::records::MetadataRef;
use crate::search::MetadataResult;

/// What a record's catalog says its album is on the other lookup catalog.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AlbumLinks {
    /// Never read: no document of its album's releases on the list was
    /// read, or the record is a Discogs one, whose documents name no
    /// counterpart.
    NotAsked,
    /// Read: the other catalog's albums a statement names, in the order the
    /// statements were read, strongest first. None when nothing states one.
    Read(Vec<MetadataRef>),
    /// A document the reading needed could not be had and no other
    /// statement named an album, so whether one is stated is not known.
    Unread,
}

impl AlbumLinks {
    /// What was read — nothing unless the album's links were read.
    pub fn read(&self) -> &[MetadataRef] {
        match self {
            AlbumLinks::Read(albums) => albums,
            AlbumLinks::NotAsked | AlbumLinks::Unread => &[],
        }
    }

    /// Whether a statement read about this album names `album` as the same.
    pub fn names(&self, album: &MetadataRef) -> bool {
        self.read().contains(album)
    }
}

/// One album a reading of a MusicBrainz release group found it to be on
/// another catalog, kept beyond the list that read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupAlbum {
    pub group: String,
    pub album: MetadataRef,
}

impl GroupAlbum {
    /// The album this one says `album` is, when it names `album` on either
    /// side.
    pub fn other_than(&self, album: &MetadataRef) -> Option<MetadataRef> {
        let group = MetadataRef::new(Catalog::MusicBrainz, self.group.clone());
        if *album == group {
            Some(self.album.clone())
        } else if *album == self.album {
            Some(group)
        } else {
            None
        }
    }
}

/// What one MusicBrainz release group on a list was read to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupLinks {
    pub group: String,
    pub links: AlbumLinks,
}

/// What each MusicBrainz group on `list` is on the other lookup catalog, in
/// first-seen order: every group one of whose releases on the list had its
/// documents read. `stated` answers what a MusicBrainz release's documents
/// state its album is — `Unread` where they could not be had — and `None`
/// where nothing read them.
///
/// A group is every album its read releases' documents name; failing that,
/// `Unread` where one of them could not be had, since a link it holds would
/// come first; failing that, what `list`'s releases print.
pub fn read_groups(
    list: &[&MetadataResult],
    stated: impl Fn(&str) -> Option<AlbumLinks>,
) -> Vec<GroupLinks> {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for result in list.iter().filter(|result| result.source == LookupCatalog::MusicBrainz) {
        let Some(group) = result.source_group_id.as_deref() else {
            continue;
        };
        let release = result.release_id.as_str();
        match groups.iter_mut().find(|(listed, _)| *listed == group) {
            Some((_, releases)) if releases.contains(&release) => {}
            Some((_, releases)) => releases.push(release),
            None => groups.push((group, vec![release])),
        }
    }
    groups
        .into_iter()
        .filter_map(|(group, releases)| {
            let read: Vec<AlbumLinks> = releases.into_iter().filter_map(&stated).collect();
            if read.is_empty() {
                return None;
            }
            let mut found = Found::default();
            for links in read {
                match links {
                    AlbumLinks::Read(albums) => {
                        for album in albums {
                            found.push(album);
                        }
                    }
                    AlbumLinks::Unread => found.unread = true,
                    AlbumLinks::NotAsked => {}
                }
            }
            let links = if found.albums.is_empty() && !found.unread {
                AlbumLinks::Read(on_list::albums(group, list))
            } else {
                found.settle()
            };
            Some(GroupLinks { group: group.to_string(), links })
        })
        .collect()
}

/// Put what each group was read to be onto the MusicBrainz records of it. A
/// record of a group nothing was read about keeps what it had.
pub fn apply(result: &mut MetadataResult, groups: &[GroupLinks]) {
    if result.source != LookupCatalog::MusicBrainz {
        return;
    }
    let Some(group) = &result.source_group_id else {
        return;
    };
    if let Some(read) = groups.iter().find(|read| read.group == *group) {
        result.album_links = read.links.clone();
    }
}

/// What these groups of `list` were read to be, to keep beyond it — the
/// albums each names: each group that names an album, and — where `list` holds another catalog's album its
/// releases were compared against — each that names none, which takes away
/// what an earlier reading kept. A group found to name nothing with nothing
/// on the list to compare it against keeps what it had, and so does one whose
/// reading is not known.
pub fn to_keep(groups: &[GroupLinks], list: &[&MetadataResult]) -> Vec<(String, Vec<MetadataRef>)> {
    let compared = list.iter().any(|result| {
        names_other_album(result.source.catalog()) && result.source_group_id.is_some()
    });
    groups
        .iter()
        .filter_map(|read| match &read.links {
            AlbumLinks::Read(albums) if !albums.is_empty() || compared => {
                Some((read.group.clone(), albums.clone()))
            }
            AlbumLinks::Read(_) | AlbumLinks::NotAsked | AlbumLinks::Unread => None,
        })
        .collect()
}

/// The albums statements named so far, each once, and whether a document
/// one was asked of could not be had.
#[derive(Default)]
pub(crate) struct Found {
    pub(crate) albums: Vec<MetadataRef>,
    pub(crate) unread: bool,
}

impl Found {
    pub(crate) fn push(&mut self, album: MetadataRef) {
        if !self.albums.contains(&album) {
            self.albums.push(album);
        }
    }

    /// What was read: the albums named, or — where none was named — that
    /// nothing is stated, unless something asked for could not be had.
    pub(crate) fn settle(self) -> AlbumLinks {
        if self.albums.is_empty() && self.unread {
            AlbumLinks::Unread
        } else {
            AlbumLinks::Read(self.albums)
        }
    }
}

/// Whether a page of `catalog` names an album this reading joins: one of
/// another lookup catalog's.
pub(crate) fn names_other_album(catalog: Catalog) -> bool {
    catalog.lookup().is_some_and(|lookup| lookup != LookupCatalog::MusicBrainz)
}

#[path = "album_links/on_list.rs"]
mod on_list;

#[cfg(test)]
#[path = "album_links_tests.rs"]
mod tests;
