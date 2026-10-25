//! One catalog release bae fetched, as bae keeps it.
//!
//! Everything an import surface reads about a release — the picker's detail,
//! the draft a pick projects, the tracklist the auto-import check reads, the records
//! a commit writes, the covers a picker offers — is extracted from the
//! release's documents once, when it is fetched, into a [`SourceRelease`].
//! What depends on the folder's audio is not extracted: which of the release's
//! mediums the audio is a rip of, and how a Discogs tracklist's index and
//! heading rows are laid out against it, are chosen when the release is read
//! against the audio (see `import::catalog_reading`).

use crate::pressing::Pressing;
use crate::import::cover_art::RemoteCover;
use crate::import::medium_coverage::MediumCoverage;
use crate::import::release_metadata::ReleaseMetadata;
use crate::import::search::ImportSearchReleaseDetail;
use crate::pressing::{Medium, StatedMedia};
use crate::import::{ArtistCredit, Catalog, ImportError, MetadataRef, ReleaseRecord};

/// The catalogs bae fetches releases from are exactly the ones it asks, so a
/// source release cannot exist for any other.
pub(crate) fn not_fetched(catalog: Catalog) -> ! {
    unreachable!("nothing fetches releases from {}", catalog.as_str())
}

/// One fetched release: its facts with every supporting document's
/// contribution already resolved into them, and its full tracklist.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRelease {
    pub(crate) release: MetadataRef,
    /// The album the release's own catalog files it under: its MusicBrainz
    /// release group or its Discogs master.
    pub(crate) source_group_id: Option<String>,
    /// Album and pressing facts, the release's own first and whatever its
    /// cross-referenced release and album documents supplied where it stated
    /// nothing.
    pub(crate) metadata: ReleaseMetadata,
    /// What the other catalogs say this release, or its album, is — never the
    /// release's own catalog, whose record [`Self::records`] derives. Its
    /// documents' statements, and, once stored, what reading its album's
    /// MusicBrainz release group found the album to be where the documents
    /// reach no record of that catalog (see [`crate::import::album_links`]).
    pub(crate) other_records: Vec<ReleaseRecord>,
    pub(crate) covers: ReleaseCovers,
    /// The MusicBrainz release whose Cover Art Archive gallery a picker asks
    /// for: this release itself, or the MusicBrainz release a Discogs release
    /// is cross-referenced to.
    pub(crate) archive_release: Option<ArchiveRelease>,
    /// The MusicBrainz release groups whose documents named this release's
    /// album, in the order the album covers were read from them.
    pub(crate) archive_groups: Vec<String>,
    pub(crate) mediums: Vec<SourceMedium>,
    pub(crate) catalog: CatalogFacts,
    /// What the release's own document writes about which pressing it is, in
    /// free text and in its order: see
    /// [`crate::import::search::musicbrainz_release_notes`] and
    /// [`crate::import::search::discogs_release_notes`].
    pub(crate) notes: Vec<String>,
    /// The documents the fetch named and did not get, whose facts are
    /// missing from the ones above: supporting documents, and the release's
    /// own where an identify run read it from a Disc ID lookup's answer.
    pub(crate) unfetched: Vec<UnfetchedDocument>,
}

/// One document a fetch followed a link to, or started from an answer in
/// place of, and did not get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnfetchedDocument {
    pub(crate) document: crate::import::PayloadSource,
    pub(crate) key: String,
    pub(crate) reason: UnfetchedReason,
}

/// Why a linked document is missing from a fetched release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnfetchedReason {
    /// Its source was asked and failed.
    Failed,
    /// It is a Discogs document and this library held no Discogs key.
    DiscogsNotConfigured,
    /// The read stopped short of it on purpose: an identification read
    /// follows only the documents ranking the release reads, and takes a
    /// release a Disc ID lookup returned from that answer rather than its own
    /// document.
    StoppedShort,
}

/// The images a release's documents offer, in the order a picker shows them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseCovers {
    /// This pressing's own artwork, whichever catalog printed it.
    pub(crate) release: Vec<RemoteCover>,
    /// The album's artwork, which is some release of the album's and may not
    /// be this one.
    pub(crate) album: Vec<RemoteCover>,
}

/// A MusicBrainz release as the Cover Art Archive addresses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchiveRelease {
    pub(crate) release_id: String,
    pub(crate) group_id: Option<String>,
}

/// What only one catalog's release document states.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CatalogFacts {
    MusicBrainz {
        /// The releases on other catalogs the document names as the same
        /// release, in relation order.
        links: Vec<MetadataRef>,
        /// What its documents state its album is on the other lookup
        /// catalog: see [`crate::import::payloads::ReleasePayloads::album_statements`].
        album_links: crate::import::album_links::AlbumLinks,
    },
    Discogs {
        /// What the release's format entries say it is made of, entry by
        /// entry with each entry's quantity.
        media: StatedMedia,
        /// The composer credits the release states for itself rather than for
        /// one track.
        release_roles: Vec<RoleCredit>,
    },
}

/// One medium: a disc, a side pair of a record, a layer of a hybrid disc.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceMedium {
    /// The carrier this medium is, `None` where the catalog does not say one
    /// bae knows: the format MusicBrainz states for the medium, or the one a
    /// Discogs release's position prefix or format entries give it (see
    /// [`crate::import::discogs_mediums`]).
    pub(crate) medium: Option<Medium>,
    /// The disc IDs MusicBrainz has registered against this medium, each
    /// once, in its order; a Discogs medium holds none.
    pub(crate) disc_ids: Vec<String>,
    pub(crate) entries: Vec<TracklistEntry>,
}

/// One row of a tracklist, as its catalog lists it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TracklistEntry {
    pub(crate) kind: EntryKind,
    /// The position the catalog prints: `A1`, `1-2`, `3`. Unstated where the
    /// catalog prints none.
    pub(crate) position: Option<String>,
    /// MusicBrainz's own count of the track within its medium, read where it
    /// prints no position.
    pub(crate) number: Option<i64>,
    /// The title the catalog gives the row. A MusicBrainz track with no
    /// usable title keeps none, and reading it fails.
    pub(crate) title: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    /// Display credits in the catalog's order.
    pub(crate) credits: Vec<DisplayCredit>,
    /// Composer credits, at their positions among the row's relations.
    pub(crate) roles: Vec<RoleCredit>,
    /// The works a MusicBrainz recording performs, at their positions among
    /// its relations.
    pub(crate) works: Vec<PerformedWork>,
    /// A Discogs index's sub-tracks.
    pub(crate) children: Vec<TracklistEntry>,
}

/// What kind of row a tracklist entry is. Only Discogs lists anything other
/// than tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Track,
    /// A title over the sub-track rows that follow it.
    Heading,
    /// A title holding its sub-tracks as children.
    Index,
}

impl EntryKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Heading => "heading",
            Self::Index => "index",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "track" => Some(Self::Track),
            "heading" => Some(Self::Heading),
            "index" => Some(Self::Index),
            _ => None,
        }
    }
}

/// One display credit. `artist` is absent where the catalog printed a name
/// with no artist behind it; the printed name still heads the picker's row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DisplayCredit {
    pub(crate) position: i32,
    pub(crate) credited_name: String,
    pub(crate) artist: Option<ArtistCredit>,
    /// What the catalog prints after the credit when another follows.
    pub(crate) join: Option<String>,
}

/// A track's own credits as its display credits state them, place by place:
/// a credit naming an artist as that artist, a printed name with no artist
/// behind it as a credit of that name. The last credit's join is not kept.
pub(crate) fn stated_credits(credits: &[DisplayCredit]) -> Vec<crate::import::SourceCredit> {
    let mut stated: Vec<crate::import::SourceCredit> = credits
        .iter()
        .filter_map(|credit| {
            let artist = match &credit.artist {
                Some(artist) => artist.clone(),
                None if credit.credited_name.trim().is_empty() => return None,
                None => ArtistCredit::named(credit.credited_name.trim()),
            };
            Some(crate::import::SourceCredit {
                credit: artist,
                join: credit.join.clone(),
            })
        })
        .collect();
    crate::credits::drop_last_join(&mut stated, |credit| &mut credit.join);
    stated
}

/// One composer credit and the words its catalog credits it with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoleCredit {
    pub(crate) position: i32,
    pub(crate) artist: ArtistCredit,
    pub(crate) role: Option<String>,
}

/// One work a recording performs, at its position among the recording's
/// relations.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PerformedWork {
    pub(crate) position: i32,
    pub(crate) work: SourceWork,
}

/// A MusicBrainz work as one reference to it states it: every reference
/// carries its whole sub-graph, and reading the release expands each work
/// once.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceWork {
    pub(crate) musicbrainz_work_id: String,
    pub(crate) title: String,
    pub(crate) disambiguation: Option<String>,
    pub(crate) work_type: Option<String>,
    /// Composer credits and part relations, in relation order.
    pub(crate) events: Vec<SourceWorkEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SourceWorkEvent {
    Composer(ArtistCredit),
    Part {
        direction: PartDirection,
        work: SourceWork,
    },
}

/// Which way a MusicBrainz "parts" relation points from the work stating it:
/// `Forward` names a part of the work, `Backward` the work it is part of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartDirection {
    Forward,
    Backward,
}

impl SourceRelease {
    /// The release these facts describe.
    pub fn release(&self) -> &MetadataRef {
        &self.release
    }

    /// Whether reading this release again at `depth` could add what its fetch
    /// missed: a document whose source failed, a Discogs document now that a
    /// key is held, or — for a complete read — a document an identification
    /// read stopped short of. Anything else missing is what the catalogs
    /// answered, and asking again answers the same.
    pub(crate) fn fetch_could_add(
        &self,
        depth: crate::import::payloads::ReadDepth,
        discogs_configured: bool,
    ) -> bool {
        self.unfetched.iter().any(|document| match document.reason {
            UnfetchedReason::Failed => true,
            UnfetchedReason::DiscogsNotConfigured => discogs_configured,
            UnfetchedReason::StoppedShort => match depth {
                crate::import::payloads::ReadDepth::Identification => false,
                crate::import::payloads::ReadDepth::Complete => true,
            },
        })
    }

    /// Every catalog identity known for this release at pressing or album
    /// level: its own first, then what the other catalogs say, in catalog
    /// order.
    pub fn records(&self) -> Vec<ReleaseRecord> {
        let mut others = self.other_records.clone();
        others.sort_by_key(|record| catalog_rank(record.catalog()));
        let mut records = vec![ReleaseRecord::new(
            &self.release,
            self.source_group_id.clone(),
        )];
        records.extend(others);
        records
    }

    /// The mediums the coverage names, in release order.
    pub(crate) fn covered_mediums<'a>(
        &'a self,
        coverage: &'a MediumCoverage,
    ) -> impl Iterator<Item = &'a SourceMedium> + 'a {
        self.mediums
            .iter()
            .enumerate()
            .filter(move |(position, _)| coverage.covers(*position))
            .map(|(_, medium)| medium)
    }

    /// The rows of the covered mediums, in tracklist order.
    pub(crate) fn covered_entries(&self, coverage: &MediumCoverage) -> Vec<TracklistEntry> {
        self.covered_mediums(coverage)
            .flat_map(|medium| medium.entries.iter().cloned())
            .collect()
    }

    /// Every cover option this one release offers, its pressing's images
    /// before its album's, each offered once, by
    /// [`crate::import::cover_art::offered_covers`].
    pub(crate) fn covers(&self) -> Vec<RemoteCover> {
        let mut unique = Vec::new();
        for cover in self
            .covers
            .release
            .iter()
            .chain(&self.covers.album)
            .cloned()
        {
            crate::import::cover_art::push_unique_cover(&mut unique, cover);
        }
        crate::import::cover_art::offered_covers(unique)
    }

    /// On-demand picker artwork: the Discogs images the release stored, and
    /// what the Cover Art Archive holds for the MusicBrainz release and the
    /// release groups its album was read from.
    async fn gallery_covers(
        &self,
        http: &crate::util::http::Http,
    ) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = self.covers();
        covers.retain(|cover| cover.source != Catalog::MusicBrainz);
        if let Some(archive) = &self.archive_release {
            let mut gallery = crate::import::cover_art::musicbrainz_gallery(
                http,
                &archive.release_id,
                archive.group_id.as_deref(),
            )
            .await?;
            match self.release.catalog {
                Catalog::MusicBrainz => {
                    gallery.extend(covers);
                    covers = gallery;
                }
                Catalog::Discogs => covers.extend(gallery),
                other => not_fetched(other),
            }
        }
        let covered_group = self
            .archive_release
            .as_ref()
            .and_then(|archive| archive.group_id.as_deref());
        for group in &self.archive_groups {
            if covered_group == Some(group.as_str()) {
                continue;
            }
            for cover in crate::import::cover_art::musicbrainz_group_gallery(http, group).await? {
                crate::import::cover_art::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    /// The keys the pane checks against the library.
    pub(crate) fn library_check(&self) -> crate::db::LibraryCheck {
        crate::db::LibraryCheck {
            source: self.release.catalog,
            release_id: self.release.key.clone(),
            source_group_id: self.source_group_id.clone(),
        }
    }

    /// What this release states about its media.
    pub(crate) fn stated_media(&self) -> StatedMedia {
        match &self.catalog {
            CatalogFacts::MusicBrainz { .. } if self.mediums.is_empty() => StatedMedia::Undescribed,
            CatalogFacts::MusicBrainz { .. } => StatedMedia::PerMedium(
                self.mediums.iter().map(|medium| medium.medium).collect(),
            ),
            CatalogFacts::Discogs { media, .. } => media.clone(),
        }
    }

    /// The releases on other catalogs this release's own document names as
    /// the same release. Only a MusicBrainz document names any.
    pub(crate) fn links(&self) -> Vec<MetadataRef> {
        match &self.catalog {
            CatalogFacts::MusicBrainz { links, .. } => links.clone(),
            CatalogFacts::Discogs { .. } => Vec::new(),
        }
    }

    /// What its documents state its album is on the other lookup catalog:
    /// `NotAsked` for a Discogs release, whose documents state none.
    pub(crate) fn album_links(&self) -> crate::import::album_links::AlbumLinks {
        match &self.catalog {
            CatalogFacts::MusicBrainz { album_links, .. } => album_links.clone(),
            CatalogFacts::Discogs { .. } => crate::import::album_links::AlbumLinks::NotAsked,
        }
    }

    /// The album's credits, printed as the catalog prints them.
    pub(crate) fn artist_line(&self) -> Option<String> {
        let artists = &self.metadata.album.artists;
        (!artists.is_empty()).then(|| crate::import::SourceCredit::line(artists))
    }

    /// The pressing this release describes.
    pub(crate) fn pressing(&self) -> &Pressing {
        &self.metadata.pressing
    }

    /// What the picker shows of this release as the primary of a pick: its
    /// album and pressing facts and the cover options. It shows no tracklist:
    /// picking a release applies it, and the draft it writes is what the pane
    /// shows.
    ///
    /// `partners` are the pick's other claimed releases, because the cover
    /// options the detail carries are the pick's, not this release's alone.
    pub fn detail(&self, partners: &[SourceRelease]) -> ImportSearchReleaseDetail {
        let pressing = self.pressing().clone();
        ImportSearchReleaseDetail {
            release_id: self.release.key.clone(),
            source: self.release.catalog,
            source_group_id: self.source_group_id.clone(),
            title: self.metadata.album.title.clone(),
            artist: self.artist_line(),
            year: pressing.year,
            labels: pressing.labels,
            barcodes: pressing.barcodes,
            facts: pressing.facts,
            media: self.stated_media(),
            links: self.links(),
            cover_art: pick_covers(self, partners),
        }
    }

}

pub(crate) fn catalog_rank(catalog: Catalog) -> usize {
    Catalog::ALL
        .iter()
        .position(|known| *known == catalog)
        .expect("a record names one of the catalogs")
}

/// The cover options a pick offers, in the order a surface shows them and so
/// the order the default is the first of.
///
/// A pick claims a primary release and, where it names one, that release on
/// each other catalog, and it is picked whole — so its artwork is every
/// claimed release's artwork, not the primary's alone. Every claimed
/// release's own images come first, the primary's first among them, because
/// a pressing's own cover is the one this import wants; the albums' images
/// follow in the same order, each of them being some release of the album's
/// and possibly not this one. An image reachable twice — the release an
/// editor cross-linked to the primary also claimed as a partner, two
/// partners under one master — is offered once.
pub fn pick_covers(primary: &SourceRelease, partners: &[SourceRelease]) -> Vec<RemoteCover> {
    let claimed = || std::iter::once(primary).chain(partners);
    let mut covers = Vec::new();
    for release in claimed() {
        for cover in &release.covers.release {
            crate::import::cover_art::push_unique_cover(&mut covers, cover.clone());
        }
    }
    for release in claimed() {
        for cover in &release.covers.album {
            crate::import::cover_art::push_unique_cover(&mut covers, cover.clone());
        }
    }
    crate::import::cover_art::offered_covers(covers)
}

/// The complete galleries behind [`pick_covers`], for the picker: the same
/// claimed releases, each asked what the Cover Art Archive holds for it.
pub(crate) async fn pick_gallery_covers(
    http: &crate::util::http::Http,
    primary: &SourceRelease,
    partners: &[SourceRelease],
) -> Result<Vec<RemoteCover>, ImportError> {
    let mut covers = Vec::new();
    for release in std::iter::once(primary).chain(partners) {
        for cover in release.gallery_covers(http).await? {
            crate::import::cover_art::push_unique_cover(&mut covers, cover);
        }
    }
    Ok(covers)
}

/// The records the releases one pick claims describe together.
///
/// `claimed` is the primary first — the release the draft is read from — then
/// each partner.
///
/// The primary's facts are read first, so what they say about another catalog
/// stands unless that catalog is one the person themselves claimed — a
/// claimed release's own record outranks what an editor cross-linked to it.
/// The primary's own record is the list's first, the applied one; the rest
/// follow in the order surfaces list catalogs.
pub fn claimed_records(claimed: &[&SourceRelease]) -> Vec<ReleaseRecord> {
    let mut records: Vec<ReleaseRecord> = Vec::new();
    for release in claimed {
        for record in release.records() {
            let claimed_by_the_person = record.catalog() == release.release.catalog;
            match records
                .iter_mut()
                .find(|existing| existing.catalog() == record.catalog())
            {
                Some(existing) if claimed_by_the_person => *existing = record,
                Some(_) => {}
                None => records.push(record),
            }
        }
    }
    let applied = claimed.first().map(|primary| primary.release.catalog);
    records.sort_by_key(|record| {
        (
            Some(record.catalog()) != applied,
            catalog_rank(record.catalog()),
        )
    });
    records
}
