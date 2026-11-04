//! The catalogs' keys for releases and albums, and the documents a lookup
//! returned.

use crate::catalog::{Catalog, LookupCatalog};
use serde::{Deserialize, Serialize};

/// One catalog's key for one entity. Whether this points at a release vs. a
/// release-group/master is determined by the field this value lives in —
/// there's no structural difference, both are `(catalog, key)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetadataRef {
    pub catalog: Catalog,
    pub key: String,
}

impl MetadataRef {
    pub fn new(catalog: Catalog, key: impl Into<String>) -> Self {
        Self { catalog, key: key.into() }
    }

    /// This key, when its catalog answers lookups and so is one bae fetches
    /// from.
    pub fn lookup(&self) -> Option<LookupRef> {
        Some(LookupRef::new(self.catalog.lookup()?, self.key.clone()))
    }
}

/// A release on a catalog that answers lookups: one bae can fetch, read and
/// pick. A record naming any other catalog is a [`MetadataRef`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LookupRef {
    pub catalog: LookupCatalog,
    pub key: String,
}

impl LookupRef {
    pub fn new(catalog: LookupCatalog, key: impl Into<String>) -> Self {
        Self { catalog, key: key.into() }
    }

    /// The same key as a reference any catalog's record can hold.
    pub fn metadata_ref(&self) -> MetadataRef {
        MetadataRef::new(self.catalog.catalog(), self.key.clone())
    }
}

impl From<LookupRef> for MetadataRef {
    fn from(release: LookupRef) -> Self {
        Self { catalog: release.catalog.catalog(), key: release.key }
    }
}

/// A known pressing or album in one catalog. Album links do not claim that
/// any particular pressing was selected. At most one record per catalog is
/// kept, and a release's list of them names the applied record — the one its
/// draft was read from, or the linked pressing — first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReleaseRecord {
    Pressing { release: MetadataRef, album_key: Option<String> },
    Album { album: MetadataRef },
}

impl ReleaseRecord {
    pub fn new(release: &MetadataRef, album_key: Option<String>) -> Self {
        Self::Pressing { release: release.clone(), album_key }
    }

    pub fn album(album: &MetadataRef) -> Self {
        Self::Album { album: album.clone() }
    }

    pub fn catalog(&self) -> Catalog {
        match self {
            Self::Pressing { release, .. } => release.catalog,
            Self::Album { album } => album.catalog,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            Self::Pressing { release, .. } => &release.key,
            Self::Album { album } => &album.key,
        }
    }

    pub fn url(&self) -> String {
        match self {
            Self::Pressing { release, .. } => release.catalog.release_url(&release.key),
            Self::Album { album } => album.catalog.album_url(&album.key),
        }
    }

    pub fn release_ref(&self) -> Option<&MetadataRef> {
        match self {
            Self::Pressing { release, .. } => Some(release),
            Self::Album { .. } => None,
        }
    }

    pub fn album_ref(&self) -> Option<MetadataRef> {
        match self {
            Self::Pressing { release, album_key, .. } => {
                album_key.as_ref().map(|key| MetadataRef::new(release.catalog, key.clone()))
            }
            Self::Album { album } => Some(album.clone()),
        }
    }
}

/// Which lookup produced a fetched document, and therefore which entity's id it
/// is keyed by.
///
/// Wider than [`Catalog`]: fetching one release fetches supporting documents
/// that belong to other entities — its release group, a Discogs master — and
/// each is keyed by the entity it describes.
#[cfg(desktop)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayloadSource {
    /// A MusicBrainz release, by release id.
    MusicBrainz,
    /// A MusicBrainz release group, by group id.
    MusicBrainzReleaseGroup,
    /// A Discogs release, by release id. Also where a MusicBrainz-seeded
    /// release's cross-reference lands: the id comes out of the MusicBrainz
    /// document's url-rels.
    Discogs,
    /// A Discogs master, by master id — read out of the Discogs release
    /// document that names it.
    DiscogsMaster,
    /// The MusicBrainz release cross-linked to a *Discogs* release, keyed by the
    /// Discogs release id. Its own key is not derivable the way the reverse
    /// direction's is: MusicBrainz's URL lookup endpoint found it, and nothing
    /// in the Discogs document names it back.
    MusicBrainzDiscogsXref,
    /// A uniquely cross-linked MusicBrainz group, keyed by Discogs master id.
    MusicBrainzDiscogsMasterXref,
    /// A Wikidata item, by item id — read out of the url-rels of the
    /// MusicBrainz release or release group that names it.
    Wikidata,
}

#[cfg(desktop)]
impl PayloadSource {
    /// The stored `document` column value of a document a fetch missed.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::MusicBrainzReleaseGroup => "musicbrainz_release_group",
            Self::Discogs => "discogs",
            Self::DiscogsMaster => "discogs_master",
            Self::MusicBrainzDiscogsXref => "musicbrainz_discogs_xref",
            Self::MusicBrainzDiscogsMasterXref => "musicbrainz_discogs_master_xref",
            Self::Wikidata => "wikidata",
        }
    }

    /// The catalog asked for this document: a lookup by a Discogs address is
    /// asked of MusicBrainz. A Wikidata item is no lookup catalog's.
    pub fn asked_catalog(self) -> Option<LookupCatalog> {
        match self {
            Self::MusicBrainz
            | Self::MusicBrainzReleaseGroup
            | Self::MusicBrainzDiscogsXref
            | Self::MusicBrainzDiscogsMasterXref => Some(LookupCatalog::MusicBrainz),
            Self::Discogs | Self::DiscogsMaster => Some(LookupCatalog::Discogs),
            Self::Wikidata => None,
        }
    }

    /// The payload holding a release's own editorial metadata on `source` — the
    /// anchor of everything else identification fetched alongside it.
    pub fn release_of(catalog: LookupCatalog) -> Self {
        match catalog {
            LookupCatalog::MusicBrainz => Self::MusicBrainz,
            LookupCatalog::Discogs => Self::Discogs,
        }
    }
}

#[cfg(desktop)]
impl std::str::FromStr for PayloadSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "musicbrainz" => Ok(Self::MusicBrainz),
            "musicbrainz_release_group" => Ok(Self::MusicBrainzReleaseGroup),
            "discogs" => Ok(Self::Discogs),
            "discogs_master" => Ok(Self::DiscogsMaster),
            "musicbrainz_discogs_xref" => Ok(Self::MusicBrainzDiscogsXref),
            "musicbrainz_discogs_master_xref" => Ok(Self::MusicBrainzDiscogsMasterXref),
            "wikidata" => Ok(Self::Wikidata),
            _ => Err(format!("unknown payload source: {s}")),
        }
    }
}

/// One document a metadata lookup returned, carrying the entity it describes so
/// the walk can key it without re-reading it.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePayload {
    pub source: PayloadSource,
    pub source_release_id: String,
    pub json: String,
}

#[cfg(desktop)]
impl SourcePayload {
    pub fn new(source: PayloadSource, source_release_id: impl Into<String>, json: String) -> Self {
        Self { source, source_release_id: source_release_id.into(), json }
    }
}
