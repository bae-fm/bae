#[tokio::test]
async fn storage_page_sort_by_artist_names() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two artists with distinct sort-orderings; ArtistNames sort triggers
    // the `needs_artist_sort_join` branch.
    for (artist_id, artist_name) in &[
        ("ba5b6a6c-bc8c-4015-8b3c-03e78dfe28e5", "Zulu"),
        ("f2ad46f1-3a5e-4bb5-807f-5a314ae94f25", "Alpha"),
    ] {
        let artist = DbArtist {
            id: artist_id.to_string(),
            name: artist_name.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        manager.database.insert_artist(&artist).await.unwrap();
        let mut album = create_test_album();
        album.title = format!("Album by {artist_name}");
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager
            .database
            .insert_album_artist(&crate::db::DbAlbumArtist::new_test(&album.id, artist_id))
            .await
            .unwrap();
        insert_release(&manager, &release).await;
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::ArtistNames,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Alpha", "Album by Zulu"]);

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::ArtistNames,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Zulu", "Album by Alpha"]);
}

#[tokio::test]
async fn storage_page_sort_by_media_nulls_last() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, media) in &[
        ("Album No Media", None),
        ("Album CD", Some(bae_types::pressing::Medium::Cd)),
        ("Album Vinyl", Some(bae_types::pressing::Medium::Vinyl)),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.pressing.facts = media
            .map(|medium| bae_types::pressing::made_of(medium, 1))
            .unwrap_or_default();
        manager.database.insert_album(&album).await.unwrap();
        insert_release(&manager, &release).await;
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Media,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    // Unknown media sorts last in both directions.
    assert_eq!(titles, vec!["Album CD", "Album Vinyl", "Album No Media"]);

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Media,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album Vinyl", "Album CD", "Album No Media"]);
}

#[tokio::test]
async fn storage_page_sort_by_file_count() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Three releases, each with a distinct number of files.
    for (title, file_count) in &[("Album A", 1usize), ("Album B", 3), ("Album C", 2)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        insert_release(&manager, &release).await;
        for i in 0..*file_count {
            let file = DbFile {
                id: bae_test_support::test_uuid(&format!("{}-file-{i}", release.id)),
                release_id: release.id.clone(),
                original_filename: format!("{i}.flac"),
                file_size: 1000,
                content_type: bae_util::content_type::ContentType::Flac,
                source_audio: None,
                stored: false,
                created_at: Utc::now(),
            };
            manager.database.insert_file(&file).await.unwrap();
        }
    }

    let desc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::FileCount,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album B", "Album C", "Album A"]);
}

#[tokio::test]
async fn storage_page_sort_by_total_size() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, file_size) in &[("Small", 100i64), ("Big", 10_000), ("Medium", 1_000)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        insert_release(&manager, &release).await;
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{}-file", release.id)),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            file_size: *file_size,
            content_type: bae_util::content_type::ContentType::Flac,
            source_audio: None,
            stored: false,
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::TotalSize,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Small", "Medium", "Big"]);
}

/// The release ids of the Storage Manager list under `filter`, sorted by
/// storage in `direction`.
#[cfg(feature = "test-utils")]
async fn by_storage(
    manager: &LibraryManager,
    filter: crate::db::StorageFilter,
    direction: crate::db::SortDirection,
) -> Vec<String> {
    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Storage,
        direction,
    };
    manager
        .get_storage_page(&sort, filter, 0, 10)
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.release.id)
        .collect()
}

/// Sorting by storage puts the releases uploading first, then those waiting
/// or downloading to be pinned, then the local, the pinned and the
/// cloud-only ones, whatever their titles; descending reverses it, and each
/// filter keeps the order among the releases it shows.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_sort_by_storage_state() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let dir = |name: &str| temp_dir.path().join(name);

    let cloud = uploaded_release(&manager, &dir("cloud"), "Album Title A", false).await;
    let pinned = uploaded_release(&manager, &dir("pinned"), "Album Title B", true).await;
    let local = insert_local_release_with_files(
        &manager,
        &dir("local"),
        "Album Title C",
        &[("track.flac", b"local")],
    )
    .await
    .id;
    let pinning = uploaded_release(&manager, &dir("pinning"), "Album Title D", false).await;
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![pinning.clone()]).await.unwrap();
    let uploading = insert_release_with_queued_uploads(
        &manager,
        &dir("uploading"),
        "Album Title E",
        &[("track.flac", b"uploading")],
    )
    .await
    .id;

    use crate::db::{SortDirection, StorageFilter};
    let ascending = vec![
        uploading.clone(),
        pinning.clone(),
        local.clone(),
        pinned.clone(),
        cloud.clone(),
    ];
    assert_eq!(
        by_storage(&manager, StorageFilter::All, SortDirection::Ascending).await,
        ascending
    );
    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Storage,
        direction: SortDirection::Ascending,
    };
    use crate::album_detail::StorageRowStatus;
    assert_eq!(
        manager
            .get_storage_page(&sort, StorageFilter::All, 0, 10)
            .await
            .unwrap()
            .rows
            .iter()
            .map(|row| row.status)
            .collect::<Vec<_>>(),
        vec![
            StorageRowStatus::Uploading,
            StorageRowStatus::Pinning,
            StorageRowStatus::Local,
            StorageRowStatus::Pinned,
            StorageRowStatus::Cloud,
        ],
        "each row shows where it sorts"
    );
    let mut descending = ascending.clone();
    descending.reverse();
    assert_eq!(
        by_storage(&manager, StorageFilter::All, SortDirection::Descending).await,
        descending
    );
    assert_eq!(
        by_storage(&manager, StorageFilter::Local, SortDirection::Ascending).await,
        vec![uploading.clone(), local.clone()]
    );
    assert_eq!(
        by_storage(&manager, StorageFilter::Remote, SortDirection::Ascending).await,
        vec![pinning.clone(), pinned.clone(), cloud.clone()]
    );
}

/// A release is pinning while the download queue holds it waiting or
/// downloading, in the write that changes the queue: a failure, a cancel, a
/// finish and the queue emptied as the library opens each return it to its
/// resting place, and a retry or a refreshed listing row keep it pinning.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_release_sorts_as_pinning_while_the_download_queue_holds_it() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let dir = |name: &str| temp_dir.path().join(name);
    let first = uploaded_release(&manager, &dir("first"), "Album Title A", false).await;
    let second = uploaded_release(&manager, &dir("second"), "Album Title B", false).await;
    let ascending = || {
        by_storage(
            &manager,
            crate::db::StorageFilter::All,
            crate::db::SortDirection::Ascending,
        )
    };
    let resting = vec![first.clone(), second.clone()];
    let second_pinning = vec![second.clone(), first.clone()];
    assert_eq!(ascending().await, resting);

    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![second.clone()]).await.unwrap();
    assert_eq!(ascending().await, second_pinning, "queued");

    assert!(manager
        .database
        .activate_queued_release::<()>(&second)
        .await
        .unwrap());
    assert_eq!(ascending().await, second_pinning, "downloading");

    assert!(manager
        .database
        .fail_queued_release::<()>(&second, "the network connection was lost".into())
        .await
        .unwrap());
    assert_eq!(ascending().await, resting, "failed");

    manager.retry_downloads().await.unwrap();
    assert_eq!(ascending().await, second_pinning, "retried");

    // Rewriting the release's listing row reads the queue again.
    manager
        .database
        .execute_for_test(
            "UPDATE releases SET media = media WHERE id = ?1",
            vec![second.clone()],
        )
        .await
        .unwrap();
    assert_eq!(ascending().await, second_pinning, "refreshed");

    manager.cancel_download(&second).await.unwrap();
    assert_eq!(ascending().await, resting, "cancelled");

    manager.enqueue_pins(vec![second.clone()]).await.unwrap();
    assert!(manager
        .database
        .activate_queued_release::<()>(&second)
        .await
        .unwrap());
    assert!(manager
        .database
        .finish_queued_release::<()>(&second)
        .await
        .unwrap());
    assert_eq!(ascending().await, resting, "finished");

    manager.enqueue_pins(vec![second.clone()]).await.unwrap();
    manager.database.clear_release_queues().await.unwrap();
    assert_eq!(ascending().await, resting, "emptied as the library opens");
}

/// The queues the Storage Manager's list value carries, as one read of it
/// finds them.
#[cfg(feature = "test-utils")]
async fn listed_queues(manager: &LibraryManager) -> crate::library::StorageQueues {
    let mut browse = manager.database.subscribe_storage_browse(crate::db::StorageBrowseRequest {
        sort: crate::db::StorageSortCriterion {
            field: crate::db::StorageSortField::Storage,
            direction: crate::db::SortDirection::Ascending,
        },
        filter: crate::db::StorageFilter::All,
        windows: Default::default(),
        selected: Default::default(),
    });
    browse
        .next()
        .await
        .expect("the browse query answers")
        .result
        .unwrap()
        .queues
}

/// The list's value carries each transfer queue while it holds work waiting
/// or under way, with whether it is paused and the release a serial queue
/// runs; a queue holding only a failed entry holds none.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_storage_list_carries_the_queues_holding_work() {
    use crate::library::{ReleaseQueueRun, StorageQueues, UploadQueueRun};
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let pinning = uploaded_release(&manager, &temp_dir.path().join("pin"), "Album Title", false).await;
    let idle = StorageQueues {
        uploads: None,
        pins: None,
        outputs: None,
    };
    assert_eq!(listed_queues(&manager).await, idle);

    manager.set_sync_paused(true).await.unwrap();
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("upload"),
        "Album Title B",
        &[("track.flac", b"uploading")],
    )
    .await;
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![pinning.clone()]).await.unwrap();
    let uploads = Some(UploadQueueRun { paused: true });
    assert_eq!(
        listed_queues(&manager).await,
        StorageQueues {
            uploads,
            pins: Some(ReleaseQueueRun { paused: true, active: None }),
            outputs: None,
        }
    );

    assert!(manager
        .database
        .activate_queued_release::<()>(&pinning)
        .await
        .unwrap());
    manager
        .database
        .execute_for_test(
            "INSERT INTO output_queue (release_id, created_at, state, target_dir, kind) \
             VALUES (?1, 0, 'queued', '/exports', 'export')",
            vec![pinning.clone()],
        )
        .await
        .unwrap();
    let outputs = Some(ReleaseQueueRun { paused: false, active: None });
    assert_eq!(
        listed_queues(&manager).await,
        StorageQueues {
            uploads,
            pins: Some(ReleaseQueueRun { paused: true, active: Some(pinning.clone()) }),
            outputs: outputs.clone(),
        }
    );

    assert!(manager
        .database
        .fail_queued_release::<()>(&pinning, "the network connection was lost".into())
        .await
        .unwrap());
    assert_eq!(
        listed_queues(&manager).await,
        StorageQueues {
            uploads,
            pins: None,
            outputs,
        },
        "a failed pin is no work"
    );
}
