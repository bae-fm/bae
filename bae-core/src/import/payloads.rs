//! The documents one source release's lookups returned, and the projections
//! replayed from them.
//!
//! A release is described by more than its own document: MusicBrainz adds a
//! release group, the Wikidata item that group names, and — where an editor
//! linked one — a Discogs cross-reference with its master. [`ReleasePayloads`]
//! is that whole set, and every shape the import surfaces need — the picker's
//! detail, the editor's seed, the commit's `ParsedAlbum`, the tracklist the
//! Ready rule checks, the cover options the archive serves — is projected from
//! it without touching the network.
//!
//! Each document is stored under the entity it describes, so two releases that
//! share a release group or a Discogs master share its row. The set is
//! reassembled by reading the release's own document and following the ids
//! inside it — the same reading that found them when they were fetched.

use chrono::{DateTime, Utc};

use crate::db::{Database, DbSourceReleasePayload};
use crate::discogs::client::DiscogsClient;
use crate::discogs::DiscogsRelease;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{ImportSearchReleaseDetail, SourceTracks};
use crate::import::{
    parse_catalog_url, Catalog, CatalogPage, ImportError, MetadataRef, ParsedAlbum, PayloadSource,
    ReleaseRecord, SourcePayload,
};
use crate::musicbrainz::MbReleaseResponse;
use crate::util::rate_limiter::CallPriority;
use tracing::warn;

mod projection;
mod relationships;
mod traversal;

/// The catalogs whose documents bae fetches and archives are exactly the ones
/// it asks, so a set of payloads cannot exist for any other.
fn not_fetched(catalog: Catalog) -> ! {
    unreachable!("nothing fetches documents from {}", catalog.as_str())
}

/// Every document one catalog release's lookups produced, anchored on the
/// release itself.
///
/// The anchor is a field rather than one entry among the rest, so a value of
/// this type cannot exist without the document it is about — which is what
/// makes holding one mean "identification fetched this release". The supporting
/// documents are each present or not on their own terms: a release with no
/// group and a source with no cross-reference both read the same way they did
/// at fetch time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleasePayloads {
    release: MetadataRef,
    /// The release's own document, as its source returned it.
    anchor: String,
    #[serde(deserialize_with = "deserialize_supporting_documents")]
    supporting: Vec<SourcePayload>,
}

/// Frozen applications use the same optional-document admission as fetched and
/// archived sets. Every production constructor admits supporting documents once,
/// so projection reads need not validate and log the same bytes repeatedly.
fn deserialize_supporting_documents<'de, D>(deserializer: D) -> Result<Vec<SourcePayload>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(Vec::<SourcePayload>::deserialize(deserializer)?
        .into_iter()
        .filter(|document| {
            traversal::supporting_document_edges(
                document.source,
                &document.source_release_id,
                &document.json,
            )
            .is_some()
        })
        .collect())
}

/// The documents and measured track lengths used by one metadata application.
/// Re-reading this value preserves Discogs' selected index/sub-track layout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppliedSource {
    pub payloads: ReleasePayloads,
    pub audio_durations_ms: Vec<u64>,
    /// Exact documents for the other releases explicitly claimed by this pick.
    pub partners: Vec<ReleasePayloads>,
}

impl AppliedSource {
    pub fn parsed(
        &self,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<ParsedAlbum, ImportError> {
        self.payloads.parsed(&self.audio_durations_ms, clock, ids)
    }
}

impl ReleasePayloads {
    /// The release these documents describe.
    pub fn release(&self) -> &MetadataRef {
        &self.release
    }

    /// The stored rows for this set, all stamped with one fetch time.
    pub fn rows(&self, now: DateTime<Utc>) -> Vec<DbSourceReleasePayload> {
        std::iter::once(SourcePayload::new(
            PayloadSource::release_of(self.release.catalog),
            self.release.key.clone(),
            self.anchor.clone(),
        ))
        .chain(self.supporting.iter().cloned())
        .map(|payload| DbSourceReleasePayload::new(&payload, now))
        .collect()
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

    /// Catalog identities explicitly known at pressing or album level.
    pub fn records(&self) -> Result<Vec<ReleaseRecord>, ImportError> {
        self.projected_records()
    }

    /// What the source says about this release's own tracklist — the half of the
    /// Ready rule the folder's probed durations are checked against.
    pub fn source_tracks_for_audio(
        &self,
        audio_durations_ms: &[u64],
    ) -> Result<SourceTracks, ImportError> {
        match self.release.catalog {
            Catalog::MusicBrainz => Ok(crate::import::search::mb_source_tracks(
                &self.musicbrainz_anchor()?,
            )),
            Catalog::Discogs => Ok(crate::import::search::discogs_source_tracks(
                &self.discogs_anchor()?,
                Some(audio_durations_ms),
            )),
            other => not_fetched(other),
        }
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
                    covers.extend(crate::import::cover_art::musicbrainz_release_cover(&release));
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

    /// The cover options this one release's documents offer, in the order a
    /// picker shows them: its pressing's images, then its album's.
    ///
    /// What a pick offers is [`pick_covers`] — a pick claims more releases
    /// than this one.
    fn covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut unique = Vec::new();
        for cover in self.release_covers()?.into_iter().chain(self.album_covers()?) {
            crate::import::cover_art::push_unique_cover(&mut unique, cover);
        }
        Ok(unique)
    }

    /// On-demand picker artwork. The archived documents supply Discogs images;
    /// the archive supplies the MusicBrainz release and release-group galleries.
    /// This does not change the offline metadata projection or automatic cover.
    async fn gallery_covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = self.covers()?;
        covers.retain(|cover| cover.source != Catalog::MusicBrainz);
        let musicbrainz = match self.release.catalog {
            Catalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            Catalog::Discogs => self.musicbrainz_xref()?,
            other => not_fetched(other),
        };
        let covered_group = musicbrainz
            .as_ref()
            .and_then(|release| release.release_group.as_ref())
            .map(|group| group.id.clone());
        if let Some(release) = musicbrainz {
            let mut gallery = crate::import::cover_art::musicbrainz_gallery(
                &release.id,
                release
                    .release_group
                    .as_ref()
                    .map(|group| group.id.as_str()),
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
        for (catalog, key, _) in self.album_documents()? {
            if catalog != Catalog::MusicBrainz || covered_group.as_deref() == Some(key) {
                continue;
            }
            for cover in crate::import::cover_art::musicbrainz_group_gallery(key).await? {
                crate::import::cover_art::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    /// The keys the pane checks against the library, without building its
    /// tracks or artwork. A source that names no group leaves it absent.
    pub(crate) fn library_check(&self) -> Result<crate::db::LibraryCheck, ImportError> {
        let (release_id, source_group_id) = match self.release.catalog {
            Catalog::MusicBrainz => {
                let release = self.musicbrainz_anchor()?;
                (release.id, release.release_group.map(|group| group.id))
            }
            Catalog::Discogs => {
                let release = self.discogs_anchor()?;
                (release.id, release.master_id)
            }
            other => not_fetched(other),
        };
        Ok(crate::db::LibraryCheck {
            source: self.release.catalog,
            release_id,
            source_group_id,
        })
    }

    /// The picker's detail for this release as the primary of a pick.
    ///
    /// `partners` are the pick's other claimed releases, because the cover
    /// options the detail carries are the pick's, not this document's alone.
    pub fn detail_for_audio(
        &self,
        audio_durations_ms: &[u64],
        partners: &[ReleasePayloads],
    ) -> Result<ImportSearchReleaseDetail, ImportError> {
        let covers = pick_covers(self, partners)?;
        let mut detail = match self.release.catalog {
            Catalog::MusicBrainz => crate::import::search::build_mb_detail(
                &self.release.key,
                &self.musicbrainz_anchor()?,
                covers,
            ),
            Catalog::Discogs => Ok(crate::import::search::build_discogs_detail(
                &self.discogs_anchor()?,
                covers,
                Some(audio_durations_ms),
            )),
            other => not_fetched(other),
        }?;
        let metadata = self.projected_metadata()?;
        detail.title = metadata.album.title;
        detail.artist = (!metadata.album.artists.is_empty()).then(|| {
            metadata
                .album
                .artists
                .iter()
                .map(|artist| artist.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        });
        detail.year = metadata.pressing.year;
        detail.format = metadata.pressing.format;
        detail.label = metadata.pressing.label;
        detail.catalog_number = metadata.pressing.catalog_number;
        detail.country = metadata.pressing.country;
        detail.barcode = metadata.pressing.barcode;
        Ok(detail)
    }

    /// The DB-shape album the commit writes, and the editor's seed is projected
    /// from — the same mapping the fetch path runs, over the same documents.
    ///
    /// `audio_durations_ms` is what the release's audio actually measures, which
    /// is how a Discogs tracklist's index/sub-track layout is chosen. A
    /// MusicBrainz document states its own track times and ignores them.
    pub fn parsed(
        &self,
        audio_durations_ms: &[u64],
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<ParsedAlbum, ImportError> {
        let metadata = self.projected_metadata()?;
        match self.release.catalog {
            Catalog::MusicBrainz => crate::import::musicbrainz_mapper::map_with_metadata(
                &self.musicbrainz_anchor()?,
                metadata,
                clock,
                ids,
            ),
            Catalog::Discogs => crate::import::discogs_mapper::map_with_metadata(
                &self.discogs_anchor()?,
                metadata,
                Some(audio_durations_ms),
                clock,
                ids,
            ),
            other => not_fetched(other),
        }
    }
}

/// The cover options a pick offers, in the order a surface shows them and so
/// the order the default is the first of.
///
/// A pick claims a primary release and, where it names one, that release on
/// each other catalog, and it is picked whole — so its artwork is every
/// claimed document's artwork, not the primary's alone. Every claimed
/// release's own images come first, the primary's first among them, because
/// a pressing's own cover is the one this import wants; the albums' images
/// follow in the same order, each of them being some release of the album's
/// and possibly not this one. An image reachable twice — the release an
/// editor cross-linked to the primary also claimed as a partner, two
/// partners under one master — is offered once.
pub fn pick_covers(
    primary: &ReleasePayloads,
    partners: &[ReleasePayloads],
) -> Result<Vec<RemoteCover>, ImportError> {
    let claimed = || std::iter::once(primary).chain(partners);
    let mut covers = Vec::new();
    for release in claimed() {
        for cover in release.release_covers()? {
            crate::import::cover_art::push_unique_cover(&mut covers, cover);
        }
    }
    for release in claimed() {
        for cover in release.album_covers()? {
            crate::import::cover_art::push_unique_cover(&mut covers, cover);
        }
    }
    Ok(covers)
}

/// The complete galleries behind [`pick_covers`], for the picker: the same
/// claimed documents, each asked what the Cover Art Archive holds for it.
pub async fn pick_gallery_covers(
    primary: &ReleasePayloads,
    partners: &[ReleasePayloads],
) -> Result<Vec<RemoteCover>, ImportError> {
    let mut covers = Vec::new();
    for release in std::iter::once(primary).chain(partners) {
        for cover in release.gallery_covers().await? {
            crate::import::cover_art::push_unique_cover(&mut covers, cover);
        }
    }
    Ok(covers)
}

/// The records the releases one pick claims describe together.
///
/// `claimed` is the primary first — the release the draft is read from — then
/// each partner, paired with whatever documents are archived for it. A claimed
/// release nothing archived documents for still contributes its own record: the
/// pick claims it either way.
///
/// The primary's documents are read first, so what they say about another
/// catalog stands unless that catalog is one the person themselves claimed — a
/// claimed release's own document outranks what an editor cross-linked to it.
/// Only the primary's anchor reads the draft.
pub fn claimed_records(
    claimed: &[(MetadataRef, Option<ReleasePayloads>)],
) -> Result<Vec<ReleaseRecord>, ImportError> {
    let mut records: Vec<ReleaseRecord> = Vec::new();
    for (index, (release, payloads)) in claimed.iter().enumerate() {
        let reads_draft = index == 0;
        let described = match payloads {
            Some(payloads) => payloads.records()?,
            None => vec![ReleaseRecord::new(release, None, reads_draft)],
        };
        for mut record in described {
            let claimed_by_the_person = record.catalog() == release.catalog;
            if let ReleaseRecord::Pressing {
                reads_draft: record_reads,
                ..
            } = &mut record
            {
                *record_reads = reads_draft && claimed_by_the_person;
            }
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
    records.sort_by_key(|record| {
        Catalog::ALL
            .iter()
            .position(|catalog| *catalog == record.catalog())
            .expect("a record names one of the catalogs")
    });
    Ok(records)
}

/// Fetch the selected release and its explicitly related metadata documents.
pub async fn fetch(
    discogs_client: Option<&DiscogsClient>,
    release: &MetadataRef,
    priority: CallPriority,
) -> Result<ReleasePayloads, ImportError> {
    traversal::fetch_documents(discogs_client, release, None, priority).await
}

/// Expand an archived release when the user applies its metadata. Frozen
/// applied snapshots and ordinary reads continue to use their archived set.
pub async fn enrich(
    discogs_client: Option<&DiscogsClient>,
    stored: &ReleasePayloads,
    priority: CallPriority,
) -> Result<ReleasePayloads, ImportError> {
    traversal::fetch_documents(discogs_client, &stored.release, Some(stored), priority).await
}

/// Store a set, replacing whatever was under the same entities.
pub async fn store(
    database: &Database,
    payloads: &ReleasePayloads,
    now: DateTime<Utc>,
) -> Result<(), crate::library::LibraryError> {
    let rows = payloads.rows(now);
    let mut invalidated = std::collections::HashSet::new();
    for row in &rows {
        invalidated.extend(traversal::related_documents(
            row.source,
            &row.source_release_id,
            &row.json,
        )?);
    }
    database
        .replace_release_payloads(&rows, &invalidated.into_iter().collect::<Vec<_>>())
        .await?;
    Ok(())
}

/// The archived documents, read one key at a time.
///
/// The set is assembled by following ids out of documents already in hand, so
/// the keys of one round are only knowable once the round before it has been
/// read — which is why this is a reader rather than a map handed over up
/// front. A caller already inside a read implements it over that read, so the
/// whole set comes out of one consistent view of the table.
pub(crate) trait ArchivedDocuments {
    fn document(&self, source: PayloadSource, id: &str) -> Result<Option<String>, ImportError>;
}

/// The stored set for `release`, or `None` when nothing has fetched it.
///
/// The release's own document is the anchor: without it there is no set, and
/// with it every other key is read out of it — the release group it names, the
/// Discogs release its url-rels point at, the master that release names. No id
/// is guessed and nothing is searched for.
pub async fn load(
    database: &Database,
    release: &MetadataRef,
) -> Result<Option<ReleasePayloads>, ImportError> {
    database.load_release_payloads(release).await
}

/// [`load`]'s assembly, over a reader the caller holds.
pub(crate) fn load_on(
    documents: &impl ArchivedDocuments,
    release: &MetadataRef,
) -> Result<Option<ReleasePayloads>, ImportError> {
    traversal::load_documents(documents, release)
}

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
