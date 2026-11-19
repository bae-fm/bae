//! A release's editor: the form it opens on, a catalog applied, the album
//! picked, re-identifying, and a save refused once the release changed.

use super::*;
use crate::manager::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::LookupRef;
use bae_records::DbAlbum;
use time::UtcDateTime;

// ── Re-identify: a catalog applied to a form, then saved ─────
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

/// Apply `link` to a form of its own on `release_id` and save it as
/// applied.
async fn reidentify(manager: &LibraryManager, release_id: &str, link: bae_identify::PressingLink) {
    let form = manager.open_release_edit(release_id).await.unwrap();
    let form = manager
        .apply_release_edit_catalog(form, &link, &bae_import::CatalogApplySteps::unread())
        .await
        .unwrap();
    manager.save_release_edit(form, super::GonePick::Unpick).await.unwrap();
}

/// Seed the MusicBrainz release `release_id` of group `group_id`, listing
/// `tracks` tracks, where `prepare_release` reads it.
pub(super) fn seed_musicbrainz(
    manager: &TestManager,
    release_id: &str,
    group_id: &str,
    tracks: usize,
) {
    let response = make_mb_release_for_re_identify(release_id, group_id, tracks);
    manager.fakes.seed_musicbrainz_release(release_id, serde_json::to_string(&response).unwrap());
    manager.fakes.seed_musicbrainz_release_group(
        group_id,
        serde_json::json!({ "id": group_id }).to_string(),
    );
}

/// [`insert_n_tracks`], each track playing a file of its own, as the editor
/// reads a release's tracks.
pub(super) async fn insert_editable_tracks(manager: &LibraryManager, release_id: &str, n: usize) {
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

pub(super) fn musicbrainz_link(release_id: &str) -> bae_identify::PressingLink {
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
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager
        .records
        .insert_release_records(&release.id, &[mb_identity("g-old", "mb-rel-old")])
        .await
        .unwrap();
    insert_editable_tracks(manager, &release.id, 3).await;

    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    seed_musicbrainz(&test, new_release_id, new_group_id, 3);
    reidentify(manager, &release.id, musicbrainz_link(new_release_id)).await;

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
        stored_for(manager, LookupCatalog::MusicBrainz, new_release_id).await,
        None,
        "nothing names the picked release, so nothing stores it"
    );
}

/// A release whose tracks the picked release cannot lay out is refused with
/// both counts, before anything is held or written.
#[tokio::test]
async fn a_reidentify_that_does_not_fit_the_stored_tracks_is_refused() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(manager.library.manager(), &release.id, 10).await;
    let new_release_id = "mismatch-re-identify-mb-rel-new";
    seed_musicbrainz(&manager, new_release_id, "mismatch-re-identify-mb-group-new", 12);

    let form = manager.library.manager().open_release_edit(&release.id).await.unwrap();
    let err = manager
        .library
        .manager()
        .apply_release_edit_catalog(
            form,
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
    assert!(manager
        .library
        .manager()
        .records
        .get_release_records(&release.id)
        .await
        .unwrap()
        .is_empty());
}

/// Discarding a form with a catalog applied writes nothing: the library's
/// values, records and album are exactly as they were.
#[tokio::test]
async fn discarding_a_form_restores_the_library_s_values_exactly() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 2).await;
    let form = manager.open_release_edit(&release.id).await.unwrap();
    let before = form.opened.clone();

    let new_release_id = "discard-re-identify-mb-rel-new";
    seed_musicbrainz(&test, new_release_id, "discard-re-identify-mb-group-new", 2);
    let applied = manager
        .apply_release_edit_catalog(
            form,
            &musicbrainz_link(new_release_id),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    assert_ne!(applied.edit, before.edit, "the apply filled the form");
    drop(applied);

    let reopened = manager.open_release_edit(&release.id).await.unwrap();
    assert_eq!(reopened.opened, before);
    assert!(manager.records.get_release_records(&release.id).await.unwrap().is_empty());
}

/// Re-identifying a MusicBrainz release as a Discogs release and saving
/// leaves none of what MusicBrainz credited: its composers' roles and the
/// works its tracks performed go with it.
#[tokio::test]
async fn a_reidentify_leaves_none_of_the_previous_source_s_roles_or_works() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    test.library
        .catalogs()
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 1).await;
    let track = manager.records.get_tracks_for_release(&release.id).await.unwrap()[0].clone();
    let composer = DbArtist {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        name: "Composer Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-composer-1".to_string()),
        created_at: UtcDateTime::now(),
    };
    manager.records.insert_artist(&composer).await.unwrap();
    let work = bae_records::DbWork {
        id: bae_records::identity::work_id("mb-work-1"),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: None,
        musicbrainz_work_id: "mb-work-1".to_string(),
        created_at: UtcDateTime::now(),
    };
    manager
        .records
        .insert_library_rows_for_test(bae_records::test_fixtures::library::LibraryRows {
            track_works: vec![bae_records::DbTrackWork::new(
                &track.id,
                &work.id,
                0,
                bae_catalogs::catalog::Catalog::MusicBrainz,
                coven::IdSource::new_id(&coven::UuidIds).to_string(),
                UtcDateTime::now(),
            )],
            works: vec![work],
            track_artist_roles: vec![bae_records::DbTrackArtistRole {
                id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
                track_id: track.id.clone(),
                artist_id: composer.id.clone(),
                position: 0,
                source: bae_catalogs::catalog::Catalog::MusicBrainz,
                role: bae_types::credit_role::CreditedAs::of(
                    bae_types::credit_role::CreditRoleKind::Composer,
                ),
                credit: None,
                created_at: UtcDateTime::now(),
            }],
            ..bae_records::test_fixtures::library::LibraryRows::default()
        })
        .await
        .unwrap();

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
    test.fakes.seed_discogs_release(discogs_release_id, discogs_raw);
    test.fakes.seed_discogs_artist_image("1", None);
    test.fakes.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release(discogs_release_id.to_string()),
        None,
    )]);
    reidentify(
        manager,
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
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let grouped = create_test_album();
    let sibling = create_test_release(&grouped.id);
    manager.records.insert_album(&grouped).await.unwrap();
    insert_release(manager, &sibling).await;
    manager
        .records
        .insert_release_records(&sibling.id, &[mb_identity("move-group", "move-sibling")])
        .await
        .unwrap();

    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 1).await;

    seed_musicbrainz(&test, "move-new", "move-group", 1);
    reidentify(manager, &release.id, musicbrainz_link("move-new")).await;
    assert_eq!(
        manager.records.find_album_id_for_release(&release.id).await.unwrap(),
        Some(grouped.id.clone())
    );

    let picked = DbAlbum { title: "Picked Album".to_string(), ..create_test_album() };
    manager.records.insert_album(&picked).await.unwrap();
    let mut form = manager.open_release_edit(&release.id).await.unwrap();
    form.album_pick = album_pick(&picked);
    assert_eq!(
        draft_now(manager, &form).await.album,
        bae_import::DraftAlbum::Picked {
            album_id: picked.id.clone(),
            line: bae_import::AlbumLine::TitleAlone { title: "Picked Album".to_string() },
        }
    );
    manager.save_release_edit(form, super::GonePick::Unpick).await.unwrap();
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

/// Two editors open on one release each hold their own form: what one
/// applies and picks is never the other's, and opening one more leaves both
/// as they were.
#[tokio::test]
async fn two_forms_on_one_release_keep_their_own_apply_and_pick() {
    use bae_import::DraftAlbum;
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 2).await;
    let picked = DbAlbum { title: "Picked Album".to_string(), ..create_test_album() };
    manager.records.insert_album(&picked).await.unwrap();
    seed_musicbrainz(&test, "two-sessions-new", "two-sessions-group", 2);

    let sheet = manager.open_release_edit(&release.id).await.unwrap();
    let pane = manager.open_release_edit(&release.id).await.unwrap();
    let mut sheet = manager
        .apply_release_edit_catalog(
            sheet,
            &musicbrainz_link("two-sessions-new"),
            &bae_import::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    sheet.album_pick = album_pick(&picked);
    let third = manager.open_release_edit(&release.id).await.unwrap();

    assert_eq!(
        draft_now(manager, &pane).await.album,
        DraftAlbum::Named {
            album_id: own.id.clone(),
            line: bae_import::AlbumLine::TitleAlone { title: own.title.clone() },
        },
        "the other editor's pick and apply are not this one's"
    );
    assert_eq!(third.opened, pane.opened);
    manager.save_release_edit(sheet, super::GonePick::Unpick).await.unwrap();

    assert_eq!(
        manager.records.find_album_id_for_release(&release.id).await.unwrap(),
        Some(picked.id.clone()),
        "opening other forms left this one's pick in place"
    );
    let records = manager.records.get_release_records(&release.id).await.unwrap();
    assert_eq!(
        records.iter().map(|record| record.key()).collect::<Vec<_>>(),
        ["two-sessions-new"],
        "and its apply"
    );
}

/// Once one form saves a release, the library refuses the save of a form
/// the test opened before that save, and writes nothing: the library keeps
/// what the first form saved, and the save returns the form opened on the
/// values it saved, so the first editor can save again. The library also
/// refuses the save of a form the test opened before a field write from
/// elsewhere, and a form the test opens after the save saves as usual.
#[tokio::test]
async fn a_form_s_save_is_refused_once_the_release_changed_under_it() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 2).await;
    let first = manager.open_release_edit(&release.id).await.unwrap();
    let second = manager.open_release_edit(&release.id).await.unwrap();
    let first = manager
        .save_release_edit(titled(first, "Saved First"), super::GonePick::Unpick)
        .await
        .unwrap();
    let refused = manager
        .save_release_edit(titled(second, "Saved Second"), super::GonePick::Unpick)
        .await
        .expect_err("the release changed after the second form opened");
    assert!(
        matches!(&refused, LibraryError::ReleaseEditStale { release_id } if *release_id == release.id),
        "{refused}"
    );
    assert_eq!(refused.category(), bae_diagnostics::UiErrorCategory::ReleaseChangedSinceOpened);
    let held = |manager: &LibraryManager| {
        let release_id = release.id.clone();
        let manager = manager.clone();
        async move { manager.open_release_edit(&release_id).await.unwrap().opened.edit.album_title }
    };
    assert_eq!(held(manager).await, "Saved First");
    manager
        .save_release_edit(titled(first, "Saved Again"), super::GonePick::Unpick)
        .await
        .expect("a save returns the form opened on what it saved");
    assert_eq!(held(manager).await, "Saved Again");

    let opened_before_write = manager.open_release_edit(&release.id).await.unwrap();
    let mut written = opened_before_write.opened.edit.clone();
    written.album_title = "Written Elsewhere".to_string();
    written.album_artist_assignments =
        vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))];
    manager.apply_release_metadata_user_edit(&release.id, &written.shape().unwrap()).await.unwrap();
    assert!(manager
        .save_release_edit(titled(opened_before_write, "Saved Over It"), super::GonePick::Unpick)
        .await
        .is_err());
    assert_eq!(held(manager).await, "Written Elsewhere");

    let fresh = manager.open_release_edit(&release.id).await.unwrap();
    manager.save_release_edit(titled(fresh, "Saved Fresh"), super::GonePick::Unpick).await.unwrap();
    assert_eq!(held(manager).await, "Saved Fresh");
}

/// `form` with `title` as its album title, credited to one artist.
pub(super) fn titled(
    mut form: bae_import::ReleaseEditForm,
    title: &str,
) -> bae_import::ReleaseEditForm {
    form.edit.set_field(bae_import::DraftFieldEdit::Text {
        field: bae_import::CandidateEditField::AlbumTitle,
        value: title.to_string(),
    });
    form.edit.set_album_artists(vec![bae_import::Credited::plain(
        bae_import::ArtistAssignment::named("Artist Name"),
    )]);
    form
}

/// Each change makes a new form: its draft holds the change, says the form
/// changed and whether it can be saved, and the library holds none of it
/// until the save writes the form. Opening the release again drops it.
#[tokio::test]
async fn a_form_holds_each_change_until_its_save() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 2).await;
    let mut form = manager.open_release_edit(&release.id).await.unwrap();
    let opened = draft_now(manager, &form).await;
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
    let stored = library_titles(manager).await;

    let mut row = form.edit.tracks[1].clone();
    row.title = "Renamed Track".to_string();
    assert!(form.edit.set_track(row.clone()));
    let draft = draft_now(manager, &form).await;
    let titles: Vec<_> = draft
        .sides
        .iter()
        .flatten()
        .flat_map(bae_import::ReleaseEditTrackSide::rows)
        .map(|row| row.edit.title.as_str())
        .collect();
    assert_eq!(titles[1], "Renamed Track", "the track row beside the form's");
    assert!(draft.changed);
    assert_eq!(library_titles(manager).await, stored, "the library holds no change yet");

    form.edit.set_field(bae_import::DraftFieldEdit::Text {
        field: bae_import::CandidateEditField::AlbumTitle,
        value: "  ".to_string(),
    });
    let blank = draft_now(manager, &form).await;
    assert_eq!(blank.invalid, Some(bae_import::EditValidationError::EmptyAlbumTitle));
    assert!(
        manager.save_release_edit(form.clone(), super::GonePick::Unpick).await.is_err(),
        "a blank title can't be saved"
    );
    let form = titled(form, "Album Title");
    assert_eq!(draft_now(manager, &form).await.invalid, None);

    let reopened = manager.open_release_edit(&release.id).await.unwrap();
    assert_eq!(reopened.edit, form.opened.edit, "opening again drops what the form held");
    assert!(!draft_now(manager, &reopened).await.changed);

    let mut form = manager.save_release_edit(form, super::GonePick::Unpick).await.unwrap();
    assert_eq!(library_titles(manager).await[1], "Renamed Track");

    row.id = "no-such-row".to_string();
    assert!(!form.edit.set_track(row), "a row the form does not hold is refused");
}

#[tokio::test]
async fn re_identify_with_a_partner_writes_both_identity_rows() {
    use bae_catalogs::catalog::Catalog;
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    test.library
        .catalogs()
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();

    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager
        .records
        .insert_release_records(
            &release.id,
            &[mb_identity("g-old", "partner-re-identify-mb-rel-old")],
        )
        .await
        .unwrap();
    insert_editable_tracks(manager, &release.id, 1).await;

    let mb_release_id = "partner-re-identify-mb-rel-new";
    let mb_group_id = "partner-re-identify-mb-group-new";
    let response = make_mb_release_for_re_identify(mb_release_id, mb_group_id, 1);
    let raw_json = serde_json::to_string(&response).unwrap();
    test.fakes.seed_musicbrainz_release(mb_release_id, raw_json);
    test.fakes.seed_musicbrainz_release_group(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );

    let discogs_release_id = "80000101";
    let discogs_master_id = "80009101";
    test.fakes.seed_discogs_master(
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
    test.fakes.seed_discogs_release(discogs_release_id, discogs_raw);
    test.fakes.seed_discogs_page_lookup(&[
        (bae_catalogs::musicbrainz::DiscogsPage::Release(discogs_release_id.to_string()), None),
        (bae_catalogs::musicbrainz::DiscogsPage::Master(discogs_master_id.to_string()), None),
    ]);

    reidentify(
        manager,
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
            stored_for(manager, catalog, release_id).await,
            None,
            "a re-identify stores none of the releases it reads"
        );
    }
}

#[tokio::test]
async fn release_edit_seed_uses_persisted_track_ids() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        manager,
        &track,
        &[("track.flac", bae_records::source_audio::SourceAudioLayout::File)],
    )
    .await;

    let form = manager.open_release_edit(&release.id).await.unwrap();

    assert_eq!(form.opened.edit.tracks[0].id, track.id);
}

#[tokio::test]
async fn an_applied_catalog_s_form_keeps_the_persisted_track_ids() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(manager.library.manager(), &release.id, 2).await;
    let persisted_ids = manager
        .library
        .manager()
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
    let library = manager.library.manager();
    let form = library.open_release_edit(&release.id).await.unwrap();
    let applied = library
        .apply_release_edit_catalog(
            form,
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
    insert_editable_tracks(manager.library.manager(), &release.id, 2).await;
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
    let steps =
        bae_import::CatalogApplySteps::recorded(move |step| keep.lock().unwrap().push(step));
    let form = manager.library.manager().open_release_edit(&release.id).await.unwrap();

    manager
        .library
        .manager()
        .apply_release_edit_catalog(
            form,
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
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = bae_records::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.records.insert_track(&track).await.unwrap();

    let error = manager
        .open_release_edit(&release.id)
        .await
        .expect_err("an editor seed needs the track's stored audio relationship");

    assert!(error.to_string().contains(&track.id), "{error}");
}

#[tokio::test]
async fn release_edit_seed_names_the_file_each_track_s_body_plays_from() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.facts = bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 2);
    let first = bae_records::DbTrack::new_test(&release.id, TRACK_1, "First Track", 1);
    let mut second = bae_records::DbTrack::new_test(&release.id, TRACK_2, "Second Track", 1);
    let second_disc = bae_records::DbReleaseMedium::new(
        &coven::UuidIds,
        &release.id,
        std::num::NonZeroU16::new(2).expect("disc 2"),
        bae_types::mediums::ReleaseMedium::one_sided(Some(bae_types::pressing::Medium::Cd)),
        time::UtcDateTime::now(),
    );
    second.medium_id.clone_from(&second_disc.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    manager.records.insert_release_medium(&second_disc).await.unwrap();
    manager.records.insert_track(&first).await.unwrap();
    manager.records.insert_track(&second).await.unwrap();
    add_track_audio_sources(
        manager,
        &first,
        &[("first.flac", bae_records::source_audio::SourceAudioLayout::File)],
    )
    .await;
    add_track_audio_sources(
        manager,
        &second,
        &[
            ("pregap.flac", bae_records::source_audio::SourceAudioLayout::Cue),
            ("body.flac", bae_records::source_audio::SourceAudioLayout::Cue),
        ],
    )
    .await;
    store_test_cover(manager, &release.id, b"cover").await;

    let form = manager.open_release_edit(&release.id).await.unwrap();
    let draft = draft_now(manager, &form).await;

    assert_eq!(draft.cover.map(|cover| cover.id), Some(release.id.clone()));
    let sides = draft.sides.expect("the release has tracks");
    assert_eq!(
        sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
        [
            &bae_types::mediums::TrackSide::Disc { disc: 1 },
            &bae_types::mediums::TrackSide::Disc { disc: 2 },
        ]
    );
    let bae_import::ReleaseEditTrackRun::Track(first_row) = &sides[0].runs[0] else {
        panic!("a track on its own file is a run of its own");
    };
    assert_eq!(first_row.context.track_id, first.id);
    assert_eq!(first_row.edit.id, first.id);
    assert_eq!(first_row.context.source.layout, bae_records::source_audio::SourceAudioLayout::File);
    // A pregap stored in the file before is not the track's source.
    let bae_import::ReleaseEditTrackRun::Track(second_row) = &sides[1].runs[0] else {
        panic!("the one track of its disc is a run of its own");
    };
    assert_eq!(second_row.context.track_id, second.id);
    assert_eq!(second_row.edit.title, "Second Track");
    assert_eq!(second_row.context.source.name, "body.flac");
    assert!(matches!(
        draft.source_audio,
        Some(bae_records::source_audio::SourceAudioSummary::Mixed { .. })
    ));
}

/// `form`'s draft as its live query reads it now.
pub(super) async fn draft_now(
    manager: &LibraryManager,
    form: &bae_import::ReleaseEditForm,
) -> bae_import::ReleaseEditDraft {
    let draft = manager.subscribe_release_edit_draft();
    draft.set(Some(form.clone())).expect("the draft takes the form");
    draft.next().await.expect("the draft reads").expect("the library holds the release")
}

/// Picking `album` as the album a form files its release under.
pub(super) fn album_pick(album: &DbAlbum) -> bae_import::AlbumPick {
    bae_import::AlbumPick::Album { album_id: album.id.clone() }
}
