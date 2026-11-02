//! A test's own MusicBrainz, Discogs and Wikidata fakes. The test builds
//! them, seeds the answers it wants given, and opens its library with
//! providers that ask them.

use crate::discogs::client::{Discogs, DiscogsClient};
use crate::musicbrainz::{DiscogsPage, MusicBrainz};
use crate::providers::Providers;
use crate::wikidata::Wikidata;
use bae_util::http::Http;
use std::sync::Arc;

/// The fakes one test's library asks, seeded by name so the test says which
/// catalog answers what.
pub struct FakeCatalogs {
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
    pub fn offline() -> Self {
        Self::over(Http::for_test())
    }

    /// Fakes sending on `http`, a test transport routed to the test's own
    /// servers.
    pub fn over(http: Http) -> Self {
        Self {
            musicbrainz: Arc::new(MusicBrainz::for_test(http.clone())),
            discogs: Arc::new(Discogs::for_test(http.clone())),
            wikidata: Arc::new(Wikidata::for_test(http.clone())),
            http,
        }
    }

    /// Providers asking these fakes, for the library the test opens.
    pub fn providers(&self) -> Providers {
        Providers::for_test(
            self.http.clone(),
            self.musicbrainz.clone(),
            self.discogs.clone(),
            self.wikidata.clone(),
        )
    }

    /// A Discogs client asking these fakes with `api_key`, for one call.
    pub fn discogs_client(&self, api_key: &str) -> DiscogsClient {
        DiscogsClient::new(self.discogs.clone(), api_key.to_string())
    }

    /// MusicBrainz answers release `release_id` with `raw_json`.
    pub fn seed_musicbrainz_release(&self, release_id: &str, raw_json: String) {
        self.musicbrainz.seed_release_cache(release_id, raw_json);
    }

    /// MusicBrainz answers release group `release_group_id` with `raw_json`.
    pub fn seed_musicbrainz_release_group(&self, release_group_id: &str, raw_json: String) {
        self.musicbrainz.seed_release_group_json_cache(release_group_id, raw_json);
    }

    /// MusicBrainz answers its lookup of the Discogs pages in `answers`, each
    /// with the release group linking it or with nothing.
    pub fn seed_discogs_page_lookup(&self, answers: &[(DiscogsPage, Option<&str>)]) {
        self.musicbrainz.seed_discogs_page_lookup(answers);
    }

    /// Discogs answers release `release_id` with `raw_json`.
    pub fn seed_discogs_release(&self, release_id: &str, raw_json: String) {
        self.discogs.seed_release_cache(release_id, raw_json);
    }

    /// Discogs answers master `master_id` with `raw_json`.
    pub fn seed_discogs_master(&self, master_id: &str, raw_json: String) {
        self.discogs.seed_master_cache(master_id, raw_json);
    }

    /// Discogs answers artist `artist_id` with a primary image at
    /// `image_url`, or as an artist it does not have.
    pub fn seed_discogs_artist_image(&self, artist_id: &str, image_url: Option<String>) {
        self.discogs.seed_artist_image_response(artist_id, image_url);
    }

    /// Wikidata answers item `item` with `raw_json`, or as an item it does
    /// not have.
    pub fn seed_wikidata_entity(&self, item: &str, raw_json: Option<String>) {
        self.wikidata.seed_entity_cache(item, raw_json);
    }
}
