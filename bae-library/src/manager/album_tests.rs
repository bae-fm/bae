//! Deleting an album: its releases and what they own go with it, and an album
//! the library doesn't hold is refused.

use super::*;
use crate::manager::tests::*;
use bae_records::LibraryImageType;

/// delete_album removes each release's cover too (same helper, second wiring
/// site): the cover row is gone with its release.
#[tokio::test]
async fn delete_album_removes_release_covers() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;
    let release = ReleaseRef::of(
        manager,
        upload_release_unpaused(manager, &temp_dir.path().join("r1"), "Album One", false).await,
    )
    .await;
    store_test_cover(manager, &release.id, b"image").await;

    wait_for_published_file(manager, bae_records::synced::COVERS_NAMESPACE, &release.id).await;

    manager.delete_album(&release.album_id).await.unwrap();

    assert!(manager
        .records
        .find_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_delete_album_deletes_all_releases() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release1).await;
    insert_release(manager, &release2).await;

    manager.delete_album(&album.id).await.unwrap();

    let album_result = views(manager).find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());
    assert!(manager.get_album_by_id(&album.id).await.unwrap().is_none(), "the album row goes");
    let releases = album_releases(manager, &album.id).await;
    assert!(releases.is_empty());
}

/// Deleting an album deletes every release it holds and what each release
/// owns: its tracks and their places in the queue, its files and cover, and
/// the coven files they reference with the pins on them. The person's own
/// files stay, another album stays whole, and the album's live reads follow
/// the delete: the grid stops listing it and its detail reads none.
#[tokio::test]
async fn delete_album_deletes_its_releases_and_what_they_own() {
    let (test, temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    // The album holds a release uploaded and pinned on this device and one
    // whose files are on this device only.
    let kept = uploaded_release(manager, &temp.path().join("kept"), "Deleted", true).await;
    let album_id = find_release(manager, &kept).await.unwrap().album_id;
    let local_dir = temp.path().join("local");
    let local = insert_local_release_in_album(
        manager,
        &album_id,
        &local_dir,
        &[("a.flac", b"local audio")],
    )
    .await
    .id;
    let other_dir = temp.path().join("other");
    let other =
        insert_local_release_with_files(manager, &other_dir, "Kept", &[("b.flac", b"other audio")])
            .await;
    let mut tracks = Vec::new();
    for (index, release_id) in [&kept, &local, &other.id].into_iter().enumerate() {
        store_test_cover(manager, release_id, format!("cover {index}").as_bytes()).await;
        let track = bae_records::DbTrack::new_test(
            release_id,
            &coven::IdSource::new_id(&coven::UuidIds).to_string(),
            "Track",
            1,
        );
        manager.records.insert_track(&track).await.unwrap();
        tracks.push(track.id);
    }
    finish_bound_uploads(manager).await;
    bae_playback::tables::PlaybackQueueTables::new(manager.store.clone())
        .add_manual(tracks.clone(), bae_playback::tables::ManualPlace::End)
        .await
        .unwrap();
    let coven_files_of = |release_ids: &[&str]| {
        let ids = release_ids.iter().map(|id| format!("'{id}'")).collect::<Vec<_>>().join(", ");
        format!(
            "SELECT file FROM release_files WHERE release_id IN ({ids}) \
             UNION ALL SELECT file FROM covers WHERE id IN ({ids})"
        )
    };
    let deleted_files =
        manager.store.query_texts_for_test(&coven_files_of(&[&kept, &local])).await.unwrap();
    assert_eq!(deleted_files.len(), 4, "two files and two covers");
    let mut pinned = 0;
    for file in &deleted_files {
        pinned += manager.store.count_rows_for_test("coven_pins", "file", file).await.unwrap();
    }
    assert!(pinned > 0, "the pinned release's file is pinned here");
    let kept_files =
        manager.store.query_texts_for_test(&coven_files_of(&[&other.id])).await.unwrap();

    let mut browse = views(manager).subscribe_album_browse(
        &[],
        false,
        [bae_store::LibraryPageWindow::at(0, 10)].into(),
    );
    let listed = |browse: &bae_views::AlbumBrowseProjection| -> Vec<String> {
        browse.windows[0].rows.iter().map(|row| row.album.id.clone()).collect()
    };
    let before = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(listed(&before).len(), 2);
    let mut detail = views(manager).subscribe_album_detail(Some(album_id.clone()));
    let shown = detail.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(shown.detail.expect("the album shows").releases.len(), 2);

    manager.delete_album(&album_id).await.unwrap();

    let after = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let value = browse.next().await.expect("the query stays open").result.unwrap();
            if value.total_count == 1 {
                return value;
            }
        }
    })
    .await
    .expect("the grid follows the delete");
    assert_eq!(listed(&after), std::slice::from_ref(&other.album_id));
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let value = detail.next().await.expect("the query stays open").result.unwrap();
            if value.detail.is_none() {
                return;
            }
        }
    })
    .await
    .expect("the detail follows the delete");

    for release_id in [&kept, &local] {
        assert!(find_release(manager, release_id).await.is_none());
        assert!(manager.records.get_tracks_for_release(release_id).await.unwrap().is_empty());
        assert!(release_files(manager, release_id).await.is_empty());
        assert!(manager
            .records
            .find_library_image(release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .is_none());
    }
    for file in &deleted_files {
        assert_eq!(manager.store.count_rows_for_test("coven_files", "id", file).await.unwrap(), 0);
        assert_eq!(manager.store.count_rows_for_test("coven_pins", "file", file).await.unwrap(), 0);
    }
    assert_eq!(std::fs::read(local_dir.join("a.flac")).unwrap(), b"local audio");
    assert_eq!(std::fs::read(temp.path().join("kept").join("track.flac")).unwrap(), b"track-bytes");

    assert!(find_release(manager, &other.id).await.is_some());
    assert_eq!(manager.records.get_tracks_for_release(&other.id).await.unwrap().len(), 1);
    assert!(manager
        .records
        .find_library_image(&other.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_some());
    for file in &kept_files {
        assert_eq!(manager.store.count_rows_for_test("coven_files", "id", file).await.unwrap(), 1);
    }
    let queued: Vec<String> = bae_playback::tables::queue_value(test.library.store())
        .await
        .unwrap()
        .manual
        .into_iter()
        .map(|item| item.track_id)
        .collect();
    assert_eq!(
        queued,
        std::slice::from_ref(&tracks[2]),
        "only the other album's track stays queued"
    );
}

/// An album holding no release is not in the library, so deleting it is
/// refused, as deleting a release the library doesn't hold is.
#[tokio::test]
async fn delete_album_refuses_an_album_the_library_does_not_hold() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    manager.library.manager().delete_release(&release.id).await.unwrap();

    for album_id in [album.id.as_str(), "not-an-album"] {
        let error = manager.library.manager().delete_album(album_id).await.unwrap_err();
        assert!(matches!(error, LibraryError::AlbumMissing { .. }), "{album_id}: {error}");
        assert_eq!(
            error.category(),
            bae_diagnostics::UiErrorCategory::ChangedMeanwhile,
            "as a stale deletion plan's is: {album_id}"
        );
    }
}
