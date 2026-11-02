//! A unit test's own MusicBrainz, Discogs and Wikidata fakes. The test builds
//! them, keeps them to seed the answers it wants given, and opens its library
//! with providers that ask them.

use super::providers::Providers;
use bae_catalogs::discogs::client::Discogs;
use bae_catalogs::musicbrainz::MusicBrainz;
use bae_catalogs::wikidata::Wikidata;
use bae_util::http::Http;
use std::sync::Arc;

/// The fakes one test's library asks, each held by name so the test seeds
/// the one it means.
pub(crate) struct FakeCatalogs {
    /// The transport the fakes send on, and the one the providers fetch the
    /// Cover Art Archive's galleries with.
    http: Http,
    musicbrainz: Arc<MusicBrainz>,
    discogs: Arc<Discogs>,
    wikidata: Arc<Wikidata>,
}

impl FakeCatalogs {
    /// Fakes that reach nothing: every request fails fast on a port nothing
    /// listens on, and only what the test seeds is answered.
    pub(crate) fn offline() -> Self {
        Self::over(Http::for_test())
    }

    /// Fakes sending on `http`, a test transport routed to the test's own
    /// servers.
    pub(crate) fn over(http: Http) -> Self {
        Self {
            musicbrainz: Arc::new(MusicBrainz::for_test(http.clone())),
            discogs: Arc::new(Discogs::for_test(http.clone())),
            wikidata: Arc::new(Wikidata::for_test(http.clone())),
            http,
        }
    }

    /// Providers asking these fakes, for the library the test opens.
    pub(crate) fn providers(&self) -> Providers {
        Providers::for_test(
            self.http.clone(),
            self.musicbrainz.clone(),
            self.discogs.clone(),
            self.wikidata.clone(),
        )
    }

    pub(crate) fn musicbrainz(&self) -> &MusicBrainz {
        &self.musicbrainz
    }

    pub(crate) fn discogs(&self) -> &Arc<Discogs> {
        &self.discogs
    }

    pub(crate) fn wikidata(&self) -> &Wikidata {
        &self.wikidata
    }
}
