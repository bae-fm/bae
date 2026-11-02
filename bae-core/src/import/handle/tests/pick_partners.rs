//! What a pick made of a paired pressing carries: the release its draft is read
//! from, and every other source's record of the same pressing beside it.

use super::*;
use bae_catalogs::musicbrainz::DiscogsPage;

#[tokio::test(flavor = "multi_thread")]
async fn linked_cover_gallery_can_be_empty() {
    let (handle, _tmp, key, _) = pane_fixture().await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000003";
    seed_discogs_release(handle.catalogs.providers(), release_id);
    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let gallery = handle
        .fetch_remote_covers(crate::import::cover_art::CoverTarget::Candidate(key))
        .await
        .unwrap();
    shut_down(handle).await;
    assert_eq!(gallery, crate::import::cover_art::RemoteCoverGallery::Linked(vec![]));
}

/// Picking a paired pressing claims both sources. The partner is stored beside
/// the primary, and its own documents are archived in the same apply — so
/// opening the candidate, importing it, or re-reading its identity later needs
/// no network.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_with_a_partner_stores_it_and_archives_its_documents() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = bae_util::http::serve_not_found().await;
    let (handle, _tmp, key, hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();

    let discogs_release_id = "70000001";
    seed_discogs_release(handle.catalogs.providers(), discogs_release_id);
    let mb_release_id = "partner-mb-rel-1";
    seed_mb_release(handle.catalogs.providers(), mb_release_id, "partner-mb-group-1");
    let partner = crate::catalog_model::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        mb_release_id.to_string(),
    );

    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_release_id.to_string(),
                ),
                partners: vec![partner.clone()],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();

    let stored = handle
        .database
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back");
    assert_eq!(
        stored.release_link,
        Some(crate::import::ReleaseLink::Pressing(crate::identify::PressingLink {
            record: crate::catalog_model::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                discogs_release_id.to_string()
            ),
            partners: vec![partner.clone()],
        })),
        "the partner reads back with the pick that claimed it"
    );

    assert!(
        handle.database.load_source_release(&partner).await.unwrap().is_some(),
        "the partner's own release is stored by the apply"
    );
    assert_eq!(
        crate::import::preparation::import_preparation(&handle.database, &hash)
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
    seed_mb_release(handle.catalogs.providers(), mb_release_id, "unpaired-mb-group-1");

    let before = handle
        .database
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back")
        .metadata_provenance;

    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    "70000002",
                )],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .expect_err("a partner that will not prepare fails the apply");

    assert_eq!(
        handle
            .database
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
    seed_mb_release(handle.catalogs.providers(), mb_release_id, "repeat-mb-group-1");

    let before = handle
        .database
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the candidate row reads back")
        .metadata_provenance;

    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "repeat-mb-rel-2",
                )],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .expect_err("two MusicBrainz releases for one pressing is refused");

    assert_eq!(
        handle
            .database
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

/// A Discogs release under a master, which MusicBrainz's lookup by its
/// address links to a MusicBrainz release, seeded into the caches a read asks.
fn seed_linked_discogs_release(
    providers: &crate::catalog_model::providers::Providers,
) -> crate::catalog_model::records::LookupRef {
    seed_discogs_release_in(providers, "70000007", Some("70000008"), Some("linked-mb-rel-1"));
    seed_mb_release(providers, "linked-mb-rel-1", "linked-mb-group-1");
    crate::catalog_model::records::LookupRef::new(
        bae_catalogs::catalog::LookupCatalog::Discogs,
        "70000007",
    )
}

/// An identify run reads a release only as far as ranking it needs. Picking
/// that release reads the rest before the pick applies it, so the release the
/// pick stores is the one a complete read from scratch stores.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_of_a_release_a_run_read_reads_the_rest() {
    let archive = bae_util::http::serve_not_found().await;
    let (handle, _tmp, key, _) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release = seed_linked_discogs_release(handle.catalogs.providers());

    let read_by_the_run = handle.release_reads.release(
        &release,
        crate::catalog_model::payloads::ReadDepth::Identification,
        bae_catalogs::rate_limiter::CallPriority::Interactive,
    )
    .await
    .unwrap();
    assert!(
        !read_by_the_run
            .release
            .records()
            .iter()
            .any(|record| record.catalog() == bae_catalogs::catalog::Catalog::MusicBrainz),
        "the run's read asks MusicBrainz nothing: {:?}",
        read_by_the_run.release.records()
    );

    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink { record: release.clone(), partners: vec![] },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let picked = handle
        .database
        .load_source_release(&release)
        .await
        .unwrap()
        .expect("the pick stores its release");
    shut_down(handle).await;

    let (scratch, _scratch_tmp, _, _) = pane_fixture().await;
    scratch
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    seed_linked_discogs_release(scratch.catalogs.providers());
    let complete = scratch.release_reads.release(
        &release,
        crate::catalog_model::payloads::ReadDepth::Complete,
        bae_catalogs::rate_limiter::CallPriority::Interactive,
    )
    .await
    .unwrap();
    shut_down(scratch).await;

    assert!(picked.records().iter().any(|record| record.release_ref()
        == Some(&crate::catalog_model::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            "linked-mb-rel-1"
        ))));
    assert_eq!(picked, complete.release);
}

/// A two-track Discogs release with no master, seeded into the release cache.
fn seed_discogs_release(providers: &crate::catalog_model::providers::Providers, release_id: &str) {
    seed_discogs_release_in(providers, release_id, None, None);
}

/// A two-track Discogs release filed under `master`, when it names one,
/// seeded into the release cache; the master's own document is seeded beside
/// it, and MusicBrainz's answer about both addresses: the release's linked
/// from `linked`, when it names a MusicBrainz release, and the master's from
/// no release group.
fn seed_discogs_release_in(
    providers: &crate::catalog_model::providers::Providers,
    release_id: &str,
    master: Option<&str>,
    linked: Option<&str>,
) {
    if let Some(master) = master {
        providers.discogs().seed_master_cache(
            master,
            serde_json::json!({ "id": master.parse::<u64>().unwrap(), "title": "Album Title" })
                .to_string(),
        );
    }
    let raw_release = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "master_id": master.map(|master| master.parse::<u64>().unwrap()),
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
    bae_catalogs::discogs::documents::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    providers.discogs().seed_release_cache(release_id, raw_release);
    providers.discogs().seed_artist_image_response("1", None);
    let lookups: Vec<_> = std::iter::once((DiscogsPage::Release(release_id.to_string()), linked))
        .chain(master.map(|master| (DiscogsPage::Master(master.to_string()), None)))
        .collect();
    providers.musicbrainz().seed_discogs_page_lookup(&lookups);
}

/// A two-track MusicBrainz release, seeded into the caches the fetch path
/// reads, so a partner resolves without a network call.
fn seed_mb_release(
    providers: &crate::catalog_model::providers::Providers,
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
    let response = bae_catalogs::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        disambiguation: None,
        annotation: None,
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        status: None,
        packaging: None,
        barcode: None,
        artist_credit: vec![bae_catalogs::musicbrainz::MbArtistCredit {
            joinphrase: None,
            name: "Artist Name".to_string(),
            artist: Some(bae_catalogs::musicbrainz::MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(bae_catalogs::musicbrainz::MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![bae_catalogs::musicbrainz::MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: (1..=2)
                .map(|number| bae_catalogs::musicbrainz::MbTrack {
                    position: Some(number),
                    number: Some(number.to_string()),
                    title: None,
                    length: None,
                    recording: Some(bae_catalogs::musicbrainz::MbRecording {
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
        cover_art_archive: bae_catalogs::musicbrainz::MbCoverArtArchive {
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
        .catalogs
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
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    release_id,
                ),
                partners: vec![],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let draft = crate::import::preparation::import_preparation(&handle.database, &hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    assert_eq!(
        draft.pressing.facts.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 1).media
    );
    assert_eq!(
        crate::catalog_model::release_values::place_labels(
            &draft.mediums,
            draft.tracks.iter().map(|track| track.place)
        ),
        ["A9", "A3"],
        "one record whose positions name no letter is one side, numbered as printed"
    );
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let tracks = handle.database.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| (track.title.as_str(), track.track_number.get(), track.side.get()))
            .collect::<Vec<_>>(),
        [("First Listed", 9, 1), ("Second Listed", 3, 1)]
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_commits_what_its_picked_releases_store_now() {
    // The pick offers the partner's album address, which the archive holds
    // no image at.
    let archive = bae_util::http::serve_not_found().await;
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let primary = "70000004";
    let partner = "refetched-partner-release";
    let group = "refetched-partner-group";
    seed_discogs_release(handle.catalogs.providers(), primary);
    seed_mb_release(handle.catalogs.providers(), partner, group);
    let group_json = |allmusic: &str| {
        serde_json::json!({"id":group,"relations":[{"url":{"resource":format!("https://www.allmusic.com/album/{allmusic}")}}]}).to_string()
    };
    handle
        .catalogs
        .providers()
        .musicbrainz()
        .seed_release_group_json_cache(group, group_json("mw111"));
    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    primary,
                ),
                partners: vec![crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    partner,
                )],
            },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    // The partner is fetched again after the pick, and its album now names
    // another album on AllMusic: a pick references its releases rather than
    // copying them, so the import commits what the partner says now.
    handle
        .database
        .save_source_release(
            &crate::catalog_model::payloads::ReleasePayloads::for_test(
                crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    partner,
                ),
                mb_release_json(partner, group),
                vec![crate::catalog_model::records::SourcePayload::new(
                    crate::catalog_model::records::PayloadSource::MusicBrainzReleaseGroup,
                    group,
                    group_json("mw222"),
                )],
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    let mut events = handle.every_event();
    let import_id = handle.start_import(&key).await.unwrap().expect_queued();
    let (release_id, _) = await_import_outcome(&mut events, &import_id).await.unwrap();
    let records = handle.database.get_release_records(&release_id).await.unwrap();
    shut_down(handle).await;
    assert_eq!(
        records
            .iter()
            .find(|record| record.catalog() == bae_catalogs::catalog::Catalog::AllMusic)
            .map(|record| record.key()),
        Some("mw222")
    );
}

/// The case a pick has to carry a join its own documents never reach: a run
/// read that a MusicBrainz release group's album is a Discogs master only
/// through one of its pressings — its release links a Discogs release filed
/// under the master — and kept it. A pick of another release of that master
/// names the release group among its records, from what the run kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_carries_the_album_a_reading_found_through_another_pressing() {
    // The pick offers the joined group's album address, which the archive
    // holds no image at.
    let archive = bae_util::http::serve_not_found().await;
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let providers = handle.catalogs.providers();
    let group = "chain-mb-group";
    seed_discogs_release_in(providers, "70000012", Some("909"), None);
    crate::import::identify_service::keep_album_links(
        &handle.database,
        vec![(
            group.to_string(),
            vec![crate::catalog_model::records::MetadataRef::new(bae_catalogs::catalog::Catalog::Discogs, "909")],
        )],
    )
    .await;

    let picked =
        crate::catalog_model::records::LookupRef::new(bae_catalogs::catalog::LookupCatalog::Discogs, "70000012");
    handle
        .select_candidate_release(
            key.clone(),
            crate::identify::PressingLink { record: picked.clone(), partners: vec![] },
            crate::import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let records = handle
        .database
        .load_source_release(&picked)
        .await
        .unwrap()
        .expect("the pick stores its release")
        .records();
    shut_down(handle).await;
    assert!(records.contains(&crate::catalog_model::records::ReleaseRecord::album(
        &crate::catalog_model::records::MetadataRef::new(bae_catalogs::catalog::Catalog::MusicBrainz, group)
    )));
}

/// Steps that keep what a pick told them, `None` where each step ended.
fn recorded() -> (
    crate::import::CatalogApplySteps,
    std::sync::Arc<std::sync::Mutex<Vec<Option<crate::import::CatalogApplyStep>>>>,
) {
    let told = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let keep = told.clone();
    (crate::import::CatalogApplySteps::new(move |step| keep.lock().unwrap().push(step)), told)
}

/// A pick of a paired pressing tells each release it reads from its catalog,
/// the partner after the primary, and the cover it fetches, each ending
/// before the next starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_tells_each_catalog_read_it_waits_on() {
    let archive = bae_util::http::serve_not_found().await;
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let discogs_release_id = "70000021";
    seed_discogs_release(handle.catalogs.providers(), discogs_release_id);
    let mb_release_id = "steps-mb-rel-1";
    seed_mb_release(handle.catalogs.providers(), mb_release_id, "steps-mb-group-1");
    let (steps, told) = recorded();

    handle
        .select_candidate_release(
            key,
            crate::identify::PressingLink {
                record: crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_release_id.to_string(),
                ),
                partners: vec![crate::catalog_model::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    mb_release_id.to_string(),
                )],
            },
            steps,
        )
        .await
        .unwrap();
    shut_down(handle).await;

    assert_eq!(
        *told.lock().unwrap(),
        vec![
            Some(crate::import::CatalogApplyStep::ReadingRelease(
                bae_catalogs::catalog::Catalog::Discogs
            )),
            None,
            Some(crate::import::CatalogApplyStep::ReadingRelease(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
            Some(crate::import::CatalogApplyStep::FetchingCover(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
        ]
    );
}

/// A release the library already holds whole is read without a step: the
/// second pick of one release tells only the cover it fetches.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_of_a_held_release_tells_no_read() {
    let archive = bae_util::http::serve_not_found().await;
    let (handle, _tmp, key, _hash) =
        pane_fixture_with(bae_util::http::Http::for_test().serve("coverartarchive.org", &archive))
            .await;
    let mb_release_id = "steps-held-mb-rel-1";
    seed_mb_release(handle.catalogs.providers(), mb_release_id, "steps-held-mb-group-1");
    let link = crate::identify::PressingLink {
        record: crate::catalog_model::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            mb_release_id.to_string(),
        ),
        partners: vec![],
    };
    let (first, first_told) = recorded();
    handle.select_candidate_release(key.clone(), link.clone(), first).await.unwrap();
    let (again, again_told) = recorded();

    handle.select_candidate_release(key, link, again).await.unwrap();
    shut_down(handle).await;

    assert_eq!(
        *first_told.lock().unwrap(),
        vec![
            Some(crate::import::CatalogApplyStep::ReadingRelease(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
            Some(crate::import::CatalogApplyStep::FetchingCover(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
        ]
    );
    assert_eq!(
        *again_told.lock().unwrap(),
        vec![
            Some(crate::import::CatalogApplyStep::FetchingCover(
                bae_catalogs::catalog::Catalog::MusicBrainz
            )),
            None,
        ]
    );
}
