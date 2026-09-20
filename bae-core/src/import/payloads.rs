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
use crate::wikidata::WikidataEntity;
use tracing::warn;

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
    supporting: Vec<SourcePayload>,
}

/// The documents and measured track lengths used by one metadata application.
/// Re-reading this value preserves Discogs' selected index/sub-track layout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppliedSource {
    pub payloads: ReleasePayloads,
    pub audio_durations_ms: Vec<u64>,
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

    fn document(&self, source: PayloadSource) -> Option<&str> {
        self.supporting
            .iter()
            .find(|d| d.source == source)
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
        let Some(json) = self.document(PayloadSource::Discogs) else {
            return Ok(None);
        };
        crate::discogs::client::parse_discogs_release_json(json)
            .map(Some)
            .map_err(|e| self.source_data(format!("stored Discogs release does not parse: {e}")))
    }

    /// The MusicBrainz release cross-linked to a Discogs-seeded one.
    fn musicbrainz_xref(&self) -> Result<Option<MbReleaseResponse>, ImportError> {
        let Some(json) = self.document(PayloadSource::MusicBrainzDiscogsXref) else {
            return Ok(None);
        };
        serde_json::from_str(json).map(Some).map_err(|e| {
            self.source_data(format!(
                "stored MusicBrainz cross-reference does not parse: {e}"
            ))
        })
    }

    /// The Wikidata item MusicBrainz's documents named for this release,
    /// parsed. `None` when no editor linked one, and when the fetch that would
    /// have archived it failed — either way the release describes itself with
    /// the records its own catalogs' documents state, and the next
    /// identification asks Wikidata again.
    fn wikidata(&self) -> Result<Option<WikidataEntity>, ImportError> {
        let Some(json) = self.document(PayloadSource::Wikidata) else {
            return Ok(None);
        };
        crate::wikidata::parse_entity(json)
            .map(Some)
            .map_err(|e| self.source_data(format!("stored Wikidata item does not parse: {e}")))
    }

    /// The original release year the Discogs master states, falling back to the
    /// release's own year when no master was archived — the same reading the
    /// fetch path does.
    fn discogs_master_year(&self, release: &DiscogsRelease) -> Result<Option<u32>, ImportError> {
        match self.document(PayloadSource::DiscogsMaster) {
            Some(json) => crate::discogs::client::parse_discogs_master_year(json).map_err(|e| {
                self.source_data(format!("stored Discogs master does not parse: {e}"))
            }),
            None => Ok(release.year),
        }
    }

    /// Every catalog these documents describe the release in: the anchor's own
    /// record, plus one for every catalog page the anchor and its archived
    /// supporting documents link out to.
    ///
    /// The anchor reads the draft — these are the documents its facts come
    /// from. A set fetched for a partner of a pick says the same about its own
    /// anchor, and `records_for_commit` is where that
    /// is settled across the releases one pick claims.
    ///
    /// A link to a catalog the anchor already is, is not followed: the
    /// document in hand outranks what another editor said about it. A group
    /// page contributes its key to that catalog's record and never stands as
    /// one itself — a record names a pressing.
    pub fn records(&self) -> Result<Vec<ReleaseRecord>, ImportError> {
        let (anchor_group, linked) = match self.release.catalog {
            Catalog::MusicBrainz => {
                let response = self.musicbrainz_anchor()?;
                let release_group = response
                    .release_group
                    .as_ref()
                    .map(|group| group.id.clone());
                let mut linked: Vec<CatalogPage> = response
                    .related_urls()
                    .filter_map(parse_catalog_url)
                    .collect();
                // The release group is fetched with its own url-rels, and an
                // editor files a link on whichever of the two it describes.
                if let Some(json) = self.document(PayloadSource::MusicBrainzReleaseGroup) {
                    let group = crate::musicbrainz::parse_release_group(json).map_err(|e| {
                        self.source_data(format!(
                            "stored MusicBrainz release group does not parse: {e}"
                        ))
                    })?;
                    linked.extend(
                        crate::musicbrainz::relation_urls(&group.relations)
                            .filter_map(parse_catalog_url),
                    );
                }
                // A cross-linked Discogs release was fetched along with the
                // anchor, and only that document names the master it belongs
                // to — a url-rel states the release page and nothing above it.
                if let Some(discogs) = self.discogs_xref()? {
                    if let Some(master) = discogs.master_id {
                        linked.push(CatalogPage::Group {
                            catalog: Catalog::Discogs,
                            key: master,
                        });
                    }
                }
                // Wikidata's item for the album carries identifiers for the
                // catalogs MusicBrainz editors leave unlinked. It is read last,
                // so what a document bae fetched itself says about a catalog
                // stands over what the item says about the same one.
                if let Some(item) = self.wikidata()? {
                    linked.extend(item.catalog_pages());
                }
                (release_group, linked)
            }
            Catalog::Discogs => {
                let release = self.discogs_anchor()?;
                let mut linked = Vec::new();
                // Nothing in a Discogs document names a MusicBrainz release;
                // MusicBrainz's own URL endpoint is what found this one.
                if let Some(mb) = self.musicbrainz_xref()? {
                    linked.push(CatalogPage::Release {
                        catalog: Catalog::MusicBrainz,
                        key: mb.id.clone(),
                    });
                    if let Some(group) = mb.release_group.as_ref() {
                        linked.push(CatalogPage::Group {
                            catalog: Catalog::MusicBrainz,
                            key: group.id.clone(),
                        });
                    }
                }
                (release.master_id.clone(), linked)
            }
            other => not_fetched(other),
        };

        let mut records = vec![ReleaseRecord::new(&self.release, anchor_group, true)];
        // A group page contributes its key to the record of the catalog that
        // published it; a record whose group is still its own release has not
        // been told one yet.
        let mut groups: Vec<(Catalog, String)> = Vec::new();
        for page in linked {
            match page {
                CatalogPage::Release { catalog, key } => {
                    if !records.iter().any(|record| record.catalog == catalog) {
                        records.push(ReleaseRecord::new(
                            &MetadataRef::new(catalog, key),
                            None,
                            false,
                        ));
                    }
                }
                CatalogPage::Group { catalog, key } => groups.push((catalog, key)),
            }
        }
        for (catalog, key) in groups {
            if let Some(record) = records
                .iter_mut()
                .find(|record| record.catalog == catalog && record.group_key == record.key)
            {
                record.group_key = key;
            }
        }
        records.sort_by_key(|record| {
            Catalog::ALL
                .iter()
                .position(|catalog| *catalog == record.catalog)
                .expect("a record names one of the catalogs")
        });
        Ok(records)
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

    /// The cover options this release offers, in the order a picker shows
    /// them.
    ///
    /// Include artwork from the anchor and its archived cross-references.
    /// Discogs releases and masters carry their complete image lists.
    pub fn covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = Vec::new();
        match self.release.catalog {
            Catalog::MusicBrainz => {
                covers.extend(crate::import::cover_art::musicbrainz_covers(
                    &self.musicbrainz_anchor()?,
                ));
                if let Some(release) = self.discogs_xref()? {
                    covers.extend(release.covers);
                }
            }
            Catalog::Discogs => {
                covers.extend(self.discogs_anchor()?.covers);
                if let Some(release) = self.musicbrainz_xref()? {
                    covers.extend(crate::import::cover_art::musicbrainz_covers(&release));
                }
            }
            other => not_fetched(other),
        }
        if let Some(json) = self.document(PayloadSource::DiscogsMaster) {
            covers.extend(
                crate::discogs::client::parse_discogs_master_covers(json).map_err(|e| {
                    self.source_data(format!("stored Discogs master images do not parse: {e}"))
                })?,
            );
        }
        let mut unique = Vec::new();
        for cover in covers {
            crate::import::cover_art::push_unique_cover(&mut unique, cover);
        }
        Ok(unique)
    }

    /// On-demand picker artwork. The archived documents supply Discogs images;
    /// the archive supplies the MusicBrainz release and release-group galleries.
    /// This does not change the offline metadata projection or automatic cover.
    pub async fn gallery_covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = self.covers()?;
        covers.retain(|cover| cover.source != Catalog::MusicBrainz);
        let musicbrainz = match self.release.catalog {
            Catalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            Catalog::Discogs => self.musicbrainz_xref()?,
            other => not_fetched(other),
        };
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
        Ok(covers)
    }

    /// The cover a surface offers first for this release, and therefore the one
    /// an import that names no other lands. Read off [`Self::covers`] so the
    /// pane and the commit cannot default to different images.
    pub fn default_cover(&self) -> Result<Option<RemoteCover>, ImportError> {
        Ok(self.covers()?.into_iter().next())
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

    pub fn detail_for_audio(
        &self,
        audio_durations_ms: &[u64],
    ) -> Result<ImportSearchReleaseDetail, ImportError> {
        let covers = self.covers()?;
        match self.release.catalog {
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
        }
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
        match self.release.catalog {
            Catalog::MusicBrainz => crate::import::musicbrainz_mapper::map_mb_response_to_db(
                &self.musicbrainz_anchor()?,
                None,
                self.discogs_xref()?,
                clock,
                ids,
            ),
            Catalog::Discogs => {
                let release = self.discogs_anchor()?;
                let master_year = self.discogs_master_year(&release)?;
                crate::import::discogs_mapper::map_discogs_to_db(
                    &release,
                    master_year,
                    Some(audio_durations_ms),
                    clock,
                    ids,
                )
            }
            other => not_fetched(other),
        }
    }
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
/// The archived documents behind `records`, for the field claims: one set per
/// record whose catalog publishes documents, read through `load`.
///
/// Only the two catalogs bae asks publish documents; the rest are pages a
/// record links out to. One of those two whose document was never fetched —
/// a record another catalog's cross-link named — states nothing either, and
/// says so in the log rather than failing the read.
///
/// The one place the records are turned into documents to compare: the
/// candidate pane reads them on its SQL snapshot, the library editor on its
/// own, and both compare the same set.
pub fn documents_of_records(
    records: &[ReleaseRecord],
    mut load: impl FnMut(&MetadataRef) -> Result<Option<ReleasePayloads>, ImportError>,
) -> Result<Vec<(Catalog, ReleasePayloads)>, ImportError> {
    let mut described = Vec::new();
    for record in records
        .iter()
        .filter(|record| Catalog::LOOKUP.contains(&record.catalog))
    {
        let Some(payloads) = load(&record.release_ref())? else {
            tracing::debug!(
                "no archived {} document for release {}; it states nothing about the fields",
                record.catalog.as_str(),
                record.key
            );
            continue;
        };
        described.push((record.catalog, payloads));
    }
    Ok(described)
}

/// What each of these catalogs' documents says about the eight album-level
/// fields, projected through the same mapping the draft itself is read with —
/// so a claim is exactly what picking that catalog would put in the field.
///
/// The one reading behind every field dot, over the documents
/// [`documents_of_records`] read for a release's records.
pub fn field_claims(
    claimed: &[(Catalog, ReleasePayloads)],
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<crate::import::FieldClaims, ImportError> {
    let readings = claimed
        .iter()
        .map(|(catalog, payloads)| {
            // A claim is about the eight album-level fields, which no tracklist
            // layout touches — so the Discogs layout has nothing to choose
            // between here and needs no measured audio.
            let parsed = payloads.parsed(&[], clock, ids)?;
            let edit = crate::import::parsed_album_to_user_edit(&parsed);
            Ok((*catalog, crate::import::FieldValues::of_edit(&edit)))
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    Ok(crate::import::FieldClaims::of(readings))
}

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
            record.reads_draft = reads_draft && record.catalog == release.catalog;
            let claimed_by_the_person = record.catalog == release.catalog;
            match records
                .iter_mut()
                .find(|existing| existing.catalog == record.catalog)
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
            .position(|catalog| *catalog == record.catalog)
            .expect("a record names one of the catalogs")
    });
    Ok(records)
}

/// Fetch everything `release` needs to be opened, mapped, and committed, from
/// the sources themselves.
///
/// Supporting documents are best-effort exactly where they were before: a
/// release group that will not fetch, a cross-reference an editor never linked,
/// a Discogs key that is not configured. The release's own document is not —
/// without it there is nothing to describe.
pub async fn fetch(
    discogs_client: Option<&DiscogsClient>,
    release: &MetadataRef,
    priority: CallPriority,
) -> Result<ReleasePayloads, ImportError> {
    let (anchor, supporting) = match release.catalog {
        Catalog::MusicBrainz => fetch_musicbrainz(discogs_client, &release.key, priority).await?,
        Catalog::Discogs => {
            let client = discogs_client.ok_or(ImportError::DiscogsNotConfigured)?;
            fetch_discogs(client, &release.key, priority).await?
        }
        other => not_fetched(other),
    };
    Ok(ReleasePayloads {
        release: release.clone(),
        anchor,
        supporting,
    })
}

/// The archived MusicBrainz release-group document among `documents`.
fn archived_release_group(documents: &[SourcePayload]) -> Option<&str> {
    documents
        .iter()
        .find(|d| d.source == PayloadSource::MusicBrainzReleaseGroup)
        .map(|d| d.json.as_str())
}

/// The Wikidata item MusicBrainz names for this release, if an editor linked
/// one.
///
/// An editor files the link on whichever of the release and its group they were
/// looking at, so both documents state it — which is why the group's own
/// document has to be in hand before the item's key is knowable. The fetch and
/// the read-back agree on this reading, so a set reads back with exactly the
/// item it was archived with.
fn wikidata_item(
    release: &MbReleaseResponse,
    release_group_json: Option<&str>,
) -> Result<Option<String>, ImportError> {
    let group = release_group_json
        .map(crate::musicbrainz::parse_release_group)
        .transpose()
        .map_err(|e| ImportError::SourceData {
            catalog: Catalog::MusicBrainz,
            detail: format!("stored MusicBrainz release group does not parse: {e}"),
        })?;
    let group_urls = group
        .iter()
        .flat_map(|group| crate::musicbrainz::relation_urls(&group.relations));
    let item =
        release
            .related_urls()
            .chain(group_urls)
            .find_map(|url| match parse_catalog_url(url) {
                Some(CatalogPage::Release {
                    catalog: Catalog::Wikidata,
                    key,
                }) => Some(key),
                _ => None,
            });
    Ok(item)
}

async fn fetch_musicbrainz(
    discogs_client: Option<&DiscogsClient>,
    release_id: &str,
    priority: CallPriority,
) -> Result<(String, Vec<SourcePayload>), ImportError> {
    let fetched = crate::musicbrainz::fetch_release_with_metadata(release_id, priority).await?;
    let mut supporting: Vec<SourcePayload> = fetched.release_group.into_iter().collect();

    // The Wikidata item is best-effort like the release group: a release
    // archived without it still describes itself in every catalog its own
    // documents name, and the next identification asks for the item again.
    if let Some(item) = wikidata_item(&fetched.response, archived_release_group(&supporting))? {
        match crate::wikidata::fetch_entity(&item, priority).await {
            Ok(json) => supporting.push(SourcePayload::new(PayloadSource::Wikidata, &item, json)),
            Err(e) => warn!("Failed to fetch Wikidata item {item}: {e}"),
        }
    }

    if let (Some(client), Some(url)) = (discogs_client, fetched.discogs_url.as_deref()) {
        if let Some((_release, xref)) =
            crate::discogs::client::fetch_discogs_xref(client, url, priority).await
        {
            supporting.extend(xref);
        }
    }

    Ok((fetched.raw_json, supporting))
}

/// The Discogs release, the master it names, and the MusicBrainz release an
/// editor cross-linked to it.
///
/// The master and the cross-reference are both best-effort: a release archived
/// without either still describes itself. A missing master leaves the release's
/// own year and artwork available.
async fn fetch_discogs(
    client: &DiscogsClient,
    release_id: &str,
    priority: CallPriority,
) -> Result<(String, Vec<SourcePayload>), ImportError> {
    let (release, raw_json) = client.get_release(release_id, priority).await?;
    let mut documents = Vec::new();

    if let Some(master_id) = &release.master_id {
        match client.get_master(master_id, priority).await {
            Ok((_year, master_json)) => documents.push(SourcePayload::new(
                PayloadSource::DiscogsMaster,
                master_id,
                master_json,
            )),
            Err(e) => warn!("Failed to fetch Discogs master {master_id}: {e}"),
        }
    }

    if let Some((_response, xref)) = crate::musicbrainz::fetch_mb_xref(release_id, priority).await {
        documents.extend(xref);
    }
    Ok((raw_json, documents))
}

/// Store a set, replacing whatever was under the same entities.
pub async fn store(
    database: &Database,
    payloads: &ReleasePayloads,
    now: DateTime<Utc>,
) -> Result<(), crate::library::LibraryError> {
    database
        .save_source_release_payloads(&payloads.rows(now))
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
    let read = |keys: &[(PayloadSource, String)]| -> Result<Vec<SourcePayload>, ImportError> {
        let mut found = Vec::with_capacity(keys.len());
        for (source, id) in keys {
            if let Some(json) = documents.document(*source, id)? {
                found.push(SourcePayload::new(*source, id.clone(), json));
            }
        }
        Ok(found)
    };
    let anchor_source = PayloadSource::release_of(release.catalog);
    let Some(anchor) = documents.document(anchor_source, &release.key)? else {
        return Ok(None);
    };
    // Everything else is keyed by an id read out of a document already in hand,
    // so each round of reads is what makes the next one's keys knowable.
    let (mut supporting, discogs_json) = match release.catalog {
        Catalog::MusicBrainz => {
            let response: MbReleaseResponse =
                serde_json::from_str(&anchor).map_err(|e| ImportError::SourceData {
                    catalog: Catalog::MusicBrainz,
                    detail: format!("stored MusicBrainz release does not parse: {e}"),
                })?;
            let mut keys = Vec::new();
            if let Some(rg) = response.release_group.as_ref() {
                keys.push((PayloadSource::MusicBrainzReleaseGroup, rg.id.clone()));
            }
            if let Some(CatalogPage::Release {
                catalog: Catalog::Discogs,
                key,
            }) = response
                .discogs_release_url()
                .as_deref()
                .and_then(parse_catalog_url)
            {
                keys.push((PayloadSource::Discogs, key));
            }
            let mut supporting = read(&keys)?;
            // The item is named by a url-rel on the release or on its group, so
            // the group's document has to be read before its key is knowable.
            if let Some(item) = wikidata_item(&response, archived_release_group(&supporting))? {
                supporting.extend(read(&[(PayloadSource::Wikidata, item)])?);
            }
            // The master is named by the cross-referenced Discogs release.
            let discogs_json = supporting
                .iter()
                .find(|d| d.source == PayloadSource::Discogs)
                .map(|d| d.json.clone());
            (supporting, discogs_json)
        }
        Catalog::Discogs => {
            let supporting = read(&[(PayloadSource::MusicBrainzDiscogsXref, release.key.clone())])?;
            // Here the anchor is the Discogs release, so it names the master.
            (supporting, Some(anchor.clone()))
        }
        other => not_fetched(other),
    };

    if let Some(json) = discogs_json {
        let master_id = crate::discogs::client::parse_discogs_release_json(&json)
            .map_err(|e| ImportError::SourceData {
                catalog: Catalog::Discogs,
                detail: format!("stored Discogs release does not parse: {e}"),
            })?
            .master_id;
        if let Some(master_id) = master_id {
            supporting.extend(read(&[(PayloadSource::DiscogsMaster, master_id)])?);
        }
    }

    Ok(Some(ReleasePayloads {
        release: release.clone(),
        anchor,
        supporting,
    }))
}

#[cfg(test)]
#[path = "payloads_tests.rs"]
mod tests;
