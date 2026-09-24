//! The services bae asks about releases, one of each for the app's life.
//!
//! Each owns its rate limit and the answers it has already had, so there is
//! exactly one of each per app: two MusicBrainz objects would each let a
//! request a second through, and answer the same question twice. The app
//! builds [`Providers`] when it starts and hands it to the library, which asks
//! it everything a provider answers. A test builds its own and shares nothing
//! with any other test.

use std::sync::Arc;

use crate::discogs::client::{Discogs, DiscogsClient, DiscogsValidationObserver};
use crate::import::cover_art::RemoteCover;
use crate::import::payloads::ReleasePayloads;
use crate::import::search::MetadataResult;
use crate::import::{ImportError, MetadataRef};
use crate::musicbrainz::{MusicBrainz, ReleaseSearchParams};
use crate::signals::failure::LookupFailure;
use crate::util::http::Http;
use crate::util::rate_limiter::CallPriority;
use crate::wikidata::Wikidata;

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

    /// Providers that send on `http` — a test transport routed to the test's
    /// own fakes — and space no requests, since a fake has no rate to keep.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(http: Http) -> Self {
        Self {
            musicbrainz: Arc::new(MusicBrainz::for_test(http.clone())),
            discogs: Arc::new(Discogs::for_test(http.clone())),
            wikidata: Arc::new(Wikidata::for_test(http.clone())),
            http,
        }
    }

    /// Providers that reach nothing: every request fails fast on a port
    /// nothing listens on, and only what a test seeds is answered.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn offline() -> Self {
        Self::for_test(Http::for_test())
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

    /// The selected release and the documents it links to.
    pub(crate) async fn fetch_payloads(
        &self,
        discogs: Option<&DiscogsClient>,
        release: &MetadataRef,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, ImportError> {
        crate::import::payloads::fetch_documents(
            &self.musicbrainz,
            &self.wikidata,
            discogs,
            release,
            None,
            priority,
        )
        .await
    }

    /// An archived release with whatever its documents link to that the
    /// archive does not hold yet.
    pub(crate) async fn enrich_payloads(
        &self,
        discogs: Option<&DiscogsClient>,
        stored: &ReleasePayloads,
        priority: CallPriority,
    ) -> Result<ReleasePayloads, ImportError> {
        crate::import::payloads::fetch_documents(
            &self.musicbrainz,
            &self.wikidata,
            discogs,
            stored.release(),
            Some(stored),
            priority,
        )
        .await
    }

    pub(crate) async fn search_musicbrainz(
        &self,
        params: ReleaseSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, ImportError> {
        crate::import::search::search_mb(&self.musicbrainz, params, priority).await
    }

    pub(crate) async fn read_album_links(
        &self,
        groups: &[String],
        priority: CallPriority,
    ) -> Vec<crate::import::album_links::GroupLinks> {
        crate::import::album_links::read(&self.musicbrainz, groups, priority).await
    }

    pub(crate) async fn lookup_musicbrainz_discid(
        &self,
        discid: &str,
        priority: CallPriority,
    ) -> Result<Vec<MetadataResult>, LookupFailure> {
        crate::import::search::lookup_by_discid(&self.musicbrainz, discid, priority).await
    }

    /// The archive's images of a MusicBrainz release, then of its group.
    pub(crate) async fn musicbrainz_gallery(
        &self,
        release_id: &str,
        group_id: Option<&str>,
    ) -> Result<Vec<RemoteCover>, ImportError> {
        crate::import::cover_art::musicbrainz_gallery(&self.http, release_id, group_id).await
    }

    pub(crate) async fn musicbrainz_group_gallery(
        &self,
        group_id: &str,
    ) -> Result<Vec<RemoteCover>, ImportError> {
        crate::import::cover_art::musicbrainz_group_gallery(&self.http, group_id).await
    }

    /// Every image one pick's releases offer, the primary's first.
    pub(crate) async fn pick_gallery_covers(
        &self,
        primary: &crate::import::source_release::SourceRelease,
        partners: &[crate::import::source_release::SourceRelease],
    ) -> Result<Vec<RemoteCover>, ImportError> {
        crate::import::source_release::pick_gallery_covers(&self.http, primary, partners).await
    }
}

/// What a test reaches past the operations for: the transport to build an
/// image cache on, and the providers whose answers it seeds.
#[cfg(any(test, feature = "test-utils"))]
mod test_access {
    use super::*;

    impl Providers {
        pub fn http(&self) -> &Http {
            &self.http
        }

        pub fn musicbrainz(&self) -> &MusicBrainz {
            &self.musicbrainz
        }

        pub fn discogs(&self) -> &Arc<Discogs> {
            &self.discogs
        }

        pub fn wikidata(&self) -> &Wikidata {
            &self.wikidata
        }
    }
}
