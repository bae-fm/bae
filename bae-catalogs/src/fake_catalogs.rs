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

    /// Put `response` and a minimal release-group document where a lookup for
    /// either will find them, so neither touches the network, and return the
    /// release id. The release document is `response`'s own serialization, so what
    /// the import parses and what it archives cannot disagree.
    pub fn seed_musicbrainz_response(
        &self,
        response: crate::musicbrainz::MbReleaseResponse,
        release_group_id: &str,
    ) -> String {
        let release_id = response.id.clone();
        let raw_json = serde_json::to_string(&response).expect("the test response serializes");
        self.musicbrainz.seed_release_cache(&release_id, raw_json);
        self.musicbrainz.seed_release_group_json_cache(
            release_group_id,
            serde_json::json!({ "id": release_group_id }).to_string(),
        );
        release_id
    }

    /// Put a synthetic test release's documents — the release, its master (if it
    /// carries a `master_id`), its artists, and MusicBrainz's answer about a
    /// back-link — where the import's requests will look, and return the release id
    /// the import should pick. The worker's `prepare_release` → `client.get_release`
    /// chain reads the seeded response; the cross-reference call resolves to "no MB
    /// link" without touching the network.
    ///
    /// `release` describes what the test wants; this renders it as the release
    /// endpoint's own JSON and reads it back through the production parser, so the
    /// two halves of the cache entry cannot disagree. That matters because the raw
    /// JSON is what the import archives, and every later projection of the release
    /// — a reset, a re-open — replays from the archived bytes rather than from
    /// whatever a test handed over alongside them.
    ///
    /// The rendered ids are numeric, as the endpoint's are, so the returned id is
    /// not the (arbitrary) one the caller wrote on the fixture.
    pub fn seed_discogs_test_release(&self, release: crate::discogs::DiscogsRelease) -> String {
        let numeric = |value: &str| -> u64 {
            crate::test_releases::discogs_fixture_id(value)
                .parse()
                .expect("a rendered Discogs id is numeric")
        };
        let credit = |artist: &crate::discogs::DiscogsArtist| {
            serde_json::json!({
                "id": numeric(&artist.id),
                "name": artist.name,
                "anv": artist.anv.clone().unwrap_or_default(),
                "join": artist.join.clone().unwrap_or_default(),
            })
        };
        let role_credit = |artist: &crate::discogs::DiscogsRoleArtist| {
            serde_json::json!({
                "id": artist.id.as_deref().map(numeric),
                "name": artist.name,
                "role": artist.role,
                "anv": artist.credited_name,
                "tracks": artist.tracks,
            })
        };
        let master_id = release.master_id.as_deref().map(numeric);

        for artist_id in crate::test_releases::discogs_fixture_artist_ids(&release) {
            self.discogs.seed_artist_image_response(&numeric(&artist_id).to_string(), None);
        }

        if let Some(master_id) = master_id {
            // Keyed by the rendered id, which is the one the parsed release names
            // and therefore the one the master fetch asks for.
            let master_json = serde_json::json!({ "id": master_id, "year": release.year });
            self.discogs.seed_master_cache(&master_id.to_string(), master_json.to_string());
        }

        let raw_json = serde_json::json!({
            "id": numeric(&release.id),
            "title": release.title,
            "year": release.year,
            "country": release.country,
            "master_id": master_id,
            "identifiers": release.barcodes.iter().map(|value| serde_json::json!({ "type": "Barcode", "value": value }))
                // The projection keeps no identifier's kind; every kind but Barcode reads back the same.
                .chain(release.identifiers.iter().map(|value| serde_json::json!({ "type": "Other", "value": value })))
                .collect::<Vec<_>>(),
            "notes": release.notes,
            "companies": release.companies.iter().map(|name| serde_json::json!({ "name": name })).collect::<Vec<_>>(),
            "formats": release.formats.iter().map(|format| serde_json::json!({
                "name": format.name,
                "qty": format.qty,
                "descriptions": format.descriptions,
                "text": format.text,
            })).collect::<Vec<_>>(),
            "labels": release.labels.iter().map(|label| serde_json::json!({
                "name": label.name(),
                "catno": label.catalog_number(),
            })).collect::<Vec<_>>(),
            "images": release.covers.iter().enumerate().map(|(index, cover)| serde_json::json!({
                "type": if index == 0 { "primary" } else { "secondary" },
                "uri": cover.image.url,
                "uri150": cover.image.downscaled.first().map(|copy| &copy.url),
            })).collect::<Vec<_>>(),
            "artists": release.artists.iter().map(credit).collect::<Vec<_>>(),
            "extraartists": release.extraartists.as_ref().map(|artists| {
                artists.iter().map(role_credit).collect::<Vec<_>>()
            }),
            "tracklist": release.tracklist.iter().map(|track| serde_json::json!({
                "position": track.position,
                "title": track.title,
                "duration": track.duration,
                "type_": track.type_,
                "artists": track.artists.iter().map(credit).collect::<Vec<_>>(),
                "extraartists": track.extraartists.as_ref().map(|artists| {
                    artists.iter().map(role_credit).collect::<Vec<_>>()
                }),
            })).collect::<Vec<_>>(),
        })
        .to_string();

        let parsed = crate::discogs::documents::parse_discogs_release_json(&raw_json)
            .expect("the rendered test release parses as the endpoint's own JSON");
        let id = parsed.id.clone();
        self.discogs.seed_release_cache(&id, raw_json);
        // A complete read asks MusicBrainz about the release's address and its
        // master's in one request; neither is linked from anything there.
        let lookups: Vec<_> = std::iter::once(crate::musicbrainz::DiscogsPage::Release(id.clone()))
            .chain(
                master_id.map(|master| crate::musicbrainz::DiscogsPage::Master(master.to_string())),
            )
            .map(|page| (page, None))
            .collect();
        self.musicbrainz.seed_discogs_page_lookup(&lookups);
        id
    }
}
