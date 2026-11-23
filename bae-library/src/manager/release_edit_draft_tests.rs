//! A release editor's form and its live draft: what a save and Cancel hand
//! the editor, and the draft following the library while the editor is open.

use super::tests::{draft_now, insert_editable_tracks, titled};
use super::*;
use crate::manager::tests::*;
use bae_util::test_channels::HANG_GUARD;

/// A library holding one release of two tracks, and its id.
async fn release_of_two_tracks(manager: &LibraryManager) -> String {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 2).await;
    release.id
}

#[tokio::test]
async fn a_save_returns_the_form_opened_on_what_it_saved() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let form = manager.open_release_edit(&release_id).await.unwrap();

    let saved = manager
        .save_release_edit(titled(form, "Saved Title"), super::GonePick::Unpick)
        .await
        .unwrap();

    assert_eq!(saved.edit.album_title, "Saved Title");
    let draft = draft_now(manager, &saved).await;
    assert!(!draft.changed, "the saved form opened on what it saved");
    manager.save_release_edit(titled(saved, "Saved Again"), super::GonePick::Unpick).await.unwrap();
}

#[tokio::test]
async fn opening_again_reads_the_release_as_it_is() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let opened = manager.open_release_edit(&release_id).await.unwrap();
    let other = manager.open_release_edit(&release_id).await.unwrap();
    manager
        .save_release_edit(titled(other, "Written Elsewhere"), super::GonePick::Unpick)
        .await
        .unwrap();

    let reopened = manager.open_release_edit(&release_id).await.unwrap();

    assert_eq!(reopened.edit.album_title, "Written Elsewhere");
    assert!(!reopened.changed());
    assert_ne!(reopened.edit, opened.edit);
}

/// Pull `draft` until its value holds `wanted`.
async fn draft_until(
    draft: &LiveReleaseEditDraft,
    what: &str,
    wanted: impl Fn(&Option<bae_import::ReleaseEditDraft>) -> bool,
) {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if wanted(&draft.next().await.expect("the draft reads")) {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the live draft never showed {what}"));
}

/// The live draft of `form`.
fn draft_of(manager: &LibraryManager, form: &bae_import::ReleaseEditForm) -> LiveReleaseEditDraft {
    let draft = manager.subscribe_release_edit_draft();
    draft.set(Some(form.clone())).expect("the draft takes the form");
    draft
}

#[tokio::test]
async fn the_live_draft_shows_a_merged_artist_without_an_edit() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let mut form = manager.open_release_edit(&release_id).await.unwrap();
    form.edit.set_album_artists(vec![bae_import::Credited::plain(
        bae_import::ArtistAssignment::named("Newcomer"),
    )]);
    let draft = draft_of(manager, &form);
    let stands = |wanted: bae_import::ArtistStanding| {
        move |draft: &Option<bae_import::ReleaseEditDraft>| {
            draft.as_ref().is_some_and(|draft| {
                draft.album_artists.as_ref().is_some_and(|field| {
                    field.credits.iter().map(|credit| &credit.standing).eq([&wanted])
                })
            })
        }
    };
    draft_until(&draft, "the credit as new", stands(bae_import::ArtistStanding::New)).await;

    manager
        .records
        .insert_artist(&bae_records::DbArtist {
            id: bae_util::test_ids::test_uuid("4f0c6d2e-0b8a-4a57-9a0e-6f3c2b1d7e55"),
            name: "Newcomer".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: time::UtcDateTime::now(),
        })
        .await
        .unwrap();

    draft_until(
        &draft,
        "the credit as the library's artist",
        stands(bae_import::ArtistStanding::Library),
    )
    .await;
}

/// An artist the library holds for one other album's release alone, and
/// that release's id: deleting the release deletes the artist.
async fn artist_of_its_own_release(
    manager: &LibraryManager,
    name: &str,
) -> (bae_records::DbArtist, String) {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    let artist = bae_records::DbArtist {
        id: bae_util::test_ids::test_uuid("7d2a4c1e-5b3f-4e8a-9c6d-2f1e0a9b8c7d"),
        name: name.to_string(),
        sort_name: Some(format!("{name}, Sorted")),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: time::UtcDateTime::now(),
    };
    manager.records.insert_artist(&artist).await.unwrap();
    manager
        .records
        .insert_album_artist(&bae_records::DbAlbumArtist::new(
            &coven::UuidIds,
            &album.id,
            &artist.id,
            0,
            time::UtcDateTime::now(),
        ))
        .await
        .unwrap();
    (artist, release.id)
}

#[tokio::test]
async fn an_artist_the_form_picked_shows_and_saves_as_a_new_credit_once_deleted() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let (artist, other_release) = artist_of_its_own_release(manager, "Departed Artist").await;
    let mut form = manager.open_release_edit(&release_id).await.unwrap();
    let picked = || {
        vec![bae_import::Credited::plain(bae_import::ArtistAssignment::picked(
            bae_records::ExistingArtist::from(artist.clone()),
        ))]
    };
    form.edit.set_album_artists(picked());
    let mut track = form.edit.tracks[0].clone();
    track.artist_assignments = bae_import::TrackArtistAssignments::Explicit(picked());
    assert!(form.edit.set_track(track));
    let draft = draft_of(manager, &form);
    let stands = |wanted: bae_import::ArtistStanding| {
        move |draft: &Option<bae_import::ReleaseEditDraft>| {
            draft.as_ref().is_some_and(|draft| {
                let track_artists = draft
                    .sides
                    .iter()
                    .flatten()
                    .flat_map(bae_import::TrackTableSide::rows)
                    .find_map(|row| match &row.artist {
                        bae_import::TrackArtistDisplay::Own(field) => Some(field),
                        _ => None,
                    });
                [draft.album_artists.as_ref(), track_artists].into_iter().all(|field| {
                    field.is_some_and(|field| {
                        field.credits.iter().map(|credit| &credit.standing).eq([&wanted])
                    })
                })
            })
        }
    };
    draft_until(&draft, "the picked artist", stands(bae_import::ArtistStanding::Library)).await;

    manager.delete_release(&other_release).await.unwrap();
    assert_eq!(manager.records.find_artist_by_id(&artist.id).await.unwrap(), None);

    draft_until(&draft, "the credit as new", stands(bae_import::ArtistStanding::New)).await;
    let draft = draft_now(manager, &form).await;
    let credit = bae_import::Credited::plain(bae_import::ArtistAssignment::Credit {
        credit: bae_catalogs::credits::ArtistCredit {
            sort_name: Some("Departed Artist, Sorted".to_string()),
            ..bae_catalogs::credits::ArtistCredit::named("Departed Artist")
        },
    });
    let first_row = draft
        .sides
        .iter()
        .flatten()
        .flat_map(bae_import::TrackTableSide::rows)
        .next()
        .expect("the release has tracks");
    assert_eq!(
        first_row.track.artist_assignments,
        bae_import::TrackArtistAssignments::Explicit(vec![credit])
    );

    // The save writes the credit the editor shows.
    manager.save_release_edit(form, super::GonePick::Unpick).await.expect("the form saves");
    let album_id = manager.records.find_album_id_for_release(&release_id).await.unwrap().unwrap();
    let credits = manager.records.get_artists_for_album(&album_id).await.unwrap();
    let [saved] = credits.as_slice() else { panic!("one album credit: {credits:?}") };
    assert_eq!(saved.artist.name, "Departed Artist");
}

#[tokio::test]
async fn the_live_draft_shows_a_cover_picked_after_it_opened() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let form = manager.open_release_edit(&release_id).await.unwrap();
    let draft = draft_of(manager, &form);
    draft_until(&draft, "no cover", |draft| draft.as_ref().is_some_and(|d| d.cover.is_none()))
        .await;

    store_test_cover(manager, &release_id, b"cover").await;

    draft_until(&draft, "the cover", |draft| {
        draft
            .as_ref()
            .is_some_and(|draft| draft.cover.as_ref().is_some_and(|cover| cover.id == release_id))
    })
    .await;
}

#[tokio::test]
async fn the_live_draft_names_the_renamed_album() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let album_id = manager.records.find_album_id_for_release(&release_id).await.unwrap().unwrap();
    let form = manager.open_release_edit(&release_id).await.unwrap();
    let draft = draft_of(manager, &form);

    manager.records.rename_album_for_test(&album_id, "Renamed Album").await.unwrap();

    draft_until(&draft, "the album's new title", |draft| {
        matches!(
            draft.as_ref().map(|draft| &draft.album),
            Some(bae_import::DraftAlbum::Named {
                line:
                    bae_import::AlbumLine::TitleAlone { title }
                    | bae_import::AlbumLine::TitleAndArtists { title, .. },
                ..
            }) if title == "Renamed Album"
        )
    })
    .await;
}

#[tokio::test]
async fn the_live_draft_is_none_once_the_release_is_deleted() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let form = manager.open_release_edit(&release_id).await.unwrap();
    let draft = draft_of(manager, &form);
    draft_until(&draft, "the draft", Option::is_some).await;

    manager.delete_release(&release_id).await.unwrap();

    draft_until(&draft, "no draft", Option::is_none).await;
}
