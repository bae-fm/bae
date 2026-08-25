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
        album.artist_id = artist_id.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
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
async fn storage_page_sort_by_format_nulls_last() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, format) in &[
        ("Album No Format", None),
        ("Album CD", Some("CD")),
        ("Album Vinyl", Some("Vinyl")),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.pressing.format = format.map(str::to_string);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::Format,
        direction: crate::db::SortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    // NULL format sorts last in both directions.
    assert_eq!(titles, vec!["Album CD", "Album Vinyl", "Album No Format"]);
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
        manager.database.insert_release(&release).await.unwrap();
        for i in 0..*file_count {
            let file = DbFile {
                id: bae_test_support::test_uuid(&format!("{}-file-{i}", release.id)),
                release_id: release.id.clone(),
                original_filename: format!("{i}.flac"),
                file_size: 1000,
                content_type: crate::util::content_type::ContentType::Flac,
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(b"fixture"),
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
        manager.database.insert_release(&release).await.unwrap();
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{}-file", release.id)),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            file_size: *file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
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
