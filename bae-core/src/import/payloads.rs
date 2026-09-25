//! The documents one source release's lookups returned, and the extraction
//! that reads them into a [`SourceRelease`].
//!
//! A release is described by more than its own document: MusicBrainz adds a
//! release group, the Wikidata item that group names, and — where an editor
//! linked one — a Discogs cross-reference with its master. [`ReleasePayloads`]
//! is that whole set, and [`ReleasePayloads::extract`] reads every fact the
//! import surfaces need out of it at once. The documents themselves are never
//! stored: a set lives as long as the fetch that returned it.

use crate::discogs::client::DiscogsClient;
use crate::discogs::DiscogsRelease;
use crate::import::cover_art::RemoteCover;
use crate::import::source_release::{
    not_fetched, ArchiveRelease, CatalogFacts, ReleaseCovers, SourceRelease, UnfetchedDocument,
};
use crate::import::{
    parse_catalog_url, Catalog, CatalogPage, ImportError, MetadataRef, PayloadSource,
    ReleaseRecord, SourcePayload,
};
use crate::musicbrainz::MbReleaseResponse;
use crate::util::rate_limiter::CallPriority;
use tracing::warn;

mod projection;
mod relationships;
mod traversal;

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
    release: MetadataRef,
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
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(release: MetadataRef, anchor: String, supporting: Vec<SourcePayload>) -> Self {
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
    pub fn release(&self) -> &MetadataRef {
        &self.release
    }

    fn document(&self, source: PayloadSource, key: &str) -> Option<&str> {
        self.supporting
            .iter()
            .find(|d| d.source == source && d.source_release_id == key)
            .map(|d| d.json.as_str())
    }

    fn source_data(&self, detail: String) -> ImportError {
        ImportError::SourceData {
            catalog: self.release.catalog,
            detail,
        }
    }

    /// The anchoring MusicBrainz release, parsed. Only called down the
    /// MusicBrainz arm of a `self.release.catalog` match, where the anchor is
    /// that release's own document.
    fn musicbrainz_anchor(&self) -> Result<MbReleaseResponse, ImportError> {
        serde_json::from_str(&self.anchor).map_err(|e| {
            self.source_data(format!("stored MusicBrainz release does not parse: {e}"))
        })
    }

    /// The anchoring Discogs release, parsed, as [`Self::musicbrainz_anchor`].
    fn discogs_anchor(&self) -> Result<DiscogsRelease, ImportError> {
        crate::discogs::client::parse_discogs_release_json(&self.anchor)
            .map_err(|e| self.source_data(format!("stored Discogs release does not parse: {e}")))
    }

    /// The Discogs release an editor cross-linked to the anchoring MusicBrainz
    /// one, parsed. `None` when the anchor's url-rels name none, or when no
    /// Discogs key was configured when it was fetched — both of which the
    /// mapper takes as "no cross-reference".
    fn discogs_xref(&self) -> Result<Option<DiscogsRelease>, ImportError> {
        let Some(counterpart) = self.counterpart()? else {
            return Ok(None);
        };
        let Some(json) = self.document(PayloadSource::Discogs, &counterpart.key) else {
            return Ok(None);
        };
        crate::discogs::client::parse_discogs_release_json(json)
            .map(Some)
            .map_err(ImportError::from)
    }

    /// The MusicBrainz release cross-linked to a Discogs-seeded one.
    fn musicbrainz_xref(&self) -> Result<Option<MbReleaseResponse>, ImportError> {
        let Some(json) = self.document(PayloadSource::MusicBrainzDiscogsXref, &self.release.key)
        else {
            return Ok(None);
        };
        serde_json::from_str(json).map(Some).map_err(|e| {
            self.source_data(format!(
                "stored MusicBrainz cross-reference does not parse: {e}"
            ))
        })
    }

    /// The images the release documents in this set publish: the anchor's own
    /// front image, and every image of the release an editor cross-linked to
    /// it. These are this pressing's artwork, whichever catalog printed it.
    fn release_covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = Vec::new();
        match self.release.catalog {
            Catalog::MusicBrainz => {
                covers.extend(crate::import::cover_art::musicbrainz_release_cover(
                    &self.musicbrainz_anchor()?,
                ));
                if let Some(release) = self.discogs_xref()? {
                    covers.extend(release.covers);
                }
            }
            Catalog::Discogs => {
                covers.extend(self.discogs_anchor()?.covers);
                if let Some(release) = self.musicbrainz_xref()? {
                    covers.extend(crate::import::cover_art::musicbrainz_release_cover(
                        &release,
                    ));
                }
            }
            other => not_fetched(other),
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
    fn album_covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let musicbrainz = match self.release.catalog {
            Catalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            Catalog::Discogs => self.musicbrainz_xref()?,
            other => not_fetched(other),
        };
        let mut covers: Vec<RemoteCover> = musicbrainz
            .as_ref()
            .and_then(crate::import::cover_art::musicbrainz_album_cover)
            .into_iter()
            .collect();
        for (catalog, key, json) in self.album_documents()? {
            match catalog {
                Catalog::Discogs => {
                    covers.extend(crate::discogs::client::parse_discogs_master_covers(json)?)
                }
                Catalog::MusicBrainz => covers.push(RemoteCover::musicbrainz_release_group(key)),
                other => not_fetched(other),
            }
        }
        Ok(covers)
    }

    /// Read every fact the import surfaces use out of this set: the
    /// release's resolved album and pressing facts, the records its documents
    /// state, its cover options, and its full tracklist.
    pub fn extract(&self) -> Result<SourceRelease, ImportError> {
        let metadata = self.projected_metadata()?;
        let mut other_records = self.projected_records()?;
        other_records.retain(|record| record.catalog() != self.release.catalog);
        let covers = ReleaseCovers {
            release: self.release_covers()?,
            album: self.album_covers()?,
        };
        let archive_groups = self
            .album_documents()?
            .into_iter()
            .filter(|(catalog, _, _)| *catalog == Catalog::MusicBrainz)
            .map(|(_, key, _)| key.to_string())
            .collect();
        let archived_musicbrainz = match self.release.catalog {
            Catalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            Catalog::Discogs => self.musicbrainz_xref()?,
            other => not_fetched(other),
        };
        let archive_release = archived_musicbrainz.map(|release| ArchiveRelease {
            group_id: release.release_group.map(|group| group.id),
            release_id: release.id,
        });
        let (source_group_id, mediums, catalog) = match self.release.catalog {
            Catalog::MusicBrainz => {
                let anchor = self.musicbrainz_anchor()?;
                (
                    anchor.release_group.as_ref().map(|group| group.id.clone()),
                    crate::import::musicbrainz_mapper::mediums(&anchor),
                    CatalogFacts::MusicBrainz {
                        links: crate::import::search::mb_release_links(&anchor),
                    },
                )
            }
            Catalog::Discogs => {
                let anchor = self.discogs_anchor()?;
                (
                    anchor.master_id.clone(),
                    crate::import::discogs_mapper::mediums(&anchor),
                    CatalogFacts::Discogs {
                        formats: anchor.format.clone(),
                        release_roles: crate::import::discogs_mapper::release_roles(&anchor),
                    },
                )
            }
            other => not_fetched(other),
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
            catalog,
            unfetched: self.unfetched.clone(),
        })
    }
}

pub(crate) use traversal::fetch_documents;

#[cfg(test)]
#[path = "payloads_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "payloads/enrichment_tests.rs"]
mod enrichment_tests;

#[cfg(test)]
#[path = "payloads/supplemental_tests.rs"]
mod supplemental_tests;

#[cfg(test)]
#[path = "payloads/ambiguity_tests.rs"]
mod ambiguity_tests;
