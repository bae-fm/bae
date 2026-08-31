/// Insert N albums each with one release; return `(albums, releases)`.
/// Each album's title is `"Album {i}"` so ordering is deterministic.
async fn seed_albums(manager: &LibraryManager, count: usize) -> (Vec<DbAlbum>, Vec<DbRelease>) {
    let mut albums = Vec::new();
    let mut releases = Vec::new();
    for i in 0..count {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        albums.push(album);
        releases.push(release);
    }
    (albums, releases)
}

fn sort_by_album_title_asc() -> crate::db::StorageSortCriterion {
    crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    }
}

#[tokio::test]
async fn storage_page_returns_all_rows_for_all_filter() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
    assert_eq!(page.total_count, 3);
}

#[tokio::test]
async fn storage_page_paginates() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 5).await;

    let page1 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            2,
        )
        .await
        .unwrap();
    let page2 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            2,
            2,
        )
        .await
        .unwrap();
    let page3 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            4,
            2,
        )
        .await
        .unwrap();

    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page2.rows.len(), 2);
    assert_eq!(page3.rows.len(), 1);
    // total_count is the full filtered universe, not the page.
    assert_eq!(page1.total_count, 5);
    assert_eq!(page2.total_count, 5);
    assert_eq!(page3.total_count, 5);
}

#[tokio::test]
async fn storage_page_sorts_album_title_ascending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 0", "Album 1", "Album 2"]);
}

#[tokio::test]
async fn storage_page_sorts_album_title_descending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 2", "Album 1", "Album 0"]);
}

/// Each storage-page row carries the state-appropriate `storage_actions`
/// the Storage Manager row context menu renders — pinned offers unpin +
/// make-Local, cloud-only offers pin + make-Local, local offers make-Remote.
/// With a cloud home present every remote/local transition is open.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_rows_carry_state_appropriate_actions() {
    use crate::album_detail::ReleaseStorageAction::{MakeLocal, MakeRemote, Pin, Unpin};

    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    // Pinned: made Remote with pin, so its blob lands in coven's offline cache.
    let pinned = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("pinned"),
        "Pinned Album",
        true,
    )
    .await;
    // Cloud-only: made Remote without pin, so its blob is evictable, not pinned.
    let cloud_only = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("cloud"),
        "Cloud Album",
        false,
    )
    .await;

    // Local: not remote, files at a local path.
    let mut local_album = create_test_album();
    local_album.title = "Local Album".to_string();
    let mut local = create_test_release(&local_album.id);
    local.remote = false;
    manager.database.insert_album(&local_album).await.unwrap();
    manager.database.insert_release(&local).await.unwrap();

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let actions: std::collections::HashMap<_, _> = page
        .rows
        .iter()
        .map(|r| (r.release.id.clone(), r.release.storage_actions.clone()))
        .collect();

    assert_eq!(actions[&pinned], vec![Unpin, MakeLocal]);
    assert_eq!(actions[&cloud_only], vec![Pin, MakeLocal]);
    assert_eq!(actions[&local.id], vec![MakeRemote]);
}

/// Each release of an album carries its own pin state. The detail resolves the
/// whole album's pin markers in one read, so every answer has to land back on
/// the release it was asked about.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn album_detail_releases_carry_their_own_pin_state() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    let mut album = create_test_album();
    album.title = "Two Pressings".to_string();
    manager.database.insert_album(&album).await.unwrap();

    let mut pressings = Vec::new();
    for (index, pin) in [false, true].into_iter().enumerate() {
        let release = insert_local_release_in_album(
            &manager,
            &album.id,
            &temp_dir.path().join(format!("pressing-{index}")),
            &[("track.flac", b"track-bytes")],
        )
        .await;
        manager.coven_make_remote(&release.id, pin).await.unwrap();
        wait_for_landed_make_remote(&manager, &release.id).await;
        pressings.push((release.id, pin));
    }

    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("the album surfaces");
    let pinned: std::collections::HashMap<_, _> = detail
        .releases
        .iter()
        .map(|release| (release.summary.id.clone(), release.summary.pinned))
        .collect();

    assert_eq!(pinned.len(), pressings.len());
    for (release_id, pin) in pressings {
        assert_eq!(
            pinned[&release_id], pin,
            "the pin answer for {release_id} belongs to that release",
        );
    }
}

/// With no cloud home, no remote storage exists, so the rows offer no
/// transitions — the context menu is empty everywhere.
#[tokio::test]
async fn storage_page_rows_have_no_actions_without_cloud_home() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].release.storage_actions.is_empty());
}

#[tokio::test]
async fn storage_page_local_filter_matches_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album_remote = create_test_album();
    let mut album_local = create_test_album();
    album_local.title = "Local Album".to_string();
    let remote_release = create_test_release(&album_remote.id);
    let mut local_release = create_test_release(&album_local.id);
    local_release.remote = false;

    manager.database.insert_album(&album_remote).await.unwrap();
    manager.database.insert_album(&album_local).await.unwrap();
    manager
        .database
        .insert_release(&remote_release)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&local_release)
        .await
        .unwrap();

    let local = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Local,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(local.rows.len(), 1);
    assert_eq!(local.total_count, 1);
    assert_eq!(local.rows[0].release.id, local_release.id);
    assert_eq!(
        local.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    let remote = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Remote,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(remote.rows.len(), 1);
    assert_eq!(remote.rows[0].release.id, remote_release.id);
    assert_ne!(
        remote.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );
}

#[tokio::test]
async fn storage_count_matches_filtered_page_total() {
    let (manager, _temp_dir) = setup_test_manager().await;
    // Three albums, one release each. Mark the second release
    // local at insert time so filters produce distinct counts.
    let mut inserted_local = None;
    for i in 0..3 {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let mut release = create_test_release(&album.id);
        let local = i == 1;
        if local {
            release.remote = false;
            inserted_local = Some(release.id.clone());
        }
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::All)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Local)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        0
    );

    let all_page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(all_page.total_count, 3);

    let local_page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Local,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(local_page.rows.len(), 1);
    assert_eq!(local_page.rows[0].release.id, inserted_local.unwrap());
}

/// `get_storage_total_size` sums `total_size` over every storage row matching
/// `filter` — the same universe `get_storage_page` pages over — independent of
/// how many pages have loaded. For each filter, the aggregate must equal the
/// sum of `total_size` over that filter's full (unpaginated) storage page.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_total_size_matches_page_total_size_sum() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    // Three releases: one local, one remote-and-quiet, and one mid-make-Remote
    // — its uploads are queued but undrained, so its gate has not flipped and it
    // is still Local as well as Uploading.
    let album_local = create_test_album();
    let album_remote = create_test_album();
    let mut release_local = create_test_release(&album_local.id);
    release_local.remote = false;
    let release_remote = create_test_release(&album_remote.id);

    manager.database.insert_album(&album_local).await.unwrap();
    manager.database.insert_album(&album_remote).await.unwrap();
    manager
        .database
        .insert_release(&release_local)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_remote)
        .await
        .unwrap();

    for (release_id, file_size) in [(&release_local.id, 1_000i64), (&release_remote.id, 100)] {
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{release_id}-file")),
            release_id: release_id.clone(),
            original_filename: "a.flac".to_string(),
            file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            source_audio: None,
            cloud_path: None,
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    // The uploading release goes through the real transition, so its 10_000
    // bytes are what coven actually has queued.
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("uploading"),
        "Uploading Album",
        &[("a.flac", &vec![0u8; 10_000])],
    )
    .await;

    for filter in [
        crate::db::StorageFilter::All,
        crate::db::StorageFilter::Local,
        crate::db::StorageFilter::Remote,
        crate::db::StorageFilter::Uploading,
    ] {
        let page = manager
            .get_storage_page(&sort_by_album_title_asc(), filter, 0, 10)
            .await
            .unwrap();
        let page_sum: i64 = page.rows.iter().map(|row| row.release.total_size).sum();

        let aggregate = manager.get_storage_total_size(filter).await.unwrap();
        assert_eq!(
            aggregate, page_sum as u64,
            "{filter:?}: aggregate must equal the page's own total_size sum"
        );
    }

    // Concrete expectations, so a bug that moves the *same* wrong figure on
    // both sides doesn't slip through. The uploading release counts as Local
    // too — its gate flips only once every upload lands.
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::All)
            .await
            .unwrap(),
        11_100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Local)
            .await
            .unwrap(),
        11_000
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        10_000
    );
}

/// A pinned release's cover belongs to its pinned set: coven's `make_remote`
/// pins the whole root batch, cover included, so unpinning has to move all of it
/// to the evictable cache. Enumerating `release_files` alone left the cover in
/// `storage/pinned/` forever — unreachable by any later unpin, since nothing
/// else names it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unpin_release_unpins_the_cover_with_its_files() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pinned-with-cover"),
        "Pinned With Cover",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    store_test_cover(&manager, &release.id, b"cover-bytes").await;

    manager.coven_make_remote(&release.id, true).await.unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 2, "the file and the cover both upload");
    let file_id = manager.database.get_files_for_release(&release.id).await.unwrap()[0]
        .id
        .clone();
    assert!(
        blob_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "make-Remote with pin leaves the release file pinned"
    );
    assert!(
        blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "make-Remote with pin leaves the cover pinned — it rides along in coven's batch"
    );

    manager.unpin_release(&release.id).await.unwrap();

    assert!(
        !blob_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file unpinned"
    );
    assert!(
        !blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "the cover unpinned with the files; nothing of the release stays pinned"
    );
}

/// The Pin transition covers the same set make-Remote pins: pinning a Remote
/// release from the UI must bring its cover into `storage/pinned/` too, or the
/// cover stays evictable and an offline library loses its art. The cover's bytes
/// count toward the Downloads pane's total, so the bar still lands exactly on
/// its denominator.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn pin_release_pins_the_cover_and_counts_its_bytes() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let file_bytes: &[u8] = b"track-bytes";
    let cover_bytes: &[u8] = b"cover-bytes-are-longer";
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("unpinned-with-cover"),
        "Unpinned With Cover",
        &[("track.flac", file_bytes)],
    )
    .await;
    store_test_cover(&manager, &release.id, cover_bytes).await;

    manager.coven_make_remote(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    assert!(
        !blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "make-Remote without pin leaves the cover evictable"
    );

    let expected_total = (file_bytes.len() + cover_bytes.len()) as u64;
    assert_eq!(
        manager
            .initial_download_progress(&release.id)
            .await
            .unwrap()
            .bytes_total,
        expected_total,
        "the pane's denominator counts every blob the pin fetches"
    );

    let mut progress = Vec::new();
    manager
        .pin_release_blobs_with_progress(&release.id, |update| progress.push(update))
        .await
        .unwrap();

    let file_id = manager.database.get_files_for_release(&release.id).await.unwrap()[0]
        .id
        .clone();
    assert!(
        blob_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file pinned"
    );
    assert!(
        blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "the cover pinned with the files"
    );
    let last = progress.last().expect("the pin reports progress");
    assert_eq!(last.bytes_done, expected_total);
    assert_eq!(last.bytes_total, expected_total);
    assert_eq!(last.fraction, 1.0, "the bar lands on its denominator");
}
