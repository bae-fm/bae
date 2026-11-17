//! A release edit session's rows: what each change writes, what a save and
//! Cancel write again, and when the rows go.

use super::tests::{draft_after, draft_now, insert_editable_tracks};
use super::*;
use crate::manager::tests::*;
use bae_import::tables::ImportTables;
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

fn album_title(title: &str) -> bae_import::DraftFieldEdit {
    bae_import::DraftFieldEdit::Text {
        field: bae_import::CandidateEditField::AlbumTitle,
        value: title.to_string(),
    }
}

/// Wait until the live draft query for `session` returns no draft.
async fn until_closed(manager: &LibraryManager, session: i64) {
    let mut draft = ImportTables::new(manager.store.clone()).subscribe_release_edit_draft(session);
    tokio::time::timeout(HANG_GUARD, async {
        while draft.next_inline().await.expect("the query stays open").unwrap().is_some() {}
    })
    .await
    .expect("bae deletes the session's rows");
}

#[tokio::test]
async fn a_session_holds_its_changes_in_its_rows() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();

    manager.edit_release_edit_field(&session, album_title("Edited Title")).await.unwrap();

    let stored = ImportTables::new(manager.store.clone())
        .load_release_edit_session(session.session)
        .await
        .unwrap()
        .expect("the session is open");
    assert_eq!(stored.form.album_title, "Edited Title");
    assert_ne!(stored.opened.edit.album_title, "Edited Title");
    assert_eq!(stored.release_id, release_id);
}

#[tokio::test]
async fn closing_a_session_deletes_its_rows() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();
    let id = session.session;

    drop(session);

    until_closed(manager, id).await;
}

#[tokio::test]
async fn opening_the_library_deletes_sessions_left_behind() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();

    bae_import::tables::ImportTables::new(manager.store.clone())
        .clear_release_edit_sessions()
        .await
        .unwrap();

    assert_eq!(
        manager.subscribe_release_edit_draft(&session).next().await.expect("the draft reads"),
        None,
        "the library opens with no session"
    );
}

#[tokio::test]
async fn deleting_the_release_deletes_its_sessions() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();

    manager.delete_release(&release_id).await.unwrap();

    assert_eq!(
        manager.subscribe_release_edit_draft(&session).next().await.expect("the draft reads"),
        None
    );
}

#[tokio::test]
async fn a_save_reopens_the_session_on_what_it_saved_in_its_write() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();
    manager.edit_release_edit_field(&session, album_title("Saved Title")).await.unwrap();
    manager
        .set_release_edit_album_artists(
            &session,
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();

    manager.save_release_edit(&session).await.unwrap();

    let draft = draft_now(manager, &session).await;
    assert_eq!(draft.edit.album_title, "Saved Title");
    assert!(!draft.changed, "the session opened again on what it saved");
    manager.edit_release_edit_field(&session, album_title("Saved Again")).await.unwrap();
    manager.save_release_edit(&session).await.unwrap();
}

#[tokio::test]
async fn cancel_reopens_the_session_on_the_release_as_it_is() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();
    let opened = draft_now(manager, &session).await;
    manager.edit_release_edit_field(&session, album_title("Discarded")).await.unwrap();
    let other = manager.open_release_edit(&release_id).await.unwrap();
    manager.edit_release_edit_field(&other, album_title("Written Elsewhere")).await.unwrap();
    manager
        .set_release_edit_album_artists(
            &other,
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    manager.save_release_edit(&other).await.unwrap();

    let reopened = draft_after(manager, &session, manager.reopen_release_edit(&session)).await;

    assert_eq!(reopened.edit.album_title, "Written Elsewhere");
    assert!(!reopened.changed);
    assert_ne!(reopened.edit, opened.edit);
}

#[tokio::test]
async fn a_change_to_a_closed_session_is_refused() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();
    ImportTables::new(manager.store.clone())
        .close_release_edit_session(session.session)
        .await
        .unwrap();

    let refused = manager.edit_release_edit_field(&session, album_title("Too Late")).await;

    assert!(matches!(refused, Err(LibraryError::ReleaseEditClosed { .. })), "{refused:?}");
}

/// Pull `draft` until its value holds `wanted`.
async fn draft_until(
    draft: &bae_store::live_query::LiveValues<Option<bae_import::ReleaseEditDraft>>,
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

#[tokio::test]
async fn the_live_draft_shows_a_merged_artist_without_an_edit() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();
    manager
        .set_release_edit_album_artists(
            &session,
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Newcomer"))],
        )
        .await
        .unwrap();
    let draft = manager.subscribe_release_edit_draft(&session);
    let stands = |wanted: bae_import::ArtistStanding| {
        move |draft: &Option<bae_import::ReleaseEditDraft>| {
            draft.as_ref().is_some_and(|draft| {
                draft
                    .album_artist_standing
                    .as_ref()
                    .is_some_and(|standing| standing.artists == std::slice::from_ref(&wanted))
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

#[tokio::test]
async fn the_live_draft_shows_a_cover_picked_after_it_opened() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();
    let draft = manager.subscribe_release_edit_draft(&session);
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
    let session = manager.open_release_edit(&release_id).await.unwrap();
    let draft = manager.subscribe_release_edit_draft(&session);

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
    let session = manager.open_release_edit(&release_id).await.unwrap();
    let draft = manager.subscribe_release_edit_draft(&session);
    draft_until(&draft, "the draft", Option::is_some).await;

    manager.delete_release(&release_id).await.unwrap();

    draft_until(&draft, "no draft", Option::is_none).await;
}
