//! What a pick made of a paired pressing carries: the release its draft is read
//! from, and every other source's record of the same pressing beside it.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn linked_cover_gallery_can_be_empty() {
    let (handle, _tmp, key, _) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();
    let release_id = "70000003";
    seed_discogs_release(handle.library_manager.providers(), release_id);
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let gallery = handle
        .fetch_remote_covers(crate::import::cover_art::CoverTarget::Candidate(key))
        .await
        .unwrap();
    shut_down(handle).await;
    assert_eq!(
        gallery,
        crate::import::cover_art::RemoteCoverGallery::Linked(vec![])
    );
}

/// Picking a paired pressing claims both sources. The partner is stored beside
/// the primary, and its own documents are archived in the same apply — so
/// opening the candidate, importing it, or re-reading its identity later needs
/// no network.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_with_a_partner_stores_it_and_archives_its_documents() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = crate::util::http::serve_not_found().await;
    let (handle, _tmp, key, hash) = pane_fixture_with(
        crate::util::http::Http::for_test().serve("coverartarchive.org", &archive),
    )
    .await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();

    let discogs_release_id = "70000001";
    seed_discogs_release(handle.library_manager.providers(), discogs_release_id);
    let mb_release_id = "partner-mb-rel-1";
    seed_mb_release(handle.library_manager.providers(), mb_release_id, "partner-mb-group-1");
    let partner = crate::import::MetadataRef::new(
        crate::import::Catalog::MusicBrainz,
        mb_release_id.to_string(),
    );

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    discogs_release_id.to_string(),
                ),
                partners: vec![partner.clone()],
            },
        )
        .await
        .unwrap();

    let stored = handle
        .library_manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back");
    assert_eq!(
        stored.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::Discogs,
                discogs_release_id.to_string()
            ),
            partners: vec![partner.clone()],
        }),
        "the partner reads back with the pick that claimed it"
    );

    assert!(
        handle
            .library_manager
            .load_source_release(&partner)
            .await
            .unwrap()
            .is_some(),
        "the partner's own release is stored by the apply"
    );
    assert_eq!(
        handle
            .library_manager
            .load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .expect("the picked candidate is prepared")
            .cover,
        Some(crate::import::CoverSelection::Local("cover.jpg".to_string())),
        "neither claimed release holds an image, so the folder's cover stands"
    );
    shut_down(handle).await;
}

/// A pick naming a partner that will not prepare stores nothing: the
/// provenance the candidate had stands, rather than a pick naming a source
/// with no documents behind it. Here Discogs has no key, so the partner is
/// unreachable while the primary reads fine.
#[tokio::test(flavor = "multi_thread")]
async fn a_partner_that_will_not_prepare_fails_the_apply() {
    let (handle, _tmp, key, hash) = pane_fixture().await;

    let mb_release_id = "unpaired-mb-rel-1";
    seed_mb_release(handle.library_manager.providers(), mb_release_id, "unpaired-mb-group-1");

    let before = handle
        .library_manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back")
        .metadata_provenance;

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    "70000002",
                )],
            },
        )
        .await
        .expect_err("a partner that will not prepare fails the apply");

    assert_eq!(
        handle
            .library_manager
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the candidate row reads back")
            .metadata_provenance,
        before,
        "the failed apply stored nothing"
    );
    shut_down(handle).await;
}

/// A pick names one release per source. A second claim about a source the pick
/// already names is two answers to one question, and it would silently replace
/// the identity the primary document states, so it is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_partner_repeating_the_primary_source_is_refused() {
    let (handle, _tmp, key, hash) = pane_fixture().await;

    let mb_release_id = "repeat-mb-rel-1";
    seed_mb_release(handle.library_manager.providers(), mb_release_id, "repeat-mb-group-1");

    let before = handle
        .library_manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back")
        .metadata_provenance;

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    "repeat-mb-rel-2",
                )],
            },
        )
        .await
        .expect_err("two MusicBrainz releases for one pressing is refused");

    assert_eq!(
        handle
            .library_manager
            .load_import_candidate_state(&hash)
            .await
            .unwrap()
            .expect("the candidate row reads back")
            .metadata_provenance,
        before,
        "the refused apply stored nothing"
    );
    shut_down(handle).await;
}

/// A two-track Discogs release with no master, seeded into the release cache.
fn seed_discogs_release(providers: &crate::providers::Providers, release_id: &str) {
    let raw_release = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": "Album Title",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }, {
            "position": "2", "title": "Track Two", "duration": "0:01", "type_": "track", "artists": []
        }],
    })
    .to_string();
    crate::discogs::client::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    providers.discogs().seed_release_cache(release_id, raw_release);
    providers.discogs().seed_artist_image_response("1", None);
    providers.musicbrainz().seed_discogs_url_lookup(release_id, None);
}

/// A two-track MusicBrainz release, seeded into the caches the fetch path
/// reads, so a partner resolves without a network call.
fn seed_mb_release(
    providers: &crate::providers::Providers,
    release_id: &str,
    release_group_id: &str,
) {
    providers
        .musicbrainz()
        .seed_release_cache(release_id, mb_release_json(release_id, release_group_id));
    providers.musicbrainz().seed_release_group_json_cache(
        release_group_id,
        serde_json::json!({ "id": release_group_id }).to_string(),
    );
}

/// The release endpoint's document for [`seed_mb_release`]'s release.
fn mb_release_json(release_id: &str, release_group_id: &str) -> String {
    let response = crate::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![crate::musicbrainz::MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(crate::musicbrainz::MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![crate::musicbrainz::MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: (1..=2)
                .map(|number| crate::musicbrainz::MbTrack {
                    position: Some(number),
                    number: Some(number.to_string()),
                    title: None,
                    length: None,
                    recording: Some(crate::musicbrainz::MbRecording {
                        id: None,
                        title: Some("Track One".to_string()),
                        artist_credit: vec![],
                        relations: vec![],
                    }),
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    serde_json::to_string(&response).expect("the test response serializes")
}

#[tokio::test(flavor = "multi_thread")]
async fn numeric_vinyl_import_preserves_unknown_sides_and_track_order() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let release_id = "numeric-vinyl-release";
    handle
        .library_manager
        .providers()
        .musicbrainz()
        .seed_release_cache(release_id, serde_json::json!({
        "id": release_id, "title": "Numbered Record",
        "artist-credit": [{ "name": "Record Artist", "artist": { "id": "record-artist", "name": "Record Artist" } }],
        "media": [{ "format": "12\" Vinyl", "tracks": [
            { "position": 1, "number": "9", "recording": { "title": "First Listed" } },
            { "position": 2, "number": "3", "recording": { "title": "Second Listed" } }
        ] }],
        "cover-art-archive": { "front": false, "darkened": false }
    }).to_string());
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    release_id,
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let draft = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    assert_eq!(draft.pressing.format, "12\" Vinyl");
    assert!(draft.tracks.iter().all(|track| track.edit.side.is_none()));
    assert_eq!(
        draft
            .tracks
            .iter()
            .map(|track| track.edit.track_number)
            .collect::<Vec<_>>(),
        [9, 3]
    );
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let tracks = handle
        .library_manager
        .get_tracks_for_release(&release_id)
        .await
        .unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| (track.title.as_str(), track.track_number, track.side))
            .collect::<Vec<_>>(),
        [
            ("First Listed", Some(9), None),
            ("Second Listed", Some(3), None)
        ]
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_commits_what_its_picked_releases_store_now() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = crate::util::http::serve_not_found().await;
    let (handle, _tmp, key, _hash) = pane_fixture_with(
        crate::util::http::Http::for_test().serve("coverartarchive.org", &archive),
    )
    .await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();
    let primary = "70000004";
    let partner = "refetched-partner-release";
    let group = "refetched-partner-group";
    seed_discogs_release(handle.library_manager.providers(), primary);
    seed_mb_release(handle.library_manager.providers(), partner, group);
    let group_json = |allmusic: &str| {
        serde_json::json!({"id":group,"relations":[{"url":{"resource":format!("https://www.allmusic.com/album/{allmusic}")}}]}).to_string()
    };
    handle
        .library_manager
        .providers()
        .musicbrainz()
        .seed_release_group_json_cache(group, group_json("mw111"));
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(crate::import::Catalog::Discogs, primary),
                partners: vec![crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    partner,
                )],
            },
        )
        .await
        .unwrap();
    // The partner is fetched again after the pick, and its album now names
    // another album on AllMusic: a pick references its releases rather than
    // copying them, so the import commits what the partner says now.
    handle
        .library_manager
        .save_source_release(
            &crate::import::payloads::ReleasePayloads::for_test(
                crate::import::MetadataRef::new(crate::import::Catalog::MusicBrainz, partner),
                mb_release_json(partner, group),
                vec![crate::import::SourcePayload::new(
                    crate::import::PayloadSource::MusicBrainzReleaseGroup,
                    group,
                    group_json("mw222"),
                )],
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle
        .library_manager
        .get_release_records(&release_id)
        .await
        .unwrap();
    shut_down(handle).await;
    assert_eq!(
        records
            .iter()
            .find(|record| record.catalog() == crate::import::Catalog::AllMusic)
            .map(|record| record.key()),
        Some("mw222")
    );
}
