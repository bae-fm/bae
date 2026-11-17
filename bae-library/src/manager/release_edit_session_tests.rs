//! A release edit session's rows: what each change writes, what a save and
//! Cancel write again, and when the rows go.

use super::tests::insert_editable_tracks;
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

/// Wait until the session `session` reads as closed.
async fn until_closed(manager: &LibraryManager, session: i64) {
    let mut draft = ImportTables::new(manager.store.clone()).subscribe_release_edit_draft(session);
    tokio::time::timeout(HANG_GUARD, async {
        while draft.next_inline().await.expect("the query stays open").unwrap().is_some() {}
    })
    .await
    .expect("the session's rows go");
}

#[tokio::test]
async fn a_session_holds_its_changes_in_its_rows() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = release_of_two_tracks(manager).await;
    let session = manager.open_release_edit(&release_id).await.unwrap();

    manager.edit_release_edit_field(&session, album_title("Edited Title")).await.unwrap();

    let stored = ImportTables::new(manager.store.clone())
        .load_release_edit_session(session.session())
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
    let id = session.session();

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

    assert!(
        manager.release_edit_draft(&session).await.is_err(),
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

    assert!(matches!(
        manager.release_edit_draft(&session).await,
        Err(LibraryError::ReleaseEditClosed { .. })
    ));
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

    let draft = manager.release_edit_draft(&session).await.unwrap();
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
    let opened = manager.release_edit_draft(&session).await.unwrap();
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

    let reopened = manager.reopen_release_edit(&session).await.unwrap();

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
        .close_release_edit_session(session.session())
        .await
        .unwrap();

    let refused = manager.edit_release_edit_field(&session, album_title("Too Late")).await;

    assert!(matches!(refused, Err(LibraryError::ReleaseEditClosed { .. })), "{refused:?}");
}
