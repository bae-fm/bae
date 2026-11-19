//! The album a release editor's form files its release under: named, picked
//! and new.

use super::tests::{
    album_pick, draft_now, insert_editable_tracks, musicbrainz_link, seed_musicbrainz,
};
use crate::manager::tests::*;
use bae_records::DbAlbum;

/// The form's draft says which album a save files the release under: the
/// one it is in, the one an applied catalog's records name (new where the
/// library holds none), or the one the person picked, which an apply keeps
/// and a fresh open drops. A library album the person picked stops being
/// the pick once someone deletes it, so a save keeps the release's album.
#[tokio::test]
async fn a_release_edit_form_names_the_album_its_save_files_under() {
    use bae_import::DraftAlbum;
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let grouped = DbAlbum { title: "Grouped Album".to_string(), ..create_test_album() };
    let sibling = create_test_release(&grouped.id);
    manager.records.insert_album(&grouped).await.unwrap();
    insert_release(manager, &sibling).await;
    manager
        .records
        .insert_release_records(&sibling.id, &[mb_identity("named-group", "named-sibling")])
        .await
        .unwrap();
    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 1).await;
    let picked = DbAlbum { title: "Picked Album".to_string(), ..create_test_album() };
    manager.records.insert_album(&picked).await.unwrap();
    seed_musicbrainz(&test, "named-new", "named-group", 1);
    seed_musicbrainz(&test, "unnamed-new", "unnamed-group", 1);
    let named = |album: &DbAlbum| DraftAlbum::Named {
        album_id: album.id.clone(),
        line: bae_import::AlbumLine::TitleAlone { title: album.title.clone() },
    };
    let picked_album = DraftAlbum::Picked {
        album_id: picked.id.clone(),
        line: bae_import::AlbumLine::TitleAlone { title: picked.title.clone() },
    };

    let form = manager.open_release_edit(&release.id).await.unwrap();
    assert_eq!(form.opened.album, named(&own));
    let apply = |form, link| async move {
        manager
            .apply_release_edit_catalog(
                form,
                &musicbrainz_link(link),
                &bae_import::CatalogApplySteps::unread(),
            )
            .await
            .unwrap()
    };
    let form = apply(form, "named-new").await;
    assert_eq!(draft_now(manager, &form).await.album, named(&grouped));
    let mut form = apply(form, "unnamed-new").await;
    assert_eq!(draft_now(manager, &form).await.album, DraftAlbum::New);

    form.album_pick = album_pick(&picked);
    assert_eq!(draft_now(manager, &form).await.album, picked_album);
    let mut missing = form.clone();
    missing.album_pick = bae_import::AlbumPick::Album { album_id: "no-such-album".to_string() };
    assert_eq!(
        draft_now(manager, &missing).await.album,
        DraftAlbum::New,
        "the draft shows no pick of an album the library does not hold"
    );
    let mut form = apply(form, "named-new").await;
    assert_eq!(
        draft_now(manager, &form).await.album,
        picked_album,
        "an apply keeps the person's pick"
    );
    form.album_pick = bae_import::AlbumPick::Records;
    assert_eq!(
        draft_now(manager, &form).await.album,
        named(&grouped),
        "with no pick, the form files the release under the album its records name"
    );

    let mut deleted_pick = manager.open_release_edit(&release.id).await.unwrap();
    deleted_pick.album_pick =
        bae_import::AlbumPick::Album { album_id: "no-such-album".to_string() };
    deleted_pick.edit.set_album_artists(vec![bae_import::Credited::plain(
        bae_import::ArtistAssignment::named("Artist Name"),
    )]);
    let refused =
        manager.save_release_edit(deleted_pick.clone(), super::PickedAlbumGone::Refuse).await;
    assert!(
        matches!(
            &refused,
            Err(super::LibraryError::AlbumMissing { album_id }) if album_id == "no-such-album"
        ),
        "a caller that names the album itself is refused: {refused:?}"
    );
    manager
        .save_release_edit(deleted_pick, super::PickedAlbumGone::Unpick)
        .await
        .expect("a deleted pick is no pick");
    assert_eq!(
        manager.get_album_id_for_release(&release.id).await.unwrap(),
        own.id,
        "a save with a deleted pick keeps the release's album"
    );
}

/// A new album picked over the one a release is in files the release alone
/// under a fresh album on save, and the album it left keeps its other
/// releases.
#[tokio::test]
async fn a_picked_new_album_files_the_release_under_a_fresh_album() {
    use bae_import::DraftAlbum;
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let own = DbAlbum { title: "Own Album".to_string(), ..create_test_album() };
    let release = create_test_release(&own.id);
    let sibling = create_test_release(&own.id);
    manager.records.insert_album(&own).await.unwrap();
    insert_release(manager, &release).await;
    insert_release(manager, &sibling).await;
    insert_editable_tracks(manager, &release.id, 1).await;

    let mut form = manager.open_release_edit(&release.id).await.unwrap();
    form.album_pick = bae_import::AlbumPick::New;
    assert_eq!(draft_now(manager, &form).await.album, DraftAlbum::PickedNew);
    form.edit.set_album_artists(vec![bae_import::Credited::plain(
        bae_import::ArtistAssignment::named("Artist Name"),
    )]);
    manager.save_release_edit(form, super::PickedAlbumGone::Unpick).await.unwrap();

    let filed = manager.records.find_album_id_for_release(&release.id).await.unwrap().unwrap();
    assert_ne!(filed, own.id, "the release left its album for a new one");
    assert_eq!(
        manager.records.find_album_id_for_release(&sibling.id).await.unwrap(),
        Some(own.id.clone()),
        "the album it left keeps its other release"
    );
}
