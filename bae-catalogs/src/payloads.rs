//! The documents one source release's lookups returned, and the extraction
//! that reads them into a [`SourceRelease`].
//!
//! A release is described by more than its own document: MusicBrainz adds a
//! release group, the Wikidata item that group names, and — where an editor
//! linked one — a Discogs cross-reference with its master. [`ReleasePayloads`]
//! is that whole set, and [`ReleasePayloads::extract`] reads every fact the
//! import surfaces need out of it at once. The documents themselves are never
//! stored: a set lives as long as the fetch that returned it.

use crate::catalog::{parse_catalog_url, Catalog, CatalogPage, LookupCatalog};
use crate::discogs::client::DiscogsClient;
use crate::discogs::DiscogsRelease;
use crate::musicbrainz::MbReleaseResponse;
use crate::rate_limiter::CallClass;
use crate::records::{LookupRef, MetadataRef, PayloadSource, ReleaseRecord, SourcePayload};
use crate::remote_image::RemoteCover;
use crate::source_release::{
    ArchiveRelease, CatalogFacts, ReleaseCovers, SourceRelease, UnfetchedDocument,
};
use crate::CatalogError;
use tracing::warn;

mod projection;
mod relationships;
mod traversal;

/// How far a read of one release follows the documents its pages link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadDepth {
    /// What an identify run ranks the release by: its own page, its album's
    /// page (a MusicBrainz release group, a Discogs master) and, for a
    /// MusicBrainz release, the Wikidata items and Discogs releases those two
    /// pages link, which its album statements read. Every other document they
    /// name is recorded as one the read stopped short of. A release a Disc ID
    /// lookup returned takes its own page from that answer (see
    /// [`DiscIdRelease`]), and records the page as stopped short of too.
    Identification,
    /// Every document its links reach: what anything that applies the
    /// release reads — a pick, an unattended pick, a pick's partners, a
    /// re-identify, a shared-album offer.
    Complete,
}

/// One release as MusicBrainz's Disc ID lookup answered it: the release's own
/// document, stating its labels, barcode, links, release group, and its
/// media with their tracks and registered Disc IDs — everything an
/// identification read takes from the release lookup's document, less the
/// annotation. The recording, work and artist relations an import maps are
/// absent too. Shared, since an identify run's state carries it from the
/// lookup to the read.
#[derive(Clone, PartialEq, Eq)]
pub struct DiscIdRelease {
    release: LookupRef,
    json: std::sync::Arc<str>,
}

impl DiscIdRelease {
    /// MusicBrainz release `release_id`, as `json` states it.
    pub fn new(release_id: String, json: String) -> Self {
        Self { release: LookupRef::new(LookupCatalog::MusicBrainz, release_id), json: json.into() }
    }

    /// The release the answer states.
    pub fn release(&self) -> &LookupRef {
        &self.release
    }

    pub(crate) fn json(&self) -> &str {
        &self.json
    }
}

impl std::fmt::Debug for DiscIdRelease {
    /// The release and the document's size: the document itself is many
    /// lines of JSON every state a run reports would print.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscIdRelease")
            .field("release", &self.release)
            .field("json_bytes", &self.json.len())
            .finish()
    }
}

/// Every document one catalog release's lookups produced, anchored on the
/// release itself.
///
/// The anchor is a field rather than one entry among the rest, so a value of
/// this type cannot exist without the document it is about — which is what
/// makes holding one mean "this release was fetched". The supporting documents
/// are each present or not on their own terms: a release with no group and a
/// source with no cross-reference both read the same way. Every supporting
/// document was admitted by the walk that fetched it, so extraction need not
/// validate the same bytes again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePayloads {
    release: LookupRef,
    /// The release's own document, as its source returned it.
    anchor: String,
    supporting: Vec<SourcePayload>,
    /// The documents the walk named and could not get.
    unfetched: Vec<UnfetchedDocument>,
}

impl ReleasePayloads {
    /// A set of documents a test states whole: the release's own and
    /// whatever supporting documents it names, each admitted as a fetch
    /// admits it.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub fn for_test(release: LookupRef, anchor: String, supporting: Vec<SourcePayload>) -> Self {
        Self {
            release,
            anchor,
            supporting: supporting
                .into_iter()
                .filter(|document| {
                    traversal::supporting_document_edges(
                        document.source,
                        &document.source_release_id,
                        &document.json,
                    )
                    .is_some()
                })
                .collect(),
            unfetched: Vec::new(),
        }
    }

    /// The release these documents describe.
    pub fn release(&self) -> &LookupRef {
        &self.release
    }

    fn document(&self, source: PayloadSource, key: &str) -> Option<&str> {
        self.supporting
            .iter()
            .find(|d| d.source == source && d.source_release_id == key)
            .map(|d| d.json.as_str())
    }

    fn source_data(&self, detail: String) -> CatalogError {
        CatalogError::SourceData { catalog: self.release.catalog.catalog(), detail }
    }

    /// The anchoring MusicBrainz release, parsed. Only called down the
    /// MusicBrainz arm of a `self.release.catalog` match, where the anchor is
    /// that release's own document.
    fn musicbrainz_anchor(&self) -> Result<MbReleaseResponse, CatalogError> {
        serde_json::from_str(&self.anchor).map_err(|e| {
            self.source_data(format!("stored MusicBrainz release does not parse: {e}"))
        })
    }

    /// The anchoring Discogs release, parsed, as [`Self::musicbrainz_anchor`].
    fn discogs_anchor(&self) -> Result<DiscogsRelease, CatalogError> {
        crate::discogs::documents::parse_discogs_release_json(&self.anchor)
            .map_err(|e| self.source_data(format!("stored Discogs release does not parse: {e}")))
    }

    /// The Discogs release an editor cross-linked to the anchoring MusicBrainz
    /// one, parsed. `None` when the anchor's url-rels name none, or when no
    /// Discogs key was configured when it was fetched — both of which the
    /// mapper takes as "no cross-reference".
    fn discogs_xref(&self) -> Result<Option<DiscogsRelease>, CatalogError> {
        let Some(counterpart) = self.counterpart()? else {
            return Ok(None);
        };
        let Some(json) = self.document(PayloadSource::Discogs, &counterpart.key) else {
            return Ok(None);
        };
        crate::discogs::documents::parse_discogs_release_json(json)
            .map(Some)
            .map_err(CatalogError::from)
    }

    /// The MusicBrainz release cross-linked to a Discogs-seeded one.
    fn musicbrainz_xref(&self) -> Result<Option<MbReleaseResponse>, CatalogError> {
        let Some(json) = self.document(PayloadSource::MusicBrainzDiscogsXref, &self.release.key)
        else {
            return Ok(None);
        };
        serde_json::from_str(json).map(Some).map_err(|e| {
            self.source_data(format!("stored MusicBrainz cross-reference does not parse: {e}"))
        })
    }

    /// The images the release documents in this set publish: the anchor's own
    /// front image, and every image of the release an editor cross-linked to
    /// it. These are this pressing's artwork, whichever catalog printed it.
    fn release_covers(&self) -> Result<Vec<RemoteCover>, CatalogError> {
        let mut covers = Vec::new();
        match self.release.catalog {
            LookupCatalog::MusicBrainz => {
                covers.extend(crate::remote_image::musicbrainz_release_cover(
                    &self.musicbrainz_anchor()?,
                ));
                if let Some(release) = self.discogs_xref()? {
                    covers.extend(release.covers);
                }
            }
            LookupCatalog::Discogs => {
                covers.extend(self.discogs_anchor()?.covers);
                if let Some(release) = self.musicbrainz_xref()? {
                    covers.extend(crate::remote_image::musicbrainz_release_cover(&release));
                }
            }
        }
        Ok(covers)
    }

    /// The images the albums these releases belong to publish: a Discogs
    /// master's gallery and the archive's address for a MusicBrainz release
    /// group.
    ///
    /// An album's image is some release of the album's, which may not be this
    /// one and may not exist at all, so every one of them is offered after
    /// every release image.
    fn album_covers(&self) -> Result<Vec<RemoteCover>, CatalogError> {
        let musicbrainz = match self.release.catalog {
            LookupCatalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            LookupCatalog::Discogs => self.musicbrainz_xref()?,
        };
        let mut covers: Vec<RemoteCover> = musicbrainz
            .as_ref()
            .and_then(crate::remote_image::musicbrainz_album_cover)
            .into_iter()
            .collect();
        for (catalog, key, json) in self.album_documents()? {
            match catalog {
                LookupCatalog::Discogs => {
                    covers.extend(crate::discogs::documents::parse_discogs_master_covers(json)?)
                }
                LookupCatalog::MusicBrainz => {
                    covers.push(RemoteCover::musicbrainz_release_group(key))
                }
            }
        }
        Ok(covers)
    }

    /// Read every fact the import surfaces use out of this set: the
    /// release's resolved album and pressing facts, the records its documents
    /// state, its cover options, and its full tracklist.
    pub fn extract(&self) -> Result<SourceRelease, CatalogError> {
        let metadata = self.projected_metadata()?;
        let mut other_records = self.projected_records()?;
        other_records.retain(|record| record.catalog() != self.release.catalog.catalog());
        let covers = ReleaseCovers { release: self.release_covers()?, album: self.album_covers()? };
        let archive_groups = self
            .album_documents()?
            .into_iter()
            .filter(|(catalog, _, _)| *catalog == LookupCatalog::MusicBrainz)
            .map(|(_, key, _)| key.to_string())
            .collect();
        let archived_musicbrainz = match self.release.catalog {
            LookupCatalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            LookupCatalog::Discogs => self.musicbrainz_xref()?,
        };
        let archive_release = archived_musicbrainz.map(|release| ArchiveRelease {
            group_id: release.release_group.map(|group| group.id),
            release_id: release.id,
        });
        let (source_group_id, mediums, release_roles, catalog, notes) = match self.release.catalog {
            LookupCatalog::MusicBrainz => {
                let anchor = self.musicbrainz_anchor()?;
                (
                    anchor.release_group.as_ref().map(|group| group.id.clone()),
                    crate::musicbrainz_mapper::mediums(&anchor),
                    crate::musicbrainz_mapper::release_roles(&anchor),
                    CatalogFacts::MusicBrainz {
                        links: crate::search::release_links_of(&anchor.relations),
                        album_links: self.album_statements()?,
                    },
                    crate::search::musicbrainz_release_notes(&anchor),
                )
            }
            LookupCatalog::Discogs => {
                let anchor = self.discogs_anchor()?;
                let media = crate::discogs_mapper::pressing(&anchor).1;
                (
                    anchor.master_id.clone(),
                    crate::discogs_mapper::mediums(&anchor, &media),
                    crate::discogs_mapper::release_roles(&anchor),
                    CatalogFacts::Discogs { media },
                    crate::search::discogs_release_notes(&anchor),
                )
            }
        };
        Ok(SourceRelease {
            release: self.release.clone(),
            source_group_id,
            metadata,
            other_records,
            covers,
            archive_release,
            archive_groups,
            mediums,
            release_roles,
            catalog,
            notes,
            unfetched: self.unfetched.clone(),
        })
    }
}

pub(crate) use traversal::{fetch_documents, read_disc_id_release};

#[cfg(test)]
#[path = "payloads_tests.rs"]
mod tests;
