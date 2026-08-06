//! The documents one source release's lookups returned, and the projections
//! replayed from them.
//!
//! A release is described by more than its own document: MusicBrainz adds a
//! release group and, where an editor linked one, a Discogs cross-reference
//! with its master. [`ReleasePayloads`] is that whole set, and every shape the
//! import surfaces need — the picker's detail, the editor's seed, the commit's
//! `ParsedAlbum`, the tracklist the Ready rule checks, the cover options the
//! archive serves — is projected from it without touching the network.
//!
//! Each document is stored under the entity it describes, so two releases that
//! share a release group or a Discogs master share its row. The set is
//! reassembled by reading the release's own document and following the ids
//! inside it — the same reading that found them when they were fetched.

use chrono::{DateTime, Utc};

use crate::db::{Database, DbSourceReleasePayload};
use crate::discogs::client::DiscogsClient;
use crate::discogs::DiscogsRelease;
use crate::import::search::{ImportSearchReleaseDetail, SourceTracks};
use crate::import::{
    ImportError, MetadataRef, MetadataSource, ParsedAlbum, PayloadSource, SourcePayload,
};
use crate::musicbrainz::MbReleaseResponse;
use crate::util::rate_limiter::CallPriority;
use tracing::warn;

/// Every document one source release's lookups produced, anchored on the
/// release itself.
///
/// The anchor is a field rather than one entry among the rest, so a value of
/// this type cannot exist without the document it is about — which is what
/// makes holding one mean "identification fetched this release". The supporting
/// documents are each present or not on their own terms: a release with no
/// group and a source with no cross-reference both read the same way they did
/// at fetch time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePayloads {
    release: MetadataRef,
    /// The release's own document, as its source returned it.
    anchor: String,
    supporting: Vec<SourcePayload>,
}

impl ReleasePayloads {
    /// The stored rows for this set, all stamped with one fetch time.
    pub fn rows(&self, now: DateTime<Utc>) -> Vec<DbSourceReleasePayload> {
        std::iter::once(SourcePayload::new(
            PayloadSource::release_of(self.release.source),
            self.release.id.clone(),
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
            metadata_source: self.release.source,
            detail,
        }
    }

    /// The anchoring MusicBrainz release, parsed. Only called down the
    /// MusicBrainz arm of a `self.release.source` match, where the anchor is
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

    /// What the source says about this release's own tracklist — the half of the
    /// Ready rule the folder's probed durations are checked against.
    pub fn source_tracks(&self) -> Result<SourceTracks, ImportError> {
        match self.release.source {
            MetadataSource::MusicBrainz => Ok(crate::import::search::mb_source_tracks(
                &self.musicbrainz_anchor()?,
            )),
            MetadataSource::Discogs => Ok(crate::import::search::discogs_source_tracks(
                &self.discogs_anchor()?,
            )),
        }
    }

    /// The picker's and confirmation pane's display shape.
    pub fn detail(&self) -> Result<ImportSearchReleaseDetail, ImportError> {
        match self.release.source {
            MetadataSource::MusicBrainz => {
                // The cover options come out of the release document itself —
                // it states whether the archive holds a front image, and names
                // the release group whose album-level image is the other
                // offer. Discogs has no counterpart: a Discogs release carries
                // its cover inside its own document, which is where
                // `build_discogs_detail` reads it.
                let anchor = self.musicbrainz_anchor()?;
                let covers = crate::import::cover_art::musicbrainz_covers(&anchor);
                crate::import::search::build_mb_detail(&self.release.id, &anchor, covers)
            }
            MetadataSource::Discogs => Ok(crate::import::search::build_discogs_detail(
                &self.discogs_anchor()?,
            )),
        }
    }

    /// The DB-shape album the commit writes, and the editor's seed is projected
    /// from — the same mapping the fetch path runs, over the same documents.
    pub fn parsed(
        &self,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<ParsedAlbum, ImportError> {
        match self.release.source {
            MetadataSource::MusicBrainz => {
                crate::import::musicbrainz_mapper::map_mb_response_to_db(
                    &self.musicbrainz_anchor()?,
                    None,
                    self.discogs_xref()?,
                    clock,
                    ids,
                )
            }
            MetadataSource::Discogs => {
                let release = self.discogs_anchor()?;
                let master_year = self.discogs_master_year(&release)?;
                crate::import::discogs_mapper::map_discogs_to_db(
                    &release,
                    master_year,
                    self.musicbrainz_xref()?.as_ref(),
                    clock,
                    ids,
                )
            }
        }
    }
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
    let (anchor, supporting) = match release.source {
        MetadataSource::MusicBrainz => {
            fetch_musicbrainz(discogs_client, &release.id, priority).await?
        }
        MetadataSource::Discogs => {
            let client = discogs_client.ok_or(ImportError::DiscogsNotConfigured)?;
            fetch_discogs(client, &release.id, priority).await?
        }
    };
    Ok(ReleasePayloads {
        release: release.clone(),
        anchor,
        supporting,
    })
}

async fn fetch_musicbrainz(
    discogs_client: Option<&DiscogsClient>,
    release_id: &str,
    priority: CallPriority,
) -> Result<(String, Vec<SourcePayload>), ImportError> {
    let fetched = crate::musicbrainz::fetch_release_with_metadata(release_id, priority).await?;
    let mut supporting: Vec<SourcePayload> = fetched.release_group.into_iter().collect();

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
/// without either still describes itself, and the master's only contribution —
/// the original release year — falls back to the release's own.
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
    let anchor_source = PayloadSource::release_of(release.source);
    let mut found = read(database, &[(anchor_source, release.id.clone())]).await?;
    let Some(anchor) = found.pop().map(|payload| payload.json) else {
        return Ok(None);
    };
    // Everything else is keyed by an id read out of a document already in hand,
    // so each round of reads is what makes the next one's keys knowable.
    let (mut documents, discogs_json) = match release.source {
        MetadataSource::MusicBrainz => {
            let response: MbReleaseResponse =
                serde_json::from_str(&anchor).map_err(|e| ImportError::SourceData {
                    metadata_source: MetadataSource::MusicBrainz,
                    detail: format!("stored MusicBrainz release does not parse: {e}"),
                })?;
            let mut keys = Vec::new();
            if let Some(rg) = response.release_group.as_ref() {
                keys.push((PayloadSource::MusicBrainzReleaseGroup, rg.id.clone()));
            }
            if let Some(xref_id) = response
                .discogs_release_url()
                .as_deref()
                .and_then(crate::import::musicbrainz_mapper::extract_discogs_release_id)
            {
                keys.push((PayloadSource::Discogs, xref_id));
            }
            let documents = read(database, &keys).await?;
            // The master is named by the cross-referenced Discogs release.
            let discogs_json = documents
                .iter()
                .find(|d| d.source == PayloadSource::Discogs)
                .map(|d| d.json.clone());
            (documents, discogs_json)
        }
        MetadataSource::Discogs => {
            let documents = read(
                database,
                &[(PayloadSource::MusicBrainzDiscogsXref, release.id.clone())],
            )
            .await?;
            // Here the anchor is the Discogs release, so it names the master.
            (documents, Some(anchor.clone()))
        }
    };

    if let Some(json) = discogs_json {
        let master_id = crate::discogs::client::parse_discogs_release_json(&json)
            .map_err(|e| ImportError::SourceData {
                metadata_source: MetadataSource::Discogs,
                detail: format!("stored Discogs release does not parse: {e}"),
            })?
            .master_id;
        if let Some(master_id) = master_id {
            documents.extend(read(database, &[(PayloadSource::DiscogsMaster, master_id)]).await?);
        }
    }

    Ok(Some(ReleasePayloads {
        release: release.clone(),
        anchor,
        supporting: documents,
    }))
}

async fn read(
    database: &Database,
    keys: &[(PayloadSource, String)],
) -> Result<Vec<SourcePayload>, ImportError> {
    let found = database
        .load_source_release_payloads(keys)
        .await
        .map_err(|e| ImportError::Db(crate::library::LibraryError::Database(e)))?;
    Ok(keys
        .iter()
        .filter_map(|key| {
            found
                .get(key)
                .map(|json| SourcePayload::new(key.0, key.1.clone(), json.clone()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbSourceReleasePayload;
    use coven::{FixedClock, SequentialIdProvider};
    use std::sync::Arc;

    fn now() -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .expect("a valid test instant")
            .with_timezone(&Utc)
    }

    async fn test_database() -> (Database, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().expect("a temp library dir");
        let path = dir.path().join("test.db");
        let database = Database::new_test(
            path.to_str().expect("a UTF-8 temp path"),
            Arc::new(FixedClock(now())),
            Arc::new(SequentialIdProvider::new("payload")),
        )
        .await
        .expect("the test database opens");
        (database, dir)
    }

    async fn archive(database: &Database, rows: &[(PayloadSource, &str, serde_json::Value)]) {
        let rows: Vec<DbSourceReleasePayload> = rows
            .iter()
            .map(|(source, id, json)| DbSourceReleasePayload {
                source: *source,
                source_release_id: (*id).to_string(),
                json: json.to_string(),
                fetched_at: now(),
            })
            .collect();
        database
            .save_source_release_payloads(&rows)
            .await
            .expect("the documents archive");
    }

    fn discogs_release(id: u64, master_id: u64, year: u32) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": "Album Title",
            "year": year,
            "master_id": master_id,
            "artists": [{ "id": 1, "name": "Artist Name" }],
            "tracklist": [
                { "position": "1", "title": "Track Title", "type_": "track", "artists": [] }
            ],
        })
    }

    /// A Discogs release names its master, and the master states the year the
    /// album first came out — 1967 for a 1985 reissue. Reading the set back has
    /// to follow that name out of the *anchor*, which is where a
    /// Discogs-seeded release's own document lives; a reader that only looked
    /// at the supporting documents would find no Discogs release there and
    /// silently fall back to the pressing's own year.
    #[tokio::test]
    async fn a_discogs_release_reaches_its_master_through_the_anchor() {
        let (database, _dir) = test_database().await;
        archive(
            &database,
            &[
                (
                    PayloadSource::Discogs,
                    "12345",
                    discogs_release(12345, 99, 1985),
                ),
                (
                    PayloadSource::DiscogsMaster,
                    "99",
                    serde_json::json!({ "id": 99, "year": 1967 }),
                ),
            ],
        )
        .await;

        let payloads = load(
            &database,
            &MetadataRef::new("12345", MetadataSource::Discogs),
        )
        .await
        .expect("the stored set reads back")
        .expect("the anchor is archived");

        let parsed = payloads
            .parsed(&FixedClock(now()), &SequentialIdProvider::new("album"))
            .expect("the stored documents map");
        assert_eq!(
            parsed.album.year,
            Some(1967),
            "the album year is the master's, not the pressing's"
        );
        assert_eq!(parsed.release.pressing.year, Some(1985));
    }

    /// Nothing archived is not a half-read set: the anchor's absence is the
    /// whole answer, and no supporting key is guessed from a release nobody
    /// fetched.
    #[tokio::test]
    async fn an_unfetched_release_reads_back_as_nothing() {
        let (database, _dir) = test_database().await;
        let payloads = load(
            &database,
            &MetadataRef::new("never-fetched", MetadataSource::MusicBrainz),
        )
        .await
        .expect("the read succeeds");
        assert!(payloads.is_none());
    }
}
