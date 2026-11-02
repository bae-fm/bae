//! The services bae asks about releases, one of each for the app's life.
//!
//! Each owns its rate limit and the answers it has already had, so there is
//! exactly one of each per app: two MusicBrainz objects would each let a
//! request a second through, and answer the same question twice. The app
//! builds [`Providers`] when it starts and hands it to the library, which asks
//! it everything a provider answers. A test builds its own and shares nothing
//! with any other test.

use std::sync::Arc;

use crate::catalog_model::failure::Failure;
use crate::catalog_model::payloads::{DiscIdRelease, ReadDepth, ReleasePayloads};
use crate::catalog_model::records::LookupRef;
use crate::catalog_model::search::MetadataResult;
use crate::catalog_model::CatalogError;
use bae_catalogs::discogs::client::{
    Discogs, DiscogsClient, DiscogsError, DiscogsSearchParams, DiscogsValidationObserver,
};
use bae_catalogs::musicbrainz::{MusicBrainz, ReleaseSearchParams};
use bae_catalogs::rate_limiter::CallPriority;
use bae_catalogs::remote_image::{RemoteCover, RemoteImageError};
use bae_catalogs::wikidata::Wikidata;
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

    /// Providers that reach nothing and answer nothing: every request fails
    /// fast on a port nothing listens on, and no test holds their fakes to
    /// seed.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn offline() -> Self {
        let http = Http::for_test();
        Self::for_test(
            http.clone(),
            Arc::new(MusicBrainz::for_test(http.clone())),
            Arc::new(Discogs::for_test(http.clone())),
            Arc::new(Wikidata::for_test(http)),
        )
    }

    /// A client asking Discogs with `api_key`, over this app's Discogs
    /// transport. `observer` hears what each call proves about the key.
    pub(crate) fn discogs_client(
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
    /// follows them.
    pub(crate) async fn fetch_payloads(
        &self,
        discogs: Option<&DiscogsClient>,
        release: &LookupRef,
        depth: ReadDepth,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        crate::catalog_model::payloads::fetch_documents(
            &self.musicbrainz,
            &self.wikidata,
            discogs,
            release,
            depth,
            priority,
        )
        .await
    }

    /// A release a Disc ID lookup returned, read for an identify run from
    /// that answer, and the documents it links to as far as such a read
    /// follows them.
    pub(crate) async fn read_disc_id_release(
        &self,
        discogs: Option<&DiscogsClient>,
        answered: &DiscIdRelease,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        crate::catalog_model::payloads::read_disc_id_release(
            &self.musicbrainz,
            &self.wikidata,
            discogs,
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
        crate::catalog_model::search::search_mb(&self.musicbrainz, params, priority).await
    }
}

/// What a test reaches past the operations for: the transport to build an
/// image cache on.
#[cfg(any(test, feature = "test-utils"))]
mod test_access {
    use super::*;

    impl Providers {
        pub fn http(&self) -> &Http {
            &self.http
        }
    }
}

/// One operation's view of the catalogs: the client for the library's stored
/// Discogs key, when there is one Discogs may be asked with, and the other
/// providers a release fetch follows links into.
pub struct CatalogSession {
    client: Option<Arc<DiscogsClient>>,
    providers: crate::catalog_model::providers::Providers,
}

impl CatalogSession {
    pub fn new(client: Option<Arc<DiscogsClient>>, providers: Providers) -> Self {
        Self { client, providers }
    }

    /// Whether Discogs may be asked: this library holds a key it was not told
    /// is rejected.
    pub fn asks_discogs(&self) -> bool {
        self.client.is_some()
    }

    /// Ask one provider for releases matching `params`.
    pub(crate) async fn search_musicbrainz(
        &self,
        params: ReleaseSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, CatalogError> {
        self.providers.search_musicbrainz(params, priority).await
    }

    /// The releases MusicBrainz lists under the disc ID `discid`, each with
    /// its document as the answer states it.
    pub(crate) async fn lookup_musicbrainz_discid(
        &self,
        discid: &str,
        priority: CallPriority,
    ) -> Result<Vec<(MetadataResult, DiscIdRelease)>, Failure> {
        crate::catalog_model::search::lookup_by_discid(
            &self.providers.musicbrainz,
            discid,
            priority,
        )
        .await
    }

    /// The releases MusicBrainz lists the recordings `isrcs` on.
    pub(crate) async fn lookup_musicbrainz_isrcs(
        &self,
        isrcs: &[String],
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, Failure> {
        crate::catalog_model::search::lookup_by_isrcs(&self.providers.musicbrainz, isrcs, priority)
            .await
    }

    pub(crate) async fn search_discogs(
        &self,
        params: DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<crate::catalog_model::search::MetadataResult>, CatalogError> {
        let client = self.client.as_ref().ok_or(CatalogError::DiscogsNotConfigured)?;
        Ok(crate::catalog_model::search::search_discogs(client, params, priority).await?)
    }

    pub(crate) async fn fetch_payloads(
        &self,
        release: &LookupRef,
        depth: ReadDepth,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        self.providers.fetch_payloads(self.client.as_deref(), release, depth, priority).await
    }

    pub(crate) async fn read_disc_id_release(
        &self,
        answered: &DiscIdRelease,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, CatalogError> {
        self.providers.read_disc_id_release(self.client.as_deref(), answered, priority).await
    }

    pub(crate) async fn release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, CatalogError> {
        let Some(client) = self.client.as_ref() else {
            return Err(CatalogError::DiscogsNotConfigured);
        };
        let (release, _) = client.get_release(release_id, priority).await?;
        let mut covers = release.covers;
        if let Some(master_id) = release.master_id {
            let (_, json) = client.get_master(&master_id, priority).await?;
            for cover in bae_catalogs::discogs::documents::parse_discogs_master_covers(&json)? {
                bae_catalogs::remote_image::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    pub(crate) async fn master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, CatalogError> {
        let client = self.client.as_ref().ok_or(CatalogError::DiscogsNotConfigured)?;
        let (master, _) = client.get_master(master_id, priority).await?;
        Ok(master.covers)
    }

    pub(crate) async fn artist_image_url(
        &self,
        artist_id: &str,
    ) -> Result<Option<String>, CatalogError> {
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
    pub(crate) async fn musicbrainz_gallery(
        &self,
        release_id: &str,
        group_id: Option<&str>,
    ) -> Result<Vec<RemoteCover>, RemoteImageError> {
        bae_catalogs::remote_image::musicbrainz_gallery(&self.providers.http, release_id, group_id)
            .await
    }

    pub(crate) async fn musicbrainz_group_gallery(
        &self,
        group_id: &str,
    ) -> Result<Vec<RemoteCover>, RemoteImageError> {
        bae_catalogs::remote_image::musicbrainz_group_gallery(&self.providers.http, group_id).await
    }

    /// Every image one pick's releases offer, the primary's first.
    pub(crate) async fn pick_gallery_covers(
        &self,
        primary: &crate::catalog_model::source_release::SourceRelease,
        partners: &[crate::catalog_model::source_release::SourceRelease],
    ) -> Result<Vec<RemoteCover>, CatalogError> {
        crate::catalog_model::source_release::pick_gallery_covers(
            &self.providers.http,
            primary,
            partners,
        )
        .await
    }

    /// What Discogs says of the stored key, or `None` without one to ask
    /// with.
    pub(crate) async fn validate(&self) -> Option<DiscogsCheck> {
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
