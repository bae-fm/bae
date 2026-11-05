//! The services bae asks about releases, one of each for the app's life.
//!
//! Each owns its rate limit and the answers it has already had, so there is
//! exactly one of each per app: two MusicBrainz objects would each let a
//! request a second through, and answer the same question twice. The app
//! builds [`Providers`] when it starts and hands it to the library, which asks
//! it everything a provider answers. A test builds its own and shares nothing
//! with any other test.

use std::sync::Arc;

use crate::catalog::LookupCatalog;
use crate::discogs::client::{
    Discogs, DiscogsClient, DiscogsError, DiscogsSearchParams, DiscogsValidationObserver,
};
use crate::failure::Failure;
use crate::musicbrainz::{MusicBrainz, ReleaseSearchParams};
use crate::payloads::{DiscIdRelease, ReadDepth, ReleasePayloads};
use crate::rate_limiter::CallPriority;
use crate::records::LookupRef;
use crate::remote_image::{RemoteCover, RemoteImageError};
use crate::search::MetadataResult;
use crate::wikidata::Wikidata;
use crate::CatalogError;
use bae_util::http::Http;
use tracing::warn;

#[derive(Clone)]
pub struct Providers {
    /// The transport every provider sends on, and what the Cover Art Archive
    /// galleries are fetched with — the archive has no rate to keep or answers
    /// worth holding, so it needs nothing of its own.
    http: Http,
    musicbrainz: Arc<MusicBrainz>,
    discogs: Arc<Discogs>,
    wikidata: Arc<Wikidata>,
}

impl Providers {
    pub fn new(http: Http) -> Self {
        Self {
            musicbrainz: Arc::new(MusicBrainz::new(http.clone())),
            discogs: Arc::new(Discogs::new(http.clone())),
            wikidata: Arc::new(Wikidata::new(http.clone())),
            http,
        }
    }

    /// Providers asking the services a test built and keeps: fakes it seeds
    /// with the answers it wants given, or clients whose requests it watches.
    /// `http` is the transport the Cover Art Archive galleries are fetched
    /// with.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(
        http: Http,
        musicbrainz: Arc<MusicBrainz>,
        discogs: Arc<Discogs>,
        wikidata: Arc<Wikidata>,
    ) -> Self {
        Self { http, musicbrainz, discogs, wikidata }
    }

    /// A client asking Discogs with `api_key`, over this app's Discogs
    /// transport. `observer` hears what each call proves about the key.
    pub fn discogs_client(
        &self,
        api_key: String,
        observer: Option<DiscogsValidationObserver>,
    ) -> DiscogsClient {
        match observer {
            Some(observer) => DiscogsClient::with_observer(self.discogs.clone(), api_key, observer),
            None => DiscogsClient::new(self.discogs.clone(), api_key),
        }
    }

    /// The selected release and the documents it links to, as far as `depth`
    /// follows them, asking another catalog than the release's own only when
    /// `asked` names it.
    pub(crate) async fn fetch_payloads(
        &self,
        discogs: Option<&DiscogsClient>,
        asked: &[LookupCatalog],
        release: &LookupRef,
        depth: ReadDepth,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        crate::payloads::fetch_documents(
            &self.musicbrainz,
            &self.wikidata,
            discogs,
            asked,
            release,
            depth,
            priority,
        )
        .await
    }

    /// A release a Disc ID lookup returned, read for an identify run from
    /// that answer, and the documents it links to as far as such a read
    /// follows them, asking Discogs only when `asked` names it.
    pub(crate) async fn read_disc_id_release(
        &self,
        discogs: Option<&DiscogsClient>,
        asked: &[LookupCatalog],
        answered: &DiscIdRelease,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        crate::payloads::read_disc_id_release(
            &self.musicbrainz,
            &self.wikidata,
            discogs,
            asked,
            answered,
            priority,
        )
        .await
    }

    pub(crate) async fn search_musicbrainz(
        &self,
        params: ReleaseSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, CatalogError> {
        crate::search::search_mb(&self.musicbrainz, params, priority).await
    }
}

/// One operation's view of the catalogs: the client for the library's stored
/// Discogs key, when there is one Discogs may be asked with, the catalogs the
/// person has switched on, and the other providers a release fetch follows
/// links into.
pub struct CatalogSession {
    client: Option<Arc<DiscogsClient>>,
    /// The catalogs switched on that this session can ask. A read follows
    /// links into these, and always into its release's own catalog.
    asked: Vec<LookupCatalog>,
    providers: crate::providers::Providers,
}

impl CatalogSession {
    /// A session asking the catalogs of `asked` it can: Discogs only with
    /// `client`.
    pub fn new(
        client: Option<Arc<DiscogsClient>>,
        mut asked: Vec<LookupCatalog>,
        providers: Providers,
    ) -> Self {
        if client.is_none() {
            asked.retain(|catalog| *catalog != LookupCatalog::Discogs);
        }
        Self { client, asked, providers }
    }

    /// The catalogs switched on that this session can ask.
    pub fn asked(&self) -> &[LookupCatalog] {
        &self.asked
    }

    /// Ask one provider for releases matching `params`.
    pub async fn search_musicbrainz(
        &self,
        params: ReleaseSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, CatalogError> {
        self.providers.search_musicbrainz(params, priority).await
    }

    /// The releases MusicBrainz lists under the disc ID `discid`, each with
    /// its document as the answer states it.
    pub async fn lookup_musicbrainz_discid(
        &self,
        discid: &str,
        priority: CallPriority,
    ) -> Result<Vec<(MetadataResult, DiscIdRelease)>, Failure> {
        crate::search::lookup_by_discid(&self.providers.musicbrainz, discid, priority).await
    }

    /// The releases MusicBrainz lists the recordings `isrcs` on.
    pub async fn lookup_musicbrainz_isrcs(
        &self,
        isrcs: &[String],
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, Failure> {
        crate::search::lookup_by_isrcs(&self.providers.musicbrainz, isrcs, priority).await
    }

    pub(crate) async fn search_discogs(
        &self,
        params: DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<crate::search::MetadataResult>, CatalogError> {
        let client = self.client.as_ref().ok_or(CatalogError::DiscogsNotConfigured)?;
        Ok(crate::search::search_discogs(client, params, priority).await?)
    }

    pub async fn fetch_payloads(
        &self,
        release: &LookupRef,
        depth: ReadDepth,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        self.providers
            .fetch_payloads(self.client.as_deref(), &self.asked, release, depth, priority)
            .await
    }

    pub async fn read_disc_id_release(
        &self,
        answered: &DiscIdRelease,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        self.providers
            .read_disc_id_release(self.client.as_deref(), &self.asked, answered, priority)
            .await
    }

    pub async fn release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::remote_image::RemoteCover>, CatalogError> {
        let Some(client) = self.client.as_ref() else {
            return Err(CatalogError::DiscogsNotConfigured);
        };
        let (release, _) = client.get_release(release_id, priority).await?;
        let mut covers = release.covers;
        if let Some(master_id) = release.master_id {
            let (_, json) = client.get_master(&master_id, priority).await?;
            for cover in crate::discogs::documents::parse_discogs_master_covers(&json)? {
                crate::remote_image::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    pub async fn master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::remote_image::RemoteCover>, CatalogError> {
        let client = self.client.as_ref().ok_or(CatalogError::DiscogsNotConfigured)?;
        let (master, _) = client.get_master(master_id, priority).await?;
        Ok(master.covers)
    }

    pub async fn artist_image_url(&self, artist_id: &str) -> Result<Option<String>, CatalogError> {
        let Some(client) = self.client.as_ref() else {
            warn!(
                discogs_artist_id = %artist_id,
                "Discogs has no usable key to ask for this artist's picture, so the artist gets none"
            );
            return Ok(None);
        };
        Ok(client.get_artist_image(artist_id, CallPriority::Interactive).await?)
    }

    /// The archive's images of a MusicBrainz release, then of its group.
    pub async fn musicbrainz_gallery(
        &self,
        release_id: &str,
        group_id: Option<&str>,
    ) -> Result<Vec<RemoteCover>, RemoteImageError> {
        crate::remote_image::musicbrainz_gallery(&self.providers.http, release_id, group_id).await
    }

    pub async fn musicbrainz_group_gallery(
        &self,
        group_id: &str,
    ) -> Result<Vec<RemoteCover>, RemoteImageError> {
        crate::remote_image::musicbrainz_group_gallery(&self.providers.http, group_id).await
    }

    /// Every image one pick's releases offer, the primary's first.
    pub async fn pick_gallery_covers(
        &self,
        primary: &crate::source_release::SourceRelease,
        partners: &[crate::source_release::SourceRelease],
    ) -> Result<Vec<RemoteCover>, CatalogError> {
        crate::source_release::pick_gallery_covers(&self.providers.http, primary, partners).await
    }

    /// What Discogs says of the stored key, or `None` without one to ask
    /// with.
    pub async fn validate(&self) -> Option<DiscogsCheck> {
        let client = self.client.as_ref()?;
        Some(DiscogsCheck::of(client.validate_token(CallPriority::Interactive).await))
    }
}

/// What a token-validation request proves about a key: Discogs accepted it,
/// rejected it, or could not be asked — a failure that says nothing about the
/// key, kept so the person is told why the key went unchecked.
#[derive(Debug)]
pub enum DiscogsCheck {
    Valid,
    Rejected,
    Unchecked(DiscogsError),
}

impl DiscogsCheck {
    /// Read a token-validation request's outcome. A failure that leaves the
    /// key unchecked is warned with its cause.
    pub fn of(result: Result<(), DiscogsError>) -> Self {
        match result {
            Ok(()) => Self::Valid,
            Err(DiscogsError::InvalidApiKey) => Self::Rejected,
            Err(
                error @ (DiscogsError::Transport(_)
                | DiscogsError::Provider { .. }
                | DiscogsError::NotFound
                | DiscogsError::Serialization(_)),
            ) => {
                warn!("Discogs couldn't check the key, which stays unvalidated: {error}");
                Self::Unchecked(error)
            }
        }
    }
}
