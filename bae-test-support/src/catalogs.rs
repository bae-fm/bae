//! The catalog fakes a test keeps: built here, seeded by the test with
//! [`crate::seed_discogs_test_release`] and [`crate::seed_mb_release`], and
//! asked by the library the test opens with [`providers_asking`] them.

use bae_catalogs::discogs::client::Discogs;
use bae_catalogs::musicbrainz::MusicBrainz;
use bae_catalogs::wikidata::Wikidata;
use bae_util::http::Http;
use std::sync::Arc;

/// A MusicBrainz fake that reaches nothing: every request fails fast on a
/// port nothing listens on, and only what the test seeds is answered.
pub fn offline_musicbrainz() -> Arc<MusicBrainz> {
    Arc::new(MusicBrainz::for_test(Http::for_test()))
}

/// A Discogs fake that reaches nothing, as [`offline_musicbrainz`] does.
pub fn offline_discogs() -> Arc<Discogs> {
    Arc::new(Discogs::for_test(Http::for_test()))
}

/// Providers asking `musicbrainz` and `discogs`, a test's own fakes, and a
/// Wikidata nothing seeds. The Cover Art Archive's galleries are fetched over
/// a transport that reaches nothing.
pub fn providers_asking(
    musicbrainz: &Arc<MusicBrainz>,
    discogs: &Arc<Discogs>,
) -> bae_core::catalog_model::providers::Providers {
    let http = Http::for_test();
    bae_core::catalog_model::providers::Providers::for_test(
        http.clone(),
        musicbrainz.clone(),
        discogs.clone(),
        Arc::new(Wikidata::for_test(http)),
    )
}
