//! Owned provider documents and projections parsed after a database read.
//!
//! Documents are shared by entity key. Stored references select each release's
//! supporting documents without decoding provider JSON on the database worker.

use crate::db::{Database, DbSourceReleasePayload};
use crate::discogs::client::DiscogsClient;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{ImportSearchReleaseDetail, SourceTracks};
use crate::import::{
    ImportError, MetadataRef, MetadataSource, ParsedAlbum, PayloadSource, ReleaseIdentity,
    SourcePayload,
};
use crate::util::rate_limiter::CallPriority;
use chrono::{DateTime, Utc};
use tracing::warn;
#[path = "parsed_payloads.rs"]
mod parsed_payloads;
use parsed_payloads::ParsedDocuments;
pub use parsed_payloads::ParsedReleasePayloads;

/// Raw documents from one read transaction. No parsed provider data or database
/// handle is retained; a processing worker can build every projection from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePayloads {
    release: MetadataRef,
    anchor: String,
    supporting: Vec<SourcePayload>,
    source_group_id: Option<String>,
    document_release_id: String,
}

impl ReleasePayloads {
    pub(crate) fn from_stored(
        release: MetadataRef,
        anchor: String,
        supporting: Vec<SourcePayload>,
        source_group_id: Option<String>,
        document_release_id: String,
    ) -> Self {
        Self {
            release,
            anchor,
            supporting,
            source_group_id,
            document_release_id,
        }
    }

    /// The anchor's optional group ID, normalized when its document was saved.
    /// This is the same optional group used by the picker detail, allowing its
    /// library status to be read in the transaction that fetched these bytes.
    pub(crate) fn source_group_id(&self) -> Option<&str> {
        self.source_group_id.as_deref()
    }

    /// The release ID stated by the document, also used by its parsed detail.
    pub(crate) fn source_release_id(&self) -> &str {
        &self.document_release_id
    }

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
            .find(|document| document.source == source)
            .map(|document| document.json.as_str())
    }

    /// Parse each required document once. All projections of this processing
    /// result borrow the same provider models, including artwork and track lists.
    pub fn parse(&self) -> Result<ParsedReleasePayloads, ImportError> {
        let invalid = |detail| ImportError::SourceData {
            metadata_source: self.release.source,
            detail,
        };
        let musicbrainz = |json: &str| {
            serde_json::from_str(json).map_err(|error| {
                invalid(format!(
                    "stored MusicBrainz release does not parse: {error}"
                ))
            })
        };
        let discogs = |json: &str| {
            crate::discogs::client::parse_discogs_release_json(json)
                .map_err(|error| invalid(format!("stored Discogs release does not parse: {error}")))
        };
        let documents = match self.release.source {
            MetadataSource::MusicBrainz => ParsedDocuments::MusicBrainz {
                release: musicbrainz(&self.anchor)?,
                discogs: self
                    .document(PayloadSource::Discogs)
                    .map(discogs)
                    .transpose()?,
            },
            MetadataSource::Discogs => ParsedDocuments::Discogs {
                release: discogs(&self.anchor)?,
                musicbrainz: self
                    .document(PayloadSource::MusicBrainzDiscogsXref)
                    .map(musicbrainz)
                    .transpose()?,
            },
        };
        let master = self
            .document(PayloadSource::DiscogsMaster)
            .map(|json| {
                crate::discogs::client::parse_discogs_master(json).map_err(|error| {
                    invalid(format!("stored Discogs master does not parse: {error}"))
                })
            })
            .transpose()?;
        Ok(ParsedReleasePayloads { documents, master })
    }

    pub fn identity(&self) -> Result<ReleaseIdentity, ImportError> {
        self.parse()?.identity()
    }

    /// The source tracklist fitted to the folder's measured audio.
    pub fn source_tracks_for_audio(
        &self,
        audio_durations_ms: &[u64],
    ) -> Result<SourceTracks, ImportError> {
        self.parse()?.source_tracks_for_audio(audio_durations_ms)
    }

    pub fn covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        self.parse()?.covers()
    }

    pub fn default_cover(&self) -> Result<Option<RemoteCover>, ImportError> {
        self.parse()?.default_cover()
    }

    pub fn detail_for_audio(
        &self,
        audio_durations_ms: &[u64],
    ) -> Result<ImportSearchReleaseDetail, ImportError> {
        self.parse()?.detail_for_audio(audio_durations_ms)
    }

    /// Map the stored documents using the release's measured audio durations.
    pub fn parsed(
        &self,
        audio_durations_ms: &[u64],
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<ParsedAlbum, ImportError> {
        self.parse()?.parsed(audio_durations_ms, clock, ids)
    }

    pub async fn gallery_covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        self.parse()?.gallery_covers().await
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
    let facts = crate::provider_document::decode(
        PayloadSource::release_of(release.source).as_str(),
        &release.id,
        &anchor,
    )
    .map_err(|detail| ImportError::SourceData {
        metadata_source: release.source,
        detail,
    })?;
    let identity = facts
        .release
        .expect("a release document decoder returns its identity");
    Ok(ReleasePayloads::from_stored(
        release.clone(),
        anchor,
        supporting,
        identity.group_id,
        identity.id,
    ))
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

/// The stored raw set, or `None` when its anchoring release has not been fetched.
pub async fn load(
    database: &Database,
    release: &MetadataRef,
) -> Result<Option<ReleasePayloads>, ImportError> {
    database.load_release_payloads(release).await
}

#[cfg(test)]
#[path = "payloads_tests.rs"]
mod tests;
