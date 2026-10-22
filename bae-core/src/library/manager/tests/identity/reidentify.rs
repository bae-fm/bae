// ── Re-identify: a catalog applied in an edit session, then saved ─────
//
// The applied release is fetched through `prepare_release`, so these tests
// seed the release documents and the cover-art lookups first and it reads
// them instead of hitting the network. Each test's manager has its own
// providers, so what one test seeds no other test sees.

/// The stored release under one source release's own key.
async fn stored_for(
    manager: &LibraryManager,
    catalog: Catalog,
    release_id: &str,
) -> Option<crate::import::source_release::SourceRelease> {
    manager
        .database
        .load_source_release(&crate::import::MetadataRef::new(catalog, release_id))
        .await
        .unwrap()
}

/// Apply `link` to an edit session on `release_id` and save it as applied.
async fn reidentify(manager: &LibraryManager, release_id: &str, link: crate::import::PressingLink) {
    let seed = manager
        .apply_release_edit_catalog(release_id, &link)
        .await
        .unwrap();
    manager
        .save_release_edit(release_id, &seed.edit, None)
        .await
        .unwrap();
}

/// Seed the MusicBrainz release `release_id` of group `group_id`, listing
/// `tracks` tracks, where `prepare_release` reads it.
fn seed_musicbrainz(manager: &LibraryManager, release_id: &str, group_id: &str, tracks: usize) {
    let response = make_mb_release_for_re_identify(release_id, group_id, tracks);
    manager
        .providers()
        .musicbrainz()
        .seed_release_cache(release_id, serde_json::to_string(&response).unwrap());
    manager
        .providers()
        .musicbrainz()
        .seed_release_group_json_cache(group_id, serde_json::json!({ "id": group_id }).to_string());
}

/// [`insert_n_tracks`], each track playing a file of its own, as the editor
/// reads a release's tracks.
async fn insert_editable_tracks(manager: &LibraryManager, release_id: &str, n: usize) {
    insert_n_tracks(&manager.database, release_id, n).await;
    for (index, track) in manager
        .database
        .get_tracks_for_release(release_id)
        .await
        .unwrap()
        .iter()
        .enumerate()
    {
        add_track_audio_sources(
            manager,
            track,
            &[(
                &format!("{index:02}.flac"),
                crate::album_detail::SourceAudioLayout::File,
            )],
        )
        .await;
    }
}

fn musicbrainz_link(release_id: &str) -> crate::import::PressingLink {
    crate::import::PressingLink {
        record: crate::import::MetadataRef::new(Catalog::MusicBrainz, release_id),
        partners: vec![],
    }
}

// ── re_identify_release Exact / Approximate (MB cache-seeded) ────
//
// Drive the network-side `prepare_release` through the MB LRU cache
// (`seed_release_cache` + `seed_release_group_json_cache`) and the cover-art
// client's own so these tests don't hit the network. The caches are
// process-global LRUs, so each test uses a unique MB release ID and no other
// test's seed bleeds in.

/// Build a synthetic MB release response with `n` track rows on a
/// single CD medium, plus a release group reference. Suitable for
/// driving `prepare_release` via cache seeding.
fn make_mb_release_for_re_identify(
    release_id: &str,
    release_group_id: &str,
    track_count: usize,
) -> crate::musicbrainz::MbReleaseResponse {
    use crate::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbMedium, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        disambiguation: None,
        annotation: None,
        date: Some("2024-01-01".to_string()),
        country: None,
        status: None,
        packaging: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            joinphrase: None,
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: Some("2024-01-01".to_string()),
            relations: Some(vec![]),
        }),
        label_info: vec![],
        media: vec![MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: (1..=track_count)
                .map(|n| MbTrack {
                    position: Some(n as i64),
                    number: Some(n.to_string()),
                    title: Some(format!("Track {n}")),
                    length: None,
                    recording: None,
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// Insert `n` plain track rows on the release's one CD. Mirrors the row shape
/// `prepared.parsed.tracks` would produce so the track-count check in
/// `re_identify_release` accepts the picked release.
async fn insert_n_tracks(database: &Database, release_id: &str, n: usize) {
    for i in 1..=n {
        let track = crate::db::DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {i}"),
            medium_id: crate::db::identity::release_medium_id(release_id, 1),
            side: std::num::NonZeroU8::MIN,
            track_number: std::num::NonZeroU32::new(u32::try_from(i).expect("a test track number fits u32"))
                .expect("track numbers count from one"),
            duration_ms: 200_000,
            created_at: Utc::now(),
        };
        database.insert_track(&track).await.unwrap();
    }
}

/// Saving the applied release writes its records, the applied one first, and
/// every value it gives: the tracks take its titles. The picked release is
/// stored under its own key with its release group.
#[tokio::test]
async fn a_saved_reidentify_writes_the_picked_release_s_records_and_values() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager
        .database
        .insert_release_records(&release.id, &[mb_identity("g-old", "mb-rel-old")])
        .await
        .unwrap();
    insert_editable_tracks(&manager, &release.id, 3).await;

    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    seed_musicbrainz(&manager, new_release_id, new_group_id, 3);
    reidentify(&manager, &release.id, musicbrainz_link(new_release_id)).await;

    let records = manager
        .database
        .get_release_records(&release.id)
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key(), new_release_id);
    assert_eq!(
        records[0].album_ref().expect("known parent album").key,
        new_group_id
    );
    let titles: Vec<String> = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap()
        .into_iter()
        .map(|track| track.title)
        .collect();
    assert_eq!(titles, ["Track 1", "Track 2", "Track 3"]);
    let stored = stored_for(&manager, Catalog::MusicBrainz, new_release_id)
        .await
        .expect("the picked release is stored");
    assert_eq!(stored.source_group_id.as_deref(), Some(new_group_id));
    assert!(stored.unfetched.is_empty(), "{:?}", stored.unfetched);
}

/// A release whose tracks the picked release cannot lay out is refused with
/// both counts, before anything is held or written.
#[tokio::test]
async fn a_reidentify_that_does_not_fit_the_stored_tracks_is_refused() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(&manager, &release.id, 10).await;
    let new_release_id = "mismatch-re-identify-mb-rel-new";
    seed_musicbrainz(&manager, new_release_id, "mismatch-re-identify-mb-group-new", 12);

    let err = manager
        .apply_release_edit_catalog(&release.id, &musicbrainz_link(new_release_id))
        .await
        .expect_err("a release of 12 tracks cannot lay out 10");
    let msg = err.to_string();
    assert!(msg.contains("10") && msg.contains("12"), "{msg}");
    assert!(manager
        .database
        .get_release_records(&release.id)
        .await
        .unwrap()
        .is_empty());
}

/// Discarding a session with a catalog applied writes nothing: the library's
/// values, records and album are exactly as they were.
#[tokio::test]
async fn discarding_a_session_restores_the_library_s_values_exactly() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let before = manager.open_release_edit(&release.id).await.unwrap();

    let new_release_id = "discard-re-identify-mb-rel-new";
    seed_musicbrainz(&manager, new_release_id, "discard-re-identify-mb-group-new", 2);
    let applied = manager
        .apply_release_edit_catalog(&release.id, &musicbrainz_link(new_release_id))
        .await
        .unwrap();
    assert_ne!(applied.edit, before.edit, "the apply filled the form");
    manager.discard_release_edit(&release.id);

    assert_eq!(manager.open_release_edit(&release.id).await.unwrap(), before);
    assert!(manager
        .database
        .get_release_records(&release.id)
        .await
        .unwrap()
        .is_empty());
}

/// Re-identifying a MusicBrainz release as a Discogs release and saving
/// leaves none of what MusicBrainz credited: its composers' roles and the
/// works its tracks performed go with it.
#[tokio::test]
async fn a_reidentify_leaves_none_of_the_previous_source_s_roles_or_works() {
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .set_discogs_key("test-discogs-token", crate::config::DiscogsValidation::Valid)
        .await
        .unwrap();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 1).await;
    let track = manager.database.get_tracks_for_release(&release.id).await.unwrap()[0].clone();
    let composer = DbArtist {
        id: Uuid::new_v4().to_string(),
        name: "Composer Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-composer-1".to_string()),
        created_at: Utc::now(),
    };
    manager.database.insert_artist(&composer).await.unwrap();
    let work_id = crate::db::identity::work_id("mb-work-1");
    for (statement, params) in [
        (
            "INSERT INTO works (id, title, musicbrainz_work_id, created_at) \
             VALUES (?, 'Work Title', 'mb-work-1', '2026-01-01T00:00:00Z')",
            vec![work_id.clone()],
        ),
        (
            "INSERT INTO track_works (id, track_id, work_id, position, source, created_at) \
             VALUES ('9a1d3b7e-0000-4000-8000-000000000001', ?, ?, 0, 'musicbrainz', '2026-01-01T00:00:00Z')",
            vec![track.id.clone(), work_id.clone()],
        ),
        (
            "INSERT INTO track_artist_roles (id, track_id, artist_id, position, source, source_credit, created_at) \
             VALUES ('9a1d3b7e-0000-4000-8000-000000000002', ?, ?, 0, 'musicbrainz', 'composer', '2026-01-01T00:00:00Z')",
            vec![track.id.clone(), composer.id.clone()],
        ),
    ] {
        manager.database.execute_for_test(statement, params).await.unwrap();
    }

    let discogs_release_id = "80000201";
    let discogs_raw = serde_json::json!({
        "id": 80000201u64,
        "title": "Album Title",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1", "title": "Track One", "duration": "0:01",
            "type_": "track", "artists": [],
        }],
    })
    .to_string();
    manager.providers().discogs().seed_release_cache(discogs_release_id, discogs_raw);
    manager.providers().discogs().seed_artist_image_response("1", None);
    manager.providers().musicbrainz().seed_discogs_url_lookup(discogs_release_id, None);
    reidentify(
        &manager,
        &release.id,
        crate::import::PressingLink {
            record: crate::import::MetadataRef::new(Catalog::Discogs, discogs_release_id),
            partners: vec![],
        },
    )
    .await;

    let mut left = Vec::new();
    for table in ["track_works", "track_artist_roles"] {
        left.push(
            manager
                .database
                .count_rows_for_test(table, "track_id", &track.id)
                .await
                .unwrap(),
        );
    }
    assert_eq!(left, [0, 0]);
    assert_eq!(
        manager.database.get_tracks_for_release(&release.id).await.unwrap()[0].title,
        "Track One"
    );
}

/// Saving moves the release to the album its new records name; a picked
/// album wins over them and keeps its own title.
#[tokio::test]
async fn saving_moves_the_release_to_the_album_its_records_or_the_person_name() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let grouped = create_test_album();
    let sibling = create_test_release(&grouped.id);
    manager.database.insert_album(&grouped).await.unwrap();
    insert_release(&manager, &sibling).await;
    manager
        .database
        .insert_release_records(&sibling.id, &[mb_identity("move-group", "move-sibling")])
        .await
        .unwrap();

    let own = DbAlbum {
        title: "Own Album".to_string(),
        ..create_test_album()
    };
    let release = create_test_release(&own.id);
    manager.database.insert_album(&own).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 1).await;

    seed_musicbrainz(&manager, "move-new", "move-group", 1);
    reidentify(&manager, &release.id, musicbrainz_link("move-new")).await;
    assert_eq!(
        manager.database.find_album_id_for_release(&release.id).await.unwrap(),
        Some(grouped.id.clone())
    );

    let picked = DbAlbum {
        title: "Picked Album".to_string(),
        ..create_test_album()
    };
    manager.database.insert_album(&picked).await.unwrap();
    let seed = manager.open_release_edit(&release.id).await.unwrap();
    manager
        .save_release_edit(&release.id, &seed.edit, Some(picked.id.clone()))
        .await
        .unwrap();
    assert_eq!(
        manager.database.find_album_id_for_release(&release.id).await.unwrap(),
        Some(picked.id.clone())
    );
    assert_eq!(
        manager.database.find_album_by_id(&picked.id).await.unwrap().unwrap().title,
        "Picked Album",
        "a picked album keeps its own title"
    );
}

#[tokio::test]
async fn re_identify_with_a_partner_writes_both_identity_rows() {
    use crate::import::{Catalog, MetadataRef};
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        ).await
        .unwrap();

    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager
        .database
        .insert_release_records(
            &release.id,
            &[mb_identity("g-old", "partner-re-identify-mb-rel-old")],
        )
        .await
        .unwrap();
    insert_editable_tracks(&manager, &release.id, 1).await;

    let mb_release_id = "partner-re-identify-mb-rel-new";
    let mb_group_id = "partner-re-identify-mb-group-new";
    let response = make_mb_release_for_re_identify(mb_release_id, mb_group_id, 1);
    let raw_json = serde_json::to_string(&response).unwrap();
    manager.providers().musicbrainz().seed_release_cache(mb_release_id, raw_json);
    manager
        .providers()
        .musicbrainz()
        .seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );

    let discogs_release_id = "80000101";
    let discogs_master_id = "80009101";
    manager.providers().discogs().seed_master_cache(
        discogs_master_id,
        serde_json::json!({ "id": discogs_master_id, "year": 1996 }).to_string(),
    );
    let discogs_raw = serde_json::json!({
        "id": discogs_release_id.parse::<u64>().unwrap(),
        "title": "Album Title",
        "year": 1996,
        "master_id": discogs_master_id.parse::<u64>().unwrap(),
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track 1",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }],
    })
    .to_string();
    crate::discogs::client::parse_discogs_release_json(&discogs_raw)
        .expect("the rendered Discogs release parses");
    manager.providers().discogs().seed_release_cache(discogs_release_id, discogs_raw);
    manager.providers().musicbrainz().seed_discogs_url_lookup(discogs_release_id, None);

    reidentify(
        &manager,
        &release.id,
        crate::import::PressingLink {
            record: MetadataRef::new(Catalog::MusicBrainz, mb_release_id),
            partners: vec![MetadataRef::new(Catalog::Discogs, discogs_release_id)],
        },
    )
    .await;

    let identities = manager
        .database
        .get_release_records(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 2, "one row per claimed catalog");

    let mb = identities
        .iter()
        .find(|record| record.catalog() == Catalog::MusicBrainz)
        .expect("the MusicBrainz record");
    assert_eq!(mb.album_ref().expect("known parent album").key, mb_group_id);
    assert_eq!(mb.key(), mb_release_id);

    let discogs = identities
        .iter()
        .find(|record| record.catalog() == Catalog::Discogs)
        .expect("the Discogs record");
    assert_eq!(
        discogs.album_ref().expect("known parent album").key,
        discogs_master_id
    );
    assert_eq!(discogs.key(), discogs_release_id);

    // The partner is stored under its own key, so a later reset or read of
    // that record needs no network.
    assert!(
        stored_for(&manager, Catalog::Discogs, discogs_release_id)
            .await
            .is_some(),
        "the partner's own release is stored by the re-identify"
    );
}
