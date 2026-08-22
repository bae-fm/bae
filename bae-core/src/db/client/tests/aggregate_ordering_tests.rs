use super::super::*;
use super::*;
use crate::playback::QueueEntryId;
use coven::SystemClock;
use std::sync::Arc;

async fn aggregate_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            PRAGMA reverse_unordered_selects = ON;

            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES
                ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'stamp', '2026-01-01T00:00:00Z'),
                ('c0770501-2551-4e87-8801-93f780248cf3', 'Composer Name First', 'stamp', '2026-01-01T00:00:00Z'),
                ('0d0e8916-becd-4d2c-89e0-7cc5c7005f83', 'Composer Name Second', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, NULL, 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
            VALUES
                ('2bd047a8-0ed8-4f71-851d-e168c16cbd36', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('1c2ab709-4221-4c38-8d0c-ff1d18107cce', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7e7d8df5-8292-4287-80be-7abd24f5a992', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('8aa66d48-65a0-42e4-8c1d-e7481e8c1861', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z'),
                ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z');

            INSERT INTO works (id, title, work_type, musicbrainz_work_id, _updated_at, created_at)
            VALUES ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', 'work', 'mb-work-a', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
            VALUES
                ('ba0d989a-7cb6-4050-8247-9a5424b33041', '432c8996-8af0-43dc-868a-822a256f65c4', '0d0e8916-becd-4d2c-89e0-7cc5c7005f83', 1, 'file_tags', 'stamp', '2026-01-01T00:00:00Z'),
                ('6027384b-545d-4289-8123-7201ec25276f', '432c8996-8af0-43dc-868a-822a256f65c4', 'c0770501-2551-4e87-8801-93f780248cf3', 0, 'file_tags', 'stamp', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

#[tokio::test]
async fn album_summary_orders_artist_names_and_release_ids_inside_aggregates() {
    let (db, _tmp) = aggregate_db().await;
    let summary = db
        .find_album_summary(ALBUM_A)
        .await
        .unwrap()
        .expect("album summary row");

    assert_eq!(
        summary.artist_names,
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(summary.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

#[tokio::test]
async fn work_summary_orders_composer_names_inside_the_aggregate() {
    let (db, _tmp) = aggregate_db().await;
    let results = db.search_library("Work Title A", 10).await.unwrap();
    let summary = results.works.first().expect("work summary row");

    assert_eq!(
        summary.composer_names.as_deref(),
        Some("Composer Name First, Composer Name Second")
    );
}

#[tokio::test]
async fn release_storage_summary_orders_artist_names_inside_the_aggregate() {
    let (db, _tmp) = aggregate_db().await;
    let summary = db
        .find_release_storage_summary(RELEASE_Z)
        .await
        .unwrap()
        .expect("release storage summary row");

    assert_eq!(
        summary.artist_names,
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
}

#[tokio::test]
async fn storage_page_orders_album_aggregate_columns_inside_aggregates() {
    let (db, _tmp) = aggregate_db().await;
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let rows = db
        .get_storage_page(&sort, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let row = rows.first().expect("storage row");

    assert_eq!(
        row.album.artist_names,
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(row.album.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

#[tokio::test]
async fn album_and_storage_pages_allow_missing_primary_artist() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES
                ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES
                ('f6506bc5-0b41-44de-862f-1668e72c08c6', 'Album Title Empty', NULL, 2026, '7c3d0881-e6d0-4252-8075-709b2282bcc1', 0, 'stamp', '2026-01-01T00:00:00Z'),
                ('049faa5b-52d9-4109-832b-f6853740c876', 'Album Title Extra', NULL, 2026, 'ba00ebe0-da50-428a-8ceb-2389d9a9f232', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
            VALUES
                ('c9966a2c-61b1-40dd-87ef-118587e57fe7', '049faa5b-52d9-4109-832b-f6853740c876', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('4953a209-c6c7-4b0c-82dd-cc13e47af890', '049faa5b-52d9-4109-832b-f6853740c876', '7e7d8df5-8292-4287-80be-7abd24f5a992', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('7c3d0881-e6d0-4252-8075-709b2282bcc1', 'f6506bc5-0b41-44de-862f-1668e72c08c6', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('ba00ebe0-da50-428a-8ceb-2389d9a9f232', '049faa5b-52d9-4109-832b-f6853740c876', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let album_sort = [AlbumSortCriterion {
        field: AlbumSortField::Title,
        direction: SortDirection::Ascending,
    }];
    let albums = db.get_album_page(&album_sort, 0, 10).await.unwrap();
    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].artist_names, "");
    assert_eq!(
        albums[1].artist_names,
        "Artist Name First, Artist Name Second"
    );

    let storage_sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let rows = db
        .get_storage_page(&storage_sort, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].album.artist_names, "");
    assert_eq!(
        rows[1].album.artist_names,
        "Artist Name First, Artist Name Second"
    );
}

async fn queue_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            PRAGMA reverse_unordered_selects = ON;

            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES
                ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                ('5b5f8c38-5237-4187-895c-28b1b2a43672', 'Track Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                ('8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 'Track Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
            VALUES ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
            VALUES
                ('af940e5f-472b-4162-81fb-97517afd23be', '0482872e-d4bf-4080-8426-441a0a3e71fc', '8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('8b08019f-1c04-400e-8107-9b85f7222407', '0482872e-d4bf-4080-8426-441a0a3e71fc', '5b5f8c38-5237-4187-895c-28b1b2a43672', 0, 'stamp', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

#[tokio::test]
async fn queue_items_order_track_artist_names_inside_the_aggregate() {
    let (db, _tmp) = queue_db().await;
    let items = db
        .get_queue_items(&[QueueEntry {
            id: QueueEntryId(ENTRY_A.to_string()),
            track_id: TRACK_A.to_string(),
        }])
        .await
        .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].artist_names,
        "Track Artist Name First, Track Artist Name Second"
    );
}

async fn release_detail_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            PRAGMA reverse_unordered_selects = ON;

            INSERT INTO artists (id, name, _updated_at, created_at)
            VALUES
                ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                ('5b5f8c38-5237-4187-895c-28b1b2a43672', 'Track Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                ('8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 'Track Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
            VALUES
                ('04676261-1659-47b1-879c-2947c52f4a8d', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title B', 1, 2, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
            VALUES
                ('af940e5f-472b-4162-81fb-97517afd23be', '0482872e-d4bf-4080-8426-441a0a3e71fc', '8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('8b08019f-1c04-400e-8107-9b85f7222407', '0482872e-d4bf-4080-8426-441a0a3e71fc', '5b5f8c38-5237-4187-895c-28b1b2a43672', 0, 'stamp', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

#[tokio::test]
async fn release_detail_orders_track_artists_and_keeps_tracks_without_artists() {
    let (db, _tmp) = release_detail_db().await;
    let detail = db
        .find_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");

    let track_ids: Vec<&str> = detail
        .tracks
        .iter()
        .map(|track| track.track.id.as_str())
        .collect();
    assert_eq!(track_ids, vec![TRACK_A, TRACK_B]);

    let artist_names: Vec<&str> = detail.tracks[0]
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect();
    assert_eq!(
        artist_names,
        vec!["Track Artist Name First", "Track Artist Name Second"]
    );
    assert!(detail.tracks[1].artists.is_empty());
}

#[tokio::test]
async fn release_files_come_back_in_case_insensitive_natural_order() {
    let (db, _tmp) = release_detail_db().await;
    // Inserted out of order, mixed case, with a two-digit number.
    for (i, name) in [
        "Track 10.flac",
        "cover.jpg",
        "Track 2.flac",
        "Back.jpg",
        "album.cue",
    ]
    .iter()
    .enumerate()
    {
        let mime = if name.ends_with(".flac") {
            "audio/flac"
        } else if name.ends_with(".cue") {
            "application/x-cue"
        } else {
            "image/jpeg"
        };
        db.insert_file(&DbFile {
            id: bae_test_support::test_uuid(&format!("ordering-file-{i}")),
            release_id: RELEASE_A.to_string(),
            original_filename: name.to_string(),
            file_size: 1,
            content_type: crate::util::content_type::ContentType::from_mime(mime),
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(name.as_bytes()),
            created_at: chrono::Utc::now(),
        })
        .await
        .unwrap();
    }

    let files = db.get_files_for_release(RELEASE_A).await.unwrap();
    let names: Vec<&str> = files.iter().map(|f| f.original_filename.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "album.cue",
            "Back.jpg",
            "cover.jpg",
            "Track 2.flac",
            "Track 10.flac",
        ]
    );
}
