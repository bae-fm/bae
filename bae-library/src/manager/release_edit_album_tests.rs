//! The album a release edit session files its release under: named, picked and new.

use super::tests::{album_pick, insert_editable_tracks, musicbrainz_link, seed_musicbrainz};
use crate::manager::tests::*;
use bae_records::DbAlbum;

/// The session says which album a save files the release under: the one it
/// is in, the one an applied catalog's records name (new where the library
/// holds none), or the one the person picked, which an apply keeps and a
/// fresh open drops.
#[tokio::test]
async fn a_release_edit_session_names_the_album_its_save_files_under() {
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
        title: album.title.clone(),
        artist_line: Default::default(),
    };
    let picked_album = DraftAlbum::Picked {
        album_id: picked.id.clone(),
        title: picked.title.clone(),
        artist_line: Default::default(),
    };

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
        manager.set_release_edit_album(&session, album_pick(&picked)).await.unwrap().album,
        picked_album
    );
    assert!(
        manager
            .set_release_edit_album(
                &session,
                bae_import::AlbumPick::Album { album_id: "no-such-album".to_string() },
            )
            .await
            .is_err(),
        "the session refuses an album the library does not hold"
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
        manager
            .set_release_edit_album(&session, bae_import::AlbumPick::Records)
            .await
            .unwrap()
            .album,
        named(&grouped),
        "with no pick, the session files the release under the album its records name"
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

    let session = manager.open_release_edit(&release.id).await.unwrap();
    assert_eq!(
        manager.set_release_edit_album(&session, bae_import::AlbumPick::New).await.unwrap().album,
        DraftAlbum::PickedNew
    );
    assert!(manager.set_release_edit_album(&session, bae_import::AlbumPick::Records).await.is_ok());
    manager.set_release_edit_album(&session, bae_import::AlbumPick::New).await.unwrap();
    manager
        .set_release_edit_album_artists(
            &session,
            vec![bae_import::Credited::plain(bae_import::ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();
    manager.save_release_edit(&session).await.unwrap();

    let filed = manager.records.find_album_id_for_release(&release.id).await.unwrap().unwrap();
    assert_ne!(filed, own.id, "the release left its album for a new one");
    assert_eq!(
        manager.records.find_album_id_for_release(&sibling.id).await.unwrap(),
        Some(own.id.clone()),
        "the album it left keeps its other release"
    );
}
