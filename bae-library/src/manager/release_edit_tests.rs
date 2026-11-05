//! A release's edit sessions: the seed an editor opens on, a catalog applied,
//! the album picked, re-identifying, and a save refused once the release
//! changed.

use super::*;
use crate::manager::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::LookupRef;
use bae_records::DbAlbum;
use chrono::Utc;
use uuid::Uuid;

// ── Re-identify: a catalog applied in an edit session, then saved ─────
//
// The applied release is fetched through `prepare_release`, so these tests
// seed the release documents and the cover-art lookups first and it reads
// them instead of hitting the network. Each test's manager has its own
// providers, so what one test seeds no other test sees.

/// The stored release under one source release's own key.
async fn stored_for(
    manager: &LibraryManager,
    catalog: LookupCatalog,
    release_id: &str,
) -> Option<bae_catalogs::source_release::SourceRelease> {
    bae_import::tables::ImportTables::new(manager.store.clone())
        .load_source_release(&bae_catalogs::records::LookupRef::new(catalog, release_id))
        .await
        .unwrap()
}

/// Apply `link` in an edit session of its own on `release_id` and save it as
/// applied.
async fn reidentify(manager: &LibraryManager, release_id: &str, link: bae_identify::PressingLink) {
    let session = manager.open_release_edit(release_id).await.unwrap();
    manager
        .apply_release_edit_catalog(&session, &link, &bae_import::CatalogApplySteps::unread())
        .await
        .unwrap();
    manager.save_release_edit(&session).await.unwrap();
}

/// Seed the MusicBrainz release `release_id` of group `group_id`, listing
/// `tracks` tracks, where `prepare_release` reads it.
fn seed_musicbrainz(manager: &TestManager, release_id: &str, group_id: &str, tracks: usize) {
    let response = make_mb_release_for_re_identify(release_id, group_id, tracks);
    manager.fakes.seed_musicbrainz_release(release_id, serde_json::to_string(&response).unwrap());
    manager.fakes.seed_musicbrainz_release_group(
        group_id,
        serde_json::json!({ "id": group_id }).to_string(),
    );
}

/// [`insert_n_tracks`], each track playing a file of its own, as the editor
/// reads a release's tracks.
async fn insert_editable_tracks(manager: &LibraryManager, release_id: &str, n: usize) {
    insert_n_tracks(&manager.records, release_id, n).await;
    for (index, track) in
        manager.records.get_tracks_for_release(release_id).await.unwrap().iter().enumerate()
    {
        add_track_audio_sources(
            manager,
            track,
            &[(&format!("{index:02}.flac"), bae_records::source_audio::SourceAudioLayout::File)],
        )
        .await;
    }
}

fn musicbrainz_link(release_id: &str) -> bae_identify::PressingLink {
    bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, release_id),
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
) -> bae_catalogs::musicbrainz::MbReleaseResponse {
    use bae_catalogs::musicbrainz::{
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
        cover_art_archive: bae_catalogs::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// Saving the applied release writes its records, the applied one first, and
/// every value it gives: the tracks take its titles. No row names the picked
/// release, so it is read and never stored.
#[tokio::test]
async fn a_saved_reidentify_writes_the_picked_release_s_records_and_values() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager
        .records
        .insert_release_records(&release.id, &[mb_identity("g-old", "mb-rel-old")])
        .await
        .unwrap();
    insert_editable_tracks(&manager, &release.id, 3).await;

    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    seed_musicbrainz(&manager, new_release_id, new_group_id, 3);
    reidentify(&manager, &release.id, musicbrainz_link(new_release_id)).await;

    let records = manager.records.get_release_records(&release.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].key(), new_release_id);
    assert_eq!(records[0].album_ref().expect("known parent album").key, new_group_id);
    let titles: Vec<String> = manager
        .records
        .get_tracks_for_release(&release.id)
        .await
        .unwrap()
        .into_iter()
        .map(|track| track.title)
        .collect();
    assert_eq!(titles, ["Track 1", "Track 2", "Track 3"]);
    assert_eq!(
        stored_for(&manager, LookupCatalog::MusicBrainz, new_release_id).await,
        None,
        "nothing names the picked release, so nothing stores it"
    );
}

/// A release whose tracks the picked release cannot lay out is refused with
/// both counts, before anything is held or written.
#[tokio::test]
async fn a_reidentify_that_does_not_fit_the_stored_tracks_is_refused() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(&manager, &release.id, 10).await;
    let new_release_id = "mismatch-re-identify-mb-rel-new";
    seed_musicbrainz(&manager, new_release_id, "mismatch-re-identify-mb-group-new", 12);

    let session = manager.open_release_edit(&release.id).await.unwrap();
    let err = manager
        .apply_release_edit_catalog(
            &session,
            &musicbrainz_link(new_release_id),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .expect_err("a release of 12 tracks cannot lay out 10");
    assert_eq!(
        err.category(),
        bae_diagnostics::UiErrorCategory::MetadataTrackCount {
            folder_tracks: 10,
            release_tracks: 12
        },
        "the editor names the refusal, not a generic import failure: {err}"
    );
    assert!(manager.records.get_release_records(&release.id).await.unwrap().is_empty());
}

/// Discarding a session with a catalog applied writes nothing: the library's
/// values, records and album are exactly as they were.
#[tokio::test]
async fn discarding_a_session_restores_the_library_s_values_exactly() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let session = manager.open_release_edit(&release.id).await.unwrap();
    let before = session.opened().clone();

    let new_release_id = "discard-re-identify-mb-rel-new";
    seed_musicbrainz(&manager, new_release_id, "discard-re-identify-mb-group-new", 2);
    let applied = manager
        .apply_release_edit_catalog(
            &session,
            &musicbrainz_link(new_release_id),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    assert_ne!(applied.edit, before.edit, "the apply filled the form");
    drop(session);

    assert_eq!(manager.open_release_edit(&release.id).await.unwrap().opened(), &before);
    assert!(manager.records.get_release_records(&release.id).await.unwrap().is_empty());
}

/// Re-identifying a MusicBrainz release as a Discogs release and saving
/// leaves none of what MusicBrainz credited: its composers' roles and the
/// works its tracks performed go with it.
#[tokio::test]
async fn a_reidentify_leaves_none_of_the_previous_source_s_roles_or_works() {
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 1).await;
    let track = manager.records.get_tracks_for_release(&release.id).await.unwrap()[0].clone();
    let composer = DbArtist {
        id: Uuid::new_v4().to_string(),
        name: "Composer Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-composer-1".to_string()),
        created_at: Utc::now(),
    };
    manager.records.insert_artist(&composer).await.unwrap();
    let work_id = bae_records::identity::work_id("mb-work-1");
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
        manager.store.execute_for_test(statement, params).await.unwrap();
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
    manager.fakes.seed_discogs_release(discogs_release_id, discogs_raw);
    manager.fakes.seed_discogs_artist_image("1", None);
    manager.fakes.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release(discogs_release_id.to_string()),
        None,
    )]);
    reidentify(
        &manager,
        &release.id,
        bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                LookupCatalog::Discogs,
                discogs_release_id,
            ),
            partners: vec![],
        },
    )
    .await;

    let mut left = Vec::new();
    for table in ["track_works", "track_artist_roles"] {
        left.push(manager.store.count_rows_for_test(table, "track_id", &track.id).await.unwrap());
    }
    assert_eq!(left, [0, 0]);
    assert_eq!(
        manager.records.get_tracks_for_release(&release.id).await.unwrap()[0].title,
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
    manager.records.insert_album(&grouped).await.unwrap();
    insert_release(&manager, &sibling).await;
    manager
        .records
        .insert_release_records(&sibling.id, &[mb_identity("move-group", "move-sibling")])
        .await
        .unwrap();

    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 1).await;

    seed_musicbrainz(&manager, "move-new", "move-group", 1);
    reidentify(&manager, &release.id, musicbrainz_link("move-new")).await;
    assert_eq!(
        manager.records.find_album_id_for_release(&release.id).await.unwrap(),
        Some(grouped.id.clone())
    );

    let picked = DbAlbum { title: "Picked Album".to_string(), ..create_test_album() };
    manager.records.insert_album(&picked).await.unwrap();
    let session = manager.open_release_edit(&release.id).await.unwrap();
    assert_eq!(
        manager.set_release_edit_album(&session, Some(picked.id.clone())).await.unwrap().album,
        bae_import::DraftAlbum::Picked {
            album_id: picked.id.clone(),
            title: "Picked Album".to_string(),
        }
    );
    manager.save_release_edit(&session).await.unwrap();
    assert_eq!(
        manager.records.find_album_id_for_release(&release.id).await.unwrap(),
        Some(picked.id.clone())
    );
    assert_eq!(
        manager.records.find_album_by_id(&picked.id).await.unwrap().unwrap().title,
        "Picked Album",
        "a picked album keeps its own title"
    );
}

/// The session says which album a save files the release under: the one it
/// is in, the one an applied catalog's records name (new where the library
/// holds none), or the one the person picked, which an apply keeps and a
/// fresh open drops.
#[tokio::test]
async fn a_release_edit_session_names_the_album_its_save_files_under() {
    use bae_import::DraftAlbum;
    let (manager, _temp_dir) = setup_test_manager().await;
    let grouped = DbAlbum { title: "Grouped Album".to_string(), ..create_test_album() };
    let sibling = create_test_release(&grouped.id);
    manager.records.insert_album(&grouped).await.unwrap();
    insert_release(&manager, &sibling).await;
    manager
        .records
        .insert_release_records(&sibling.id, &[mb_identity("named-group", "named-sibling")])
        .await
        .unwrap();
    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 1).await;
    let picked = DbAlbum { title: "Picked Album".to_string(), ..create_test_album() };
    manager.records.insert_album(&picked).await.unwrap();
    seed_musicbrainz(&manager, "named-new", "named-group", 1);
    seed_musicbrainz(&manager, "unnamed-new", "unnamed-group", 1);
    let named = |album: &DbAlbum| DraftAlbum::Named {
        album_id: album.id.clone(),
        title: album.title.clone(),
    };
    let picked_album =
        DraftAlbum::Picked { album_id: picked.id.clone(), title: picked.title.clone() };

    let session = manager.open_release_edit(&release.id).await.unwrap();
    assert_eq!(session.opened().album, named(&own));
    let applied = manager
        .apply_release_edit_catalog(
            &session,
            &musicbrainz_link("named-new"),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    assert_eq!(applied.album, named(&grouped));
    let applied = manager
        .apply_release_edit_catalog(
            &session,
            &musicbrainz_link("unnamed-new"),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    assert_eq!(applied.album, DraftAlbum::New);

    assert_eq!(
        manager.set_release_edit_album(&session, Some(picked.id.clone())).await.unwrap().album,
        picked_album
    );
    assert!(
        manager.set_release_edit_album(&session, Some("no-such-album".to_string())).await.is_err(),
        "an album the library does not hold cannot be picked"
    );
    let applied = manager
        .apply_release_edit_catalog(
            &session,
            &musicbrainz_link("named-new"),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    assert_eq!(applied.album, picked_album, "an apply keeps the person's pick");
    assert_eq!(
        manager.set_release_edit_album(&session, None).await.unwrap().album,
        named(&grouped),
        "left to its records, the session files it where they name"
    );
}

/// Two editors open on one release each hold their own session: what one
/// applies and picks is never the other's, and opening one more leaves both
/// as they were.
#[tokio::test]
async fn two_sessions_on_one_release_keep_their_own_apply_and_pick() {
    use bae_import::DraftAlbum;
    let (manager, _temp_dir) = setup_test_manager().await;
    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let picked = DbAlbum { title: "Picked Album".to_string(), ..create_test_album() };
    manager.records.insert_album(&picked).await.unwrap();
    seed_musicbrainz(&manager, "two-sessions-new", "two-sessions-group", 2);

    let sheet = manager.open_release_edit(&release.id).await.unwrap();
    let pane = manager.open_release_edit(&release.id).await.unwrap();
    manager
        .apply_release_edit_catalog(
            &sheet,
            &musicbrainz_link("two-sessions-new"),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    manager.set_release_edit_album(&sheet, Some(picked.id.clone())).await.unwrap();
    let third = manager.open_release_edit(&release.id).await.unwrap();

    assert_eq!(
        manager.set_release_edit_album(&pane, None).await.unwrap().album,
        DraftAlbum::Named { album_id: own.id.clone(), title: own.title.clone() },
        "the other editor's pick and apply are not this one's"
    );
    assert_eq!(third.opened(), pane.opened());
    manager.save_release_edit(&sheet).await.unwrap();

    assert_eq!(
        manager.records.find_album_id_for_release(&release.id).await.unwrap(),
        Some(picked.id.clone()),
        "opening other sessions left this one's pick in place"
    );
    let records = manager.records.get_release_records(&release.id).await.unwrap();
    assert_eq!(
        records.iter().map(|record| record.key()).collect::<Vec<_>>(),
        ["two-sessions-new"],
        "and its apply"
    );
}

/// Once one session saves a release, another session opened before that save
/// is refused, writing nothing: the library keeps what the first saved. So
/// does one opened before a field write from elsewhere, and a session opened
/// after the save saves as usual.
#[tokio::test]
async fn a_session_s_save_is_refused_once_the_release_changed_under_it() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let first = manager.open_release_edit(&release.id).await.unwrap();
    let second = manager.open_release_edit(&release.id).await.unwrap();
    title_session(&manager, &first, "Saved First").await;
    manager.save_release_edit(&first).await.unwrap();
    title_session(&manager, &second, "Saved Second").await;
    let refused = manager
        .save_release_edit(&second)
        .await
        .expect_err("the release changed after the second session opened");
    assert!(
        matches!(&refused, LibraryError::ReleaseEditStale { release_id } if *release_id == release.id),
        "{refused}"
    );
    assert_eq!(refused.category(), bae_diagnostics::UiErrorCategory::ReleaseChangedSinceOpened);
    let held = |manager: &LibraryManager| {
        let release_id = release.id.clone();
        let manager = manager.clone();
        async move {
            manager.open_release_edit(&release_id).await.unwrap().opened().edit.album_title.clone()
        }
    };
    assert_eq!(held(&manager).await, "Saved First");
    title_session(&manager, &first, "Saved Again").await;
    assert!(
        manager.save_release_edit(&first).await.is_err(),
        "a saved session opened on what the library held before its save"
    );

    let opened_before_write = manager.open_release_edit(&release.id).await.unwrap();
    let mut written = opened_before_write.opened().edit.clone();
    written.album_title = "Written Elsewhere".to_string();
    written.album_artist_assignments =
        vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))];
    manager.apply_release_metadata_user_edit(&release.id, &written.shape().unwrap()).await.unwrap();
    title_session(&manager, &opened_before_write, "Saved Over It").await;
    assert!(manager.save_release_edit(&opened_before_write).await.is_err());
    assert_eq!(held(&manager).await, "Written Elsewhere");

    let fresh = manager.open_release_edit(&release.id).await.unwrap();
    title_session(&manager, &fresh, "Saved Fresh").await;
    manager.save_release_edit(&fresh).await.unwrap();
    assert_eq!(held(&manager).await, "Saved Fresh");
}

/// Write `title` into `session`'s album title, credited to one artist.
async fn title_session(manager: &LibraryManager, session: &ReleaseEdit, title: &str) {
    manager
        .edit_release_edit_field(
            session,
            bae_import::DraftFieldEdit::Text {
                field: bae_import::CandidateEditField::AlbumTitle,
                value: title.to_string(),
            },
        )
        .await
        .unwrap();
    manager
        .set_release_edit_album_artists(
            session,
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
}

/// Each change is written to the session as it is made: the draft holds it,
/// says the session changed and whether the form can be saved, and the
/// library holds none of it until the save writes the session. Opening the
/// release again drops it.
#[tokio::test]
async fn a_session_holds_each_change_until_its_save() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let session = manager.open_release_edit(&release.id).await.unwrap();
    let opened = manager.release_edit_draft(&session).await.unwrap();
    assert!(!opened.changed, "nothing changed yet");
    let library_titles = |manager: &LibraryManager| {
        let release_id = release.id.clone();
        let manager = manager.clone();
        async move {
            manager
                .records
                .get_tracks_for_release(&release_id)
                .await
                .unwrap()
                .into_iter()
                .map(|track| track.title)
                .collect::<Vec<_>>()
        }
    };
    let stored = library_titles(&manager).await;

    let mut row = opened.edit.tracks[1].clone();
    row.title = "Renamed Track".to_string();
    let draft = manager.set_release_edit_track(&session, row.clone()).await.unwrap();
    assert_eq!(draft.edit.tracks[1].title, "Renamed Track");
    assert!(draft.changed);
    assert_eq!(library_titles(&manager).await, stored, "the library holds no change yet");
    assert_eq!(manager.release_edit_draft(&session).await.unwrap().edit, draft.edit);

    let blank = manager
        .edit_release_edit_field(
            &session,
            bae_import::DraftFieldEdit::Text {
                field: bae_import::CandidateEditField::AlbumTitle,
                value: "  ".to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(blank.invalid, Some(bae_import::EditValidationError::EmptyAlbumTitle));
    assert!(manager.save_release_edit(&session).await.is_err(), "a blank title can't be saved");
    title_session(&manager, &session, "Album Title").await;
    assert_eq!(manager.release_edit_draft(&session).await.unwrap().invalid, None);

    let reopened = manager.open_release_edit(&release.id).await.unwrap();
    let reopened = manager.release_edit_draft(&reopened).await.unwrap();
    assert_eq!(reopened.edit, opened.edit, "opening again drops what the session held");
    assert!(!reopened.changed);

    manager.save_release_edit(&session).await.unwrap();
    assert_eq!(library_titles(&manager).await[1], "Renamed Track");

    row.id = "no-such-row".to_string();
    assert!(
        manager.set_release_edit_track(&session, row).await.is_err(),
        "a row the form does not hold is refused"
    );
}

#[tokio::test]
async fn re_identify_with_a_partner_writes_both_identity_rows() {
    use bae_catalogs::catalog::Catalog;
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();

    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager
        .records
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
    manager.fakes.seed_musicbrainz_release(mb_release_id, raw_json);
    manager.fakes.seed_musicbrainz_release_group(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );

    let discogs_release_id = "80000101";
    let discogs_master_id = "80009101";
    manager.fakes.seed_discogs_master(
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
    bae_catalogs::discogs::documents::parse_discogs_release_json(&discogs_raw)
        .expect("the rendered Discogs release parses");
    manager.fakes.seed_discogs_release(discogs_release_id, discogs_raw);
    manager.fakes.seed_discogs_page_lookup(&[
        (bae_catalogs::musicbrainz::DiscogsPage::Release(discogs_release_id.to_string()), None),
        (bae_catalogs::musicbrainz::DiscogsPage::Master(discogs_master_id.to_string()), None),
    ]);

    reidentify(
        &manager,
        &release.id,
        bae_identify::PressingLink {
            record: LookupRef::new(LookupCatalog::MusicBrainz, mb_release_id),
            partners: vec![LookupRef::new(LookupCatalog::Discogs, discogs_release_id)],
        },
    )
    .await;

    let identities = manager.records.get_release_records(&release.id).await.unwrap();
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
    assert_eq!(discogs.album_ref().expect("known parent album").key, discogs_master_id);
    assert_eq!(discogs.key(), discogs_release_id);

    // No row names the applied releases, so neither is stored.
    for (catalog, release_id) in
        [(LookupCatalog::MusicBrainz, mb_release_id), (LookupCatalog::Discogs, discogs_release_id)]
    {
        assert_eq!(
            stored_for(&manager, catalog, release_id).await,
            None,
            "a re-identify stores none of the releases it reads"
        );
    }
}

#[tokio::test]
async fn release_edit_seed_uses_persisted_track_ids() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        &manager,
        &track,
        &[("track.flac", bae_records::source_audio::SourceAudioLayout::File)],
    )
    .await;

    let seed = manager.open_release_edit(&release.id).await.unwrap().opened().clone();

    assert_eq!(seed.edit.tracks[0].id, track.id);
}

#[tokio::test]
async fn an_applied_catalog_s_form_keeps_the_persisted_track_ids() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let persisted_ids = manager
        .records
        .get_tracks_for_release(&release.id)
        .await
        .unwrap()
        .into_iter()
        .map(|track| track.id)
        .collect::<Vec<_>>();

    let source_release_id = "reset-editor-release";
    let source_group_id = "reset-editor-group";
    let response = make_mb_release_for_re_identify(source_release_id, source_group_id, 2);
    let raw_json = serde_json::to_string(&response).unwrap();
    manager.fakes.seed_musicbrainz_release(source_release_id, raw_json);
    manager.fakes.seed_musicbrainz_release_group(
        source_group_id,
        r#"{"id":"reset-editor-group"}"#.to_string(),
    );
    let session = manager.open_release_edit(&release.id).await.unwrap();
    let applied = manager
        .apply_release_edit_catalog(
            &session,
            &bae_identify::PressingLink {
                record: LookupRef::new(LookupCatalog::MusicBrainz, source_release_id),
                partners: vec![],
            },
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    assert_eq!(
        applied.edit.tracks.into_iter().map(|track| track.id).collect::<Vec<_>>(),
        persisted_ids
    );
}

/// Applying a catalog release the library doesn't hold tells the read it
/// waits on, and that the read ended.
#[tokio::test]
async fn an_applied_catalog_tells_its_catalog_read() {
    use bae_catalogs::catalog::Catalog;
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let source_release_id = "steps-editor-release";
    let source_group_id = "steps-editor-group";
    let response = make_mb_release_for_re_identify(source_release_id, source_group_id, 2);
    manager
        .fakes
        .seed_musicbrainz_release(source_release_id, serde_json::to_string(&response).unwrap());
    manager.fakes.seed_musicbrainz_release_group(
        source_group_id,
        r#"{"id":"steps-editor-group"}"#.to_string(),
    );
    let told = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let keep = told.clone();
    let steps = bae_import::CatalogApplySteps::new(move |step| keep.lock().unwrap().push(step));
    let session = manager.open_release_edit(&release.id).await.unwrap();

    manager
        .apply_release_edit_catalog(
            &session,
            &bae_identify::PressingLink {
                record: LookupRef::new(LookupCatalog::MusicBrainz, source_release_id),
                partners: vec![],
            },
            &steps,
        )
        .await
        .unwrap();

    assert_eq!(
        *told.lock().unwrap(),
        vec![Some(bae_import::CatalogApplyStep::ReadingRelease(Catalog::MusicBrainz)), None,]
    );
}

#[tokio::test]
async fn release_edit_seed_refuses_a_track_without_stored_audio() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();

    let error = manager
        .open_release_edit(&release.id)
        .await
        .expect_err("an editor seed needs the track's stored audio relationship");

    assert!(error.to_string().contains(&track.id), "{error}");
}

#[tokio::test]
async fn release_edit_seed_names_the_file_each_track_s_body_plays_from() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.facts = bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 2);
    let first = bae_records::DbTrack::new_test(&release.id, TRACK_1, "First Track", 1);
    let mut second = bae_records::DbTrack::new_test(&release.id, TRACK_2, "Second Track", 1);
    let second_disc = bae_records::DbReleaseMedium::new(
        &release.id,
        std::num::NonZeroU16::new(2).expect("disc 2"),
        bae_types::mediums::ReleaseMedium::one_sided(Some(bae_types::pressing::Medium::Cd)),
        chrono::Utc::now(),
    );
    second.medium_id.clone_from(&second_disc.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.records.insert_release_medium(&second_disc).await.unwrap();
    manager.records.insert_track(&first).await.unwrap();
    manager.records.insert_track(&second).await.unwrap();
    add_track_audio_sources(
        &manager,
        &first,
        &[("first.flac", bae_records::source_audio::SourceAudioLayout::File)],
    )
    .await;
    add_track_audio_sources(
        &manager,
        &second,
        &[
            ("pregap.flac", bae_records::source_audio::SourceAudioLayout::Cue),
            ("body.flac", bae_records::source_audio::SourceAudioLayout::Cue),
        ],
    )
    .await;
    store_test_cover(&manager, &release.id, b"cover").await;

    let seed = manager.open_release_edit(&release.id).await.unwrap().opened().clone();

    assert_eq!(seed.cover.as_ref().map(|cover| cover.id.as_str()), Some(release.id.as_str()));
    let sides = &seed.display.sides;
    assert_eq!(
        sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
        [
            &bae_types::mediums::TrackSide::Disc { disc: 1 },
            &bae_types::mediums::TrackSide::Disc { disc: 2 },
        ]
    );
    let bae_import::ReleaseEditTrackRun::Track(first_context) = &sides[0].runs[0] else {
        panic!("a track on its own file is a run of its own");
    };
    assert_eq!(first_context.track_id, first.id);
    assert_eq!(first_context.source.layout, bae_records::source_audio::SourceAudioLayout::File);
    // A pregap stored in the file before is not the track's source.
    let bae_import::ReleaseEditTrackRun::Track(second_context) = &sides[1].runs[0] else {
        panic!("the one track of its disc is a run of its own");
    };
    assert_eq!(second_context.track_id, second.id);
    assert_eq!(second_context.source.name, "body.flac");
    assert!(matches!(
        seed.display.source_audio,
        Some(bae_records::source_audio::SourceAudioSummary::Mixed { .. })
    ));
}
