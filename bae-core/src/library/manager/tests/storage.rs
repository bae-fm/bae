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
        insert_release(manager, &release).await;
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

/// A page of the storage list under `filter`, ordered by album title
/// ascending — the ordering every assertion below reads rows in.
async fn storage_page(
    manager: &LibraryManager,
    filter: crate::db::StorageFilter,
    offset: u64,
    limit: u64,
) -> StoragePage {
    manager
        .get_storage_page(&sort_by_album_title_asc(), filter, offset, limit)
        .await
        .unwrap()
}

#[tokio::test]
async fn storage_page_returns_all_rows_for_all_filter() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = storage_page(&manager, crate::db::StorageFilter::All, 0, 10).await;
    assert_eq!(page.rows.len(), 3);
    assert_eq!(page.total_count, 3);
}

#[tokio::test]
async fn storage_page_paginates() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 5).await;

    let page1 = storage_page(&manager, crate::db::StorageFilter::All, 0, 2).await;
    let page2 = storage_page(&manager, crate::db::StorageFilter::All, 2, 2).await;
    let page3 = storage_page(&manager, crate::db::StorageFilter::All, 4, 2).await;

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

    let page = storage_page(&manager, crate::db::StorageFilter::All, 0, 10).await;
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
    let local = create_test_release(&local_album.id);
    manager.database.insert_album(&local_album).await.unwrap();
    insert_release(&manager, &local).await;

    let page = storage_page(&manager, crate::db::StorageFilter::All, 0, 10).await;
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
        if pin {
            wait_for_offline_copy_request(&manager, &release.id).await;
        }
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
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;

    let page = storage_page(&manager, crate::db::StorageFilter::All, 0, 10).await;
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].release.storage_actions.is_empty());
}

#[tokio::test]
async fn storage_page_local_filter_matches_local_path() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let remote_id = make_remote_release(&manager, &temp_dir.path().join("remote"), "Remote Album", false).await;
    let local_release = insert_local_release_with_files(&manager, &temp_dir.path().join("local"), "Local Album", &[("track.flac", b"local")]).await;

    let local = storage_page(&manager, crate::db::StorageFilter::Local, 0, 10).await;
    assert_eq!(local.rows.len(), 1);
    assert_eq!(local.total_count, 1);
    assert_eq!(local.rows[0].release.id, local_release.id);
    assert_eq!(
        local.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    let remote = storage_page(&manager, crate::db::StorageFilter::Remote, 0, 10).await;
    assert_eq!(remote.rows.len(), 1);
    assert_eq!(remote.rows[0].release.id, remote_id);
    assert_ne!(
        remote.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );
}

#[tokio::test]
async fn storage_count_matches_filtered_page_total() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let mut inserted_local = None;
    for i in 0..3 {
        let release = insert_local_release_with_files(&manager, &temp_dir.path().join(format!("album-{i}")), &format!("Album {i}"), &[("track.flac", b"track")]).await;
        if i == 1 {
            inserted_local = Some(release.id);
        } else {
            manager.coven_make_remote(&release.id, false).await.unwrap();
            manager.drain_uploads_expecting_work().await.unwrap();
        }
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

    let all_page = storage_page(&manager, crate::db::StorageFilter::All, 0, 10).await;
    assert_eq!(all_page.total_count, 3);

    let local_page = storage_page(&manager, crate::db::StorageFilter::Local, 0, 10).await;
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

    insert_local_release_with_files(&manager, &temp_dir.path().join("local"), "Local Album", &[("a.flac", &vec![0u8; 1_000])]).await;
    make_remote_release_with_files(&manager, &temp_dir.path().join("remote"), "Remote Album", &[("a.flac", &[0u8; 100])], false).await;
    insert_release_with_queued_uploads(&manager, &temp_dir.path().join("uploading"), "Uploading Album", &[("a.flac", &vec![0u8; 10_000])]).await;

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

    // Attaching a file makes the release remote while its upload is pending.
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
        1_000
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        10_100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        10_000
    );
}

/// Audio stays pinned while the cover remains in its separate evictable cache.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unpin_release_releases_audio_and_keeps_cover_evictable() {
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
    wait_for_offline_copy_request(&manager, &release.id).await;
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 2, "the file and the cover both upload");
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    assert!(
        blob_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "make-Remote with pin leaves the release file pinned"
    );
    assert!(
        !blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "covers remain evictable when audio is pinned"
    );

    manager.unpin_release(&release.id).await.unwrap();

    assert!(
        !blob_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file unpinned"
    );
    assert!(
        !blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "the cover remains evictable"
    );
}

/// Pin progress counts only the release's files; cover art keeps its own budget.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn pin_release_counts_audio_and_keeps_cover_evictable() {
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

    let expected_total = file_bytes.len() as u64;
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

    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    assert!(
        blob_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file pinned"
    );
    assert!(
        !blob_pinned(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await,
        "the cover remains evictable"
    );
    let last = progress.last().expect("the pin reports progress");
    assert_eq!(last.bytes_done, expected_total);
    assert_eq!(last.bytes_total, expected_total);
    assert_eq!(last.fraction, 1.0, "the bar lands on its denominator");
}

/// A pin reaches the storage view as it lands, from coven's pin-state watch:
/// no sync cycle runs behind this connection and no transfer reports it.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_pin_reaches_the_storage_view_without_a_sync_cycle() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pin-watch"),
        "Pin Watch",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.coven_make_remote(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();

    let services = crate::library::AppServices::for_test(manager.clone())
        .await
        .unwrap();
    let values = services.subscribe_storage_browse(
        &tokio::runtime::Handle::current(),
        crate::library::StorageBrowseView {
            sort: crate::db::StorageSortCriterion {
                field: crate::db::StorageSortField::AlbumTitle,
                direction: crate::db::SortDirection::Ascending,
            },
            filter: crate::db::StorageFilter::All,
            windows: [crate::library::LibraryPageWindow {
                offset: 0,
                limit: 50,
            }]
            .into_iter()
            .collect(),
        },
    );
    let pinned = |value: &crate::library::StorageBrowseSnapshot| {
        value
            .windows
            .iter()
            .flat_map(|window| &window.rows)
            .find(|row| row.release.id == release.id)
            .map(|row| row.release.pinned)
    };
    let first = values.next().await.unwrap();
    assert_eq!(pinned(&first), Some(false));

    manager
        .pin_release_blobs_with_progress(&release.id, |_| {})
        .await
        .unwrap();

    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), values.next())
            .await
            .expect("the pin reaches the storage view")
            .unwrap();
        if pinned(&value) == Some(true) {
            break;
        }
    }
}

/// A pin the provider refuses — bad credentials, a missing bucket — is about
/// the cloud setup the person fixes, not an internal fault.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_pin_the_provider_refuses_is_a_cloud_setup_failure() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pin-refused"),
        "Pin Refused",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.coven_make_remote(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    manager
        .evict_blob_for_test(&manager.release_file_row_blob_ref(&file_id).await.unwrap())
        .await
        .unwrap();

    for failure in [
        coven::StorageFailure::Authentication,
        coven::StorageFailure::ContainerNotFound,
    ] {
        let mut faults = coven::Faults::none();
        faults.fail_next = 1;
        faults.failure = failure;
        home.set_faults(faults).await;
        let error = manager
            .pin_release_blobs_with_progress(&release.id, |_| {})
            .await
            .expect_err("the provider refuses the read");

        assert_eq!(
            error.category(),
            crate::ui::UiErrorCategory::Credentials,
            "{failure:?}: {error}"
        );
    }
}

/// A pin that cannot reach the cloud is a network failure the person retries
/// once they are back online, not an internal fault: coven's typed reason
/// reaches the category the UI renders.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_pin_the_cloud_cannot_serve_is_a_network_failure() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pin-offline"),
        "Pin Offline",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.coven_make_remote(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    manager
        .evict_blob_for_test(&manager.release_file_row_blob_ref(&file_id).await.unwrap())
        .await
        .unwrap();
    home.set_online(false);

    let error = manager
        .pin_release_blobs_with_progress(&release.id, |_| {})
        .await
        .expect_err("the pin cannot fetch the file");

    assert_eq!(error.category(), crate::ui::UiErrorCategory::Network, "{error}");
}

/// Attaching files requires a configured provider; absence is a setup issue.
#[tokio::test]
async fn making_a_release_remote_without_storage_requires_configuration() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let transfer = crate::storage::transfer::ReleaseTransfer::new(
        manager.library.clone(), release.id, ReleaseStorageAction::MakeRemote);
    let error = transfer.start().await.expect_err("storage must be configured");
    assert_eq!(error.category(), crate::ui::UiErrorCategory::Config, "{error}");
}

#[cfg(feature = "test-utils")]
async fn next_storage_value(
    values: &crate::library::StorageBrowseSubscription,
) -> crate::library::StorageBrowseSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(5), values.next())
        .await
        .expect("the storage view delivers")
        .expect("the storage view resolves")
}

/// The Storage Manager reads through one subscription: moving its windows,
/// its sort, or its filter delivers the new view on the same subscription,
/// with the filtered set's count and total size beside every value.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_storage_view_moves_one_subscription() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;
    let services = crate::library::AppServices::for_test(manager.clone())
        .await
        .unwrap();
    let window = |offset, limit| crate::library::LibraryPageWindow { offset, limit };
    let view = |direction, filter, windows: &[crate::library::LibraryPageWindow]| {
        crate::library::StorageBrowseView {
            sort: crate::db::StorageSortCriterion {
                field: crate::db::StorageSortField::AlbumTitle,
                direction,
            },
            filter,
            windows: windows.iter().cloned().collect(),
        }
    };
    let titles = |value: &crate::library::StorageBrowseSnapshot| {
        value
            .windows
            .iter()
            .map(|window| {
                window
                    .rows
                    .iter()
                    .map(|row| row.album.title.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let ascending = crate::db::SortDirection::Ascending;
    let all = crate::db::StorageFilter::All;
    let values = services.subscribe_storage_browse(
        &tokio::runtime::Handle::current(),
        view(ascending, all, &[window(0, 2)]),
    );

    let first = next_storage_value(&values).await;
    assert_eq!(titles(&first), vec![vec!["Album 0", "Album 1"]]);
    assert_eq!(first.total_count, 3);

    values
        .set(view(ascending, all, &[window(0, 1), window(2, 1)]))
        .unwrap();
    let scrolled = next_storage_value(&values).await;
    assert_eq!(titles(&scrolled), vec![vec!["Album 0"], vec!["Album 2"]]);
    assert_eq!(scrolled.total_count, 3, "one count answers every window");

    let descending = crate::db::SortDirection::Descending;
    values
        .set(view(descending, all, &[window(0, 2)]))
        .unwrap();
    let resorted = next_storage_value(&values).await;
    assert_eq!(resorted.sort.direction, descending);
    assert_eq!(titles(&resorted), vec![vec!["Album 2", "Album 1"]]);

    let local = crate::db::StorageFilter::Local;
    values
        .set(view(descending, local, &[window(0, 2)]))
        .unwrap();
    let filtered = next_storage_value(&values).await;
    assert_eq!(filtered.filter, local);
    assert_eq!(
        filtered.total_count,
        manager.get_storage_count(local).await.unwrap()
    );
    assert_eq!(
        filtered.total_size,
        manager.get_storage_total_size(local).await.unwrap()
    );
}
