//! What a MusicBrainz album is on Discogs, as the catalogs state it.
//!
//! A MusicBrainz release group and a Discogs master go on one card only when
//! a document states they are the same album — never because the catalogs
//! spell it alike. Three statements say so, read in this order, and the first
//! that names a Discogs album is the one taken:
//!
//! 1. The release group's own page links the master.
//! 2. The group's page links a Wikidata item, and the item states the
//!    master's id.
//! 3. One of the group's releases links a Discogs release as the same
//!    release, and that release's own document files it under the master.
//!
//! The first two state the album itself and cost no Discogs request. The
//! third goes through one pressing — MusicBrainz says its release is that
//! Discogs release, and Discogs says that release is in the master — and
//! costs one request, for the Discogs release. Where the MusicBrainz release
//! is on the list, the Discogs release it names goes onto the list beside it
//! as its twin, so the row carries both catalogs' records and picking it
//! claims both. A twin was returned by no lookup; its record says which
//! release named it.
//!
//! One MusicBrainz request reads a group's own links and its releases' links
//! together: its releases browsed, each carrying the group's relations beside
//! its own. The browse answers a page of a hundred releases; a release past
//! them is read from its own record on the list, which carries its links when
//! the lookup that returned it asked for them.
//!
//! MusicBrainz answers about one request a second, so a list reads these only
//! when it holds both catalogs' releases — a link joins nothing otherwise.
//! What was read is carried on each record, so a list read back from the
//! store groups as it did when it was found.

use crate::db::LibraryStatus;
use crate::discogs::client::DiscogsClient;
use crate::import::search::{discogs_release_to_metadata, release_links_of, MetadataResult};
use crate::import::types::{parse_catalog_url, Catalog, CatalogPage, MetadataRef};
use crate::musicbrainz::{self, MusicBrainz};
use crate::util::rate_limiter::CallPriority;
use crate::wikidata::Wikidata;
use tracing::warn;

/// What a record's catalog says its album is on the other lookup catalog.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AlbumLinks {
    /// Never read: the list the record came with held no other catalog's
    /// release to join, or the record is a Discogs one, whose documents name
    /// no counterpart.
    NotAsked,
    /// Read: the other catalog's albums a statement names, each with the
    /// statement that names it. None when nothing states one.
    Read(Vec<AlbumLink>),
    /// A document the reading needed could not be had and no other
    /// statement named an album, so whether one is stated is not known.
    Unread,
}

impl AlbumLinks {
    /// What was read — nothing unless the album's links were read.
    pub fn read(&self) -> &[AlbumLink] {
        match self {
            AlbumLinks::Read(links) => links,
            AlbumLinks::NotAsked | AlbumLinks::Unread => &[],
        }
    }

    /// Whether a statement read about this album names `album` as the same.
    pub fn names(&self, album: &MetadataRef) -> bool {
        self.read().iter().any(|link| link.album == *album)
    }
}

/// One album on another catalog that a statement names as this one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AlbumLink {
    pub album: MetadataRef,
    pub stated: AlbumStatement,
}

/// Which statement names the album.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AlbumStatement {
    /// The release group's own page links it.
    Page,
    /// The group's page links this Wikidata item, which states it.
    Wikidata { item: String },
    /// `musicbrainz_release`, one of the group's releases, links `twin` as
    /// the same release, and `twin`'s own document files it under the album.
    Release {
        musicbrainz_release: String,
        twin: MetadataRef,
    },
}

/// One statement read about a MusicBrainz release group, kept beyond the list
/// that read it: `link.album` is the group's album on another catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupStatement {
    pub group: String,
    pub link: AlbumLink,
}

impl GroupStatement {
    /// The album this statement says `album` is, when it names `album` on
    /// either side.
    pub fn other_than(&self, album: &MetadataRef) -> Option<MetadataRef> {
        let group = MetadataRef::new(Catalog::MusicBrainz, self.group.clone());
        if *album == group {
            Some(self.link.album.clone())
        } else if *album == self.link.album {
            Some(group)
        } else {
            None
        }
    }
}

/// What reading a list's albums takes from the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToRead {
    /// The MusicBrainz groups to read, in first-seen order.
    pub groups: Vec<GroupToRead>,
    /// The other catalog's releases already on the list, each with the album
    /// its catalog files it under. A release link naming one of them is read
    /// off the list rather than asked for.
    pub on_list: Vec<(MetadataRef, Option<String>)>,
}

impl ToRead {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The groups this reads.
    pub fn group_ids(&self) -> impl Iterator<Item = &str> {
        self.groups.iter().map(|group| group.group.as_str())
    }
}

/// One MusicBrainz group to read, with its releases on the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupToRead {
    pub group: String,
    /// The group's releases on the list, in list order, each with the other
    /// catalogs' releases its record names as the same release.
    pub releases: Vec<(String, Vec<MetadataRef>)>,
}

/// What reading one MusicBrainz group answered. `Status` is what the twin
/// carries about the library: nothing as the catalogs answer, its library
/// status once the library was asked.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupReading<Status = LibraryStatus> {
    pub group: String,
    pub links: AlbumLinks,
    /// The group's releases on the list whose own links the browse read,
    /// each with the other catalogs' releases it names. A record returned
    /// by a search states none, because the search reads no links; this is
    /// what its document says.
    pub release_links: Vec<(String, Vec<MetadataRef>)>,
    /// The Discogs release a release-link statement read, when the release
    /// that names it is on the list and it is not.
    pub twin: Option<Twin<Status>>,
}

/// A release no lookup returned, on the list because a release there names
/// it as itself and it was read to learn its album.
#[derive(Debug, Clone, PartialEq)]
pub struct Twin<Status = LibraryStatus> {
    pub result: MetadataResult,
    /// The MusicBrainz release on the list whose own document names it.
    pub named_by: MetadataRef,
    pub status: Status,
}

#[cfg(test)]
impl GroupReading {
    /// A reading that answered `links` and nothing else: no release's links
    /// read, and no twin.
    pub(crate) fn of_links(group: &str, links: AlbumLinks) -> Self {
        Self {
            group: group.to_string(),
            links,
            release_links: Vec::new(),
            twin: None,
        }
    }
}

impl<Status> GroupReading<Status> {
    /// The same reading, its twin carrying `status` instead.
    pub(crate) fn with_status<Next>(
        self,
        status: impl FnOnce(&MetadataResult) -> Next,
    ) -> GroupReading<Next> {
        GroupReading {
            group: self.group,
            links: self.links,
            release_links: self.release_links,
            twin: self.twin.map(|twin| Twin {
                status: status(&twin.result),
                result: twin.result,
                named_by: twin.named_by,
            }),
        }
    }
}

/// The MusicBrainz groups among `results` whose links are to be read — every
/// one not yet asked, in first-seen order, when the results hold both
/// catalogs' releases, and none otherwise — with what the reading takes from
/// the list. `asked` names a group whose links were already read, or are
/// being read.
pub(crate) fn to_read<'a>(
    results: impl IntoIterator<Item = &'a MetadataResult>,
    asked: impl Fn(&str) -> bool,
) -> ToRead {
    let results: Vec<&MetadataResult> = results.into_iter().collect();
    let holds = |catalog: Catalog| results.iter().any(|result| result.source == catalog);
    if !(holds(Catalog::MusicBrainz) && holds(Catalog::Discogs)) {
        return ToRead::default();
    }
    let mut groups: Vec<GroupToRead> = Vec::new();
    let mut on_list: Vec<(MetadataRef, Option<String>)> = Vec::new();
    for result in &results {
        if result.source != Catalog::MusicBrainz {
            let release = MetadataRef::new(result.source, result.release_id.clone());
            if !on_list.iter().any(|(listed, _)| *listed == release) {
                on_list.push((release, result.source_group_id.clone()));
            }
            continue;
        }
        if !matches!(result.album_links, AlbumLinks::NotAsked) {
            continue;
        }
        let Some(group_id) = &result.source_group_id else {
            continue;
        };
        if asked(group_id) {
            continue;
        }
        let at = match groups.iter().position(|group| group.group == *group_id) {
            Some(at) => at,
            None => {
                groups.push(GroupToRead {
                    group: group_id.clone(),
                    releases: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let releases = &mut groups[at].releases;
        if !releases.iter().any(|(id, _)| *id == result.release_id) {
            releases.push((result.release_id.clone(), result.links.clone()));
        }
    }
    ToRead { groups, on_list }
}

/// Put what was read about each group onto the MusicBrainz records of it:
/// the album's links, and the release's own links where the browse read
/// them. A record of a group nothing was read about keeps what it had.
pub(crate) fn apply<Status>(result: &mut MetadataResult, read: &[GroupReading<Status>]) {
    if result.source != Catalog::MusicBrainz {
        return;
    }
    let Some(group_id) = &result.source_group_id else {
        return;
    };
    let Some(reading) = read.iter().find(|reading| reading.group == *group_id) else {
        return;
    };
    result.album_links = reading.links.clone();
    if let Some((_, links)) = reading
        .release_links
        .iter()
        .find(|(release, _)| *release == result.release_id)
    {
        result.links = links.clone();
    }
}

/// The twins these readings read that go on a list of `results`.
pub(crate) fn twins<'a, Status>(
    read: &'a [GroupReading<Status>],
    results: &[&MetadataResult],
) -> Vec<&'a Twin<Status>> {
    beside(read.iter().filter_map(|reading| reading.twin.as_ref()), results)
}

/// The twins that go on a list of `results`: each one beside the release that
/// names it, and none the list already holds.
pub(crate) fn beside<'a, Status: 'a>(
    candidates: impl IntoIterator<Item = &'a Twin<Status>>,
    results: &[&MetadataResult],
) -> Vec<&'a Twin<Status>> {
    let holds = |catalog: Catalog, key: &str| {
        results
            .iter()
            .any(|result| result.source == catalog && result.release_id == key)
    };
    let mut twins: Vec<&Twin<Status>> = Vec::new();
    for twin in candidates {
        let listed = holds(twin.result.source, &twin.result.release_id)
            || twins.iter().any(|other| {
                other.result.source == twin.result.source
                    && other.result.release_id == twin.result.release_id
            });
        if holds(twin.named_by.catalog, &twin.named_by.key) && !listed {
            twins.push(twin);
        }
    }
    twins
}

/// The clients a reading asks.
pub(crate) struct Readers<'a> {
    musicbrainz: &'a MusicBrainz,
    wikidata: &'a Wikidata,
    /// `None` when this library holds no Discogs key: a release link is then
    /// followed only to a release already on the list.
    discogs: Option<&'a DiscogsClient>,
}

impl<'a> Readers<'a> {
    pub(crate) fn new(
        musicbrainz: &'a MusicBrainz,
        wikidata: &'a Wikidata,
        discogs: Option<&'a DiscogsClient>,
    ) -> Self {
        Self {
            musicbrainz,
            wikidata,
            discogs,
        }
    }
}

/// Read each group's links, one group after another.
pub(crate) async fn read(
    readers: &Readers<'_>,
    to_read: &ToRead,
    priority: CallPriority,
) -> Vec<GroupReading<()>> {
    let mut read = Vec::with_capacity(to_read.groups.len());
    for group in &to_read.groups {
        read.push(read_group(readers, group, &to_read.on_list, priority).await);
    }
    read
}

/// The statements read so far, and whether one that was asked for could not
/// be had.
#[derive(Default)]
struct Found {
    links: Vec<AlbumLink>,
    unread: bool,
}

impl Found {
    fn push(&mut self, album: MetadataRef, stated: AlbumStatement) {
        if !self.links.iter().any(|link| link.album == album) {
            self.links.push(AlbumLink { album, stated });
        }
    }

    /// What was read: the albums named, or — where none was named — that
    /// nothing is stated, unless something asked for could not be had.
    fn settle(self) -> AlbumLinks {
        if self.links.is_empty() && self.unread {
            AlbumLinks::Unread
        } else {
            AlbumLinks::Read(self.links)
        }
    }
}

async fn read_group(
    readers: &Readers<'_>,
    group: &GroupToRead,
    on_list: &[(MetadataRef, Option<String>)],
    priority: CallPriority,
) -> GroupReading<()> {
    let unread = || GroupReading {
        group: group.group.clone(),
        links: AlbumLinks::Unread,
        release_links: Vec::new(),
        twin: None,
    };
    let browsed = match readers
        .musicbrainz
        .browse_group_releases(&group.group, priority)
        .await
    {
        Ok(browsed) => browsed,
        Err(error) => {
            warn!(
                musicbrainz_release_group_id = group.group,
                %error,
                "MusicBrainz release group unread; its album links are not known"
            );
            return unread();
        }
    };
    // Every browsed release carries its group's relations; the first one
    // that is this group's states them.
    let Some(page) = browsed.releases.iter().find_map(|release| {
        release
            .release_group
            .as_ref()
            .filter(|embedded| embedded.id == group.group)
            .and_then(|embedded| embedded.relations.as_ref())
    }) else {
        warn!(
            musicbrainz_release_group_id = group.group,
            "MusicBrainz browsed no release stating its group's links; its album links are not known"
        );
        return unread();
    };
    let pages: Vec<CatalogPage> = musicbrainz::relation_urls(page)
        .filter_map(parse_catalog_url)
        .collect();
    let release_links: Vec<(String, Vec<MetadataRef>)> = group
        .releases
        .iter()
        .filter_map(|(release, _)| {
            browsed
                .releases
                .iter()
                .find(|browsed| browsed.id == *release)
                .map(|browsed| (release.clone(), release_links_of(&browsed.relations)))
        })
        .collect();
    let reading = |links: AlbumLinks, twin: Option<Twin<()>>| GroupReading {
        group: group.group.clone(),
        links,
        release_links: release_links.clone(),
        twin,
    };

    let mut found = Found::default();
    for page in &pages {
        if let CatalogPage::Group { catalog, key } = page {
            if names_other_album(*catalog) {
                found.push(MetadataRef::new(*catalog, key.clone()), AlbumStatement::Page);
            }
        }
    }
    if !found.links.is_empty() {
        return reading(found.settle(), None);
    }

    for item in wikidata_items(&pages) {
        match readers.wikidata.fetch_entity(&item, priority).await {
            Ok(json) => {
                let entity = crate::wikidata::parse_entity(&json)
                    .expect("the client returns an entity document only once it parses");
                for page in entity.catalog_pages() {
                    if let CatalogPage::Group { catalog, key } = page {
                        if names_other_album(catalog) {
                            found.push(
                                MetadataRef::new(catalog, key),
                                AlbumStatement::Wikidata { item: item.clone() },
                            );
                        }
                    }
                }
            }
            Err(error) => {
                warn!(
                    musicbrainz_release_group_id = group.group,
                    wikidata_item = item,
                    %error,
                    "Wikidata item unread; the album it states is not known"
                );
                found.unread = true;
            }
        }
    }
    if !found.links.is_empty() {
        return reading(found.settle(), None);
    }

    let Some(through) = release_to_follow(group, &release_links, &browsed.releases, on_list)
    else {
        return reading(found.settle(), None);
    };
    let stated = AlbumStatement::Release {
        musicbrainz_release: through.release.clone(),
        twin: through.twin.clone(),
    };
    if let Some(album) = through.listed_album {
        if let Some(album) = album {
            found.push(MetadataRef::new(through.twin.catalog, album), stated);
        }
        return reading(found.settle(), None);
    }
    let Some(discogs) = readers.discogs else {
        warn!(
            musicbrainz_release_group_id = group.group,
            discogs_release_id = through.twin.key,
            "No Discogs key to read the release a MusicBrainz release names; its album is not known"
        );
        found.unread = true;
        return reading(found.settle(), None);
    };
    match discogs.get_release(&through.twin.key, priority).await {
        Ok((release, _)) => {
            if let Some(master) = &release.master_id {
                found.push(MetadataRef::new(Catalog::Discogs, master.clone()), stated);
            }
            let twin = through.release_on_list.then(|| Twin {
                result: discogs_release_to_metadata(&release),
                named_by: MetadataRef::new(Catalog::MusicBrainz, through.release),
                status: (),
            });
            reading(found.settle(), twin)
        }
        Err(error) => {
            warn!(
                musicbrainz_release_group_id = group.group,
                discogs_release_id = through.twin.key,
                %error,
                "Discogs release a MusicBrainz release names unread; its album is not known"
            );
            found.unread = true;
            reading(found.settle(), None)
        }
    }
}

/// The release link one reading follows.
struct Through {
    /// The MusicBrainz release that names `twin`.
    release: String,
    twin: MetadataRef,
    /// Whether `release` is on the list, so `twin` can go beside it.
    release_on_list: bool,
    /// `Some` when `twin` is on the list already: the album its record files
    /// it under, read off the list with no request.
    listed_album: Option<Option<String>>,
}

/// Which of the group's releases' Discogs links to follow: a release on the
/// list first, since what it names can go beside it — among those, one that
/// names a release the list already holds, which costs no request — and
/// then any release the browse lists.
fn release_to_follow(
    group: &GroupToRead,
    read_links: &[(String, Vec<MetadataRef>)],
    browsed: &[musicbrainz::GroupRelease],
    on_list: &[(MetadataRef, Option<String>)],
) -> Option<Through> {
    let listed_album = |twin: &MetadataRef| {
        on_list
            .iter()
            .find(|(listed, _)| listed == twin)
            .map(|(_, album)| album.clone())
    };
    let discogs = |links: &[MetadataRef]| -> Vec<MetadataRef> {
        links
            .iter()
            .filter(|link| link.catalog == Catalog::Discogs)
            .cloned()
            .collect()
    };
    let listed: Vec<(&str, Vec<MetadataRef>)> = group
        .releases
        .iter()
        .map(|(release, own)| {
            let links = read_links
                .iter()
                .find(|(read, _)| read == release)
                .map_or(own.as_slice(), |(_, links)| links.as_slice());
            (release.as_str(), discogs(links))
        })
        .collect();
    let through = |release: &str, twin: &MetadataRef, release_on_list: bool| Through {
        release: release.to_string(),
        twin: twin.clone(),
        release_on_list,
        listed_album: listed_album(twin),
    };
    listed
        .iter()
        .find_map(|(release, links)| {
            links
                .iter()
                .find(|twin| listed_album(twin).is_some())
                .map(|twin| through(release, twin, true))
        })
        .or_else(|| {
            listed.iter().find_map(|(release, links)| {
                links.first().map(|twin| through(release, twin, true))
            })
        })
        .or_else(|| {
            browsed.iter().find_map(|release| {
                discogs(&release_links_of(&release.relations))
                    .first()
                    .map(|twin| through(&release.id, twin, false))
            })
        })
}

/// Whether a page of `catalog` names an album this reading joins: one of
/// another lookup catalog's.
fn names_other_album(catalog: Catalog) -> bool {
    catalog != Catalog::MusicBrainz && Catalog::LOOKUP.contains(&catalog)
}

/// The Wikidata items a group's page links, each once, in relation order.
fn wikidata_items(pages: &[CatalogPage]) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for page in pages {
        if let CatalogPage::Group {
            catalog: Catalog::Wikidata,
            key,
        } = page
        {
            if !items.contains(key) {
                items.push(key.clone());
            }
        }
    }
    items
}

#[cfg(test)]
#[path = "album_links_tests.rs"]
mod tests;
