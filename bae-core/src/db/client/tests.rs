#[cfg(test)]
mod queue_ordering_tests {
    use super::super::*;
    use crate::playback::QueueEntryId;
    use std::collections::HashMap;

    fn meta(id: &str) -> TrackQueueMeta {
        TrackQueueMeta {
            title: format!("Title {id}"),
            artist_names: "Artist Name".to_string(),
            duration_ms: Some(1000),
            album_title: "Album Title".to_string(),
            cover_image_id: Some(format!("rel-{id}")),
        }
    }

    fn entry(entry_id: &str, track_id: &str) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId(entry_id.to_string()),
            track_id: track_id.to_string(),
        }
    }

    #[test]
    fn preserves_duplicate_queue_entries_in_order_with_distinct_ids() {
        let mut meta_by_track = HashMap::new();
        meta_by_track.insert("a".to_string(), meta("a"));
        meta_by_track.insert("b".to_string(), meta("b"));

        // The same track queued twice resolves twice, in position order, each
        // carrying its own entry id.
        let resolved = resolve_queue_entries(
            &meta_by_track,
            &[entry("e0", "a"), entry("e1", "a"), entry("e2", "b")],
        );

        let track_ids: Vec<&str> = resolved.iter().map(|i| i.track_id.as_str()).collect();
        assert_eq!(track_ids, vec!["a", "a", "b"]);
        let entry_ids: Vec<&str> = resolved.iter().map(|i| i.entry_id.as_str()).collect();
        assert_eq!(entry_ids, vec!["e0", "e1", "e2"]);
    }

    #[test]
    fn skips_entries_whose_track_is_unknown() {
        let mut meta_by_track = HashMap::new();
        meta_by_track.insert("a".to_string(), meta("a"));
        let resolved =
            resolve_queue_entries(&meta_by_track, &[entry("e0", "a"), entry("e1", "missing")]);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entry_id, "e0");
    }
}

#[cfg(test)]
mod in_clause_chunking_tests {
    use super::super::*;
    use crate::playback::QueueEntryId;
    use coven::SystemClock;
    use std::sync::Arc;

    async fn chunked_track_db() -> (Database, tempfile::TempDir, Vec<String>) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let track_count = SQL_MAX_IN_VARS * 45;
        let track_ids: Vec<String> = (0..track_count)
            .map(|index| format!("track-{index}"))
            .collect();
        db.call(move |conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES ('artist-primary', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('album-a', 'Album Title A', 'artist-primary', 2026, 'release-a', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES ('release-a', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )?;
            conn.execute(
                "WITH RECURSIVE track_numbers(n) AS ( \
                     SELECT 0 \
                     UNION ALL \
                     SELECT n + 1 FROM track_numbers WHERE n < ?1 \
                 ) \
                 INSERT INTO tracks \
                     (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at) \
                 SELECT \
                     'track-' || n, \
                     'release-a', \
                     'Track Title ' || n, \
                     1, \
                     n, \
                     1000, \
                     NULL, \
                     'stamp', \
                     '2026-01-01T00:00:00Z' \
                 FROM track_numbers",
                [track_count as i64 - 1],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        (db, tmp, track_ids)
    }

    /// `cover_versions` takes a whole page's release ids, which is unbounded, so it
    /// chunks like its siblings. Past SQLite's variable limit an unchunked `IN`
    /// doesn't return fewer rows — it fails the query outright.
    #[tokio::test]
    async fn cover_versions_merges_chunks() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();

        let cover_count = SQL_MAX_IN_VARS * 3;
        db.call(move |conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES ('artist-primary', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('album-a', 'Album Title A', 'artist-primary', 2026, 'release-0', 0, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )?;
            // covers.id is a FK to releases.id, so every cover needs its release.
            conn.execute(
                "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < ?1) \
                 INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
                 SELECT 'release-' || i, 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z' \
                 FROM n",
                [cover_count as i64 - 1],
            )?;
            conn.execute(
                "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < ?1) \
                 INSERT INTO covers \
                     (id, content_type, file_size, source, _updated_at, created_at) \
                 SELECT 'release-' || i, 'image/jpeg', 1024, 'discogs', \
                        'stamp-' || i, '2026-01-01T00:00:00Z' \
                 FROM n",
                [cover_count as i64 - 1],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let release_ids: Vec<String> = (0..cover_count).map(|i| format!("release-{i}")).collect();
        let versions = db.cover_versions(&release_ids).await.unwrap();

        // Spans three chunks; every row from every chunk must survive the merge.
        assert_eq!(versions.len(), cover_count);
        assert_eq!(
            versions.get("release-0").map(String::as_str),
            Some("stamp-0")
        );
        let last = cover_count - 1;
        assert_eq!(
            versions.get(&format!("release-{last}")).map(String::as_str),
            Some(format!("stamp-{last}").as_str())
        );
    }

    #[tokio::test]
    async fn track_id_queries_merge_chunks() {
        let (db, _tmp, track_ids) = chunked_track_db().await;
        let mut requested = track_ids.clone();
        requested.insert(SQL_MAX_IN_VARS / 2, "missing-track".to_string());

        let mut existing = db.filter_existing_track_ids(&requested).await.unwrap();
        existing.sort();
        let mut expected_existing = track_ids.clone();
        expected_existing.sort();
        assert_eq!(existing, expected_existing);

        let album_ids = db.get_album_ids_for_tracks(&requested).await.unwrap();
        assert_eq!(album_ids.len(), track_ids.len());
        for track_id in &track_ids {
            assert_eq!(album_ids.get(track_id).map(String::as_str), Some("album-a"));
        }

        let entries: Vec<QueueEntry> = requested
            .iter()
            .enumerate()
            .map(|(index, track_id)| QueueEntry {
                id: QueueEntryId(format!("entry-{index}")),
                track_id: track_id.clone(),
            })
            .collect();
        let items = db.get_queue_items(&entries).await.unwrap();
        let resolved_track_ids: Vec<&str> =
            items.iter().map(|item| item.track_id.as_str()).collect();
        let expected_track_ids: Vec<&str> = requested
            .iter()
            .filter(|track_id| track_id.as_str() != "missing-track")
            .map(String::as_str)
            .collect();
        assert_eq!(resolved_track_ids, expected_track_ids);
    }
}

#[cfg(test)]
mod aggregate_ordering_tests {
    use super::super::*;
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
                    ('artist-primary', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-first', 'Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-second', 'Artist Name Second', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-composer-first', 'Composer Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-composer-second', 'Composer Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('album-a', 'Album Title A', 'artist-primary', 2026, NULL, 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('album-artist-second', 'album-a', 'artist-second', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-artist-first', 'album-a', 'artist-first', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-z', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-b', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z'),
                    ('release-a', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z');

                INSERT INTO works (id, title, work_type, _updated_at, created_at)
                VALUES ('work-a', 'Work Title A', 'work', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES
                    ('work-artist-second', 'work-a', 'artist-composer-second', 1, 'file_tags', 'stamp', '2026-01-01T00:00:00Z'),
                    ('work-artist-first', 'work-a', 'artist-composer-first', 0, 'file_tags', 'stamp', '2026-01-01T00:00:00Z');
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
            .find_album_summary("album-a")
            .await
            .unwrap()
            .expect("album summary row");

        assert_eq!(
            summary.artist_names,
            "Artist Name Primary, Artist Name First, Artist Name Second"
        );
        assert_eq!(
            summary.release_ids,
            vec!["release-z", "release-a", "release-b"]
        );
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
            .find_release_storage_summary("release-z")
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
        assert_eq!(
            row.album.release_ids,
            vec!["release-z", "release-a", "release-b"]
        );
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
                    ('artist-first', 'Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-second', 'Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('album-empty', 'Album Title Empty', NULL, 2026, 'release-empty', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-extra', 'Album Title Extra', NULL, 2026, 'release-extra', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('album-extra-artist-second', 'album-extra', 'artist-second', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-extra-artist-first', 'album-extra', 'artist-first', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-empty', 'album-empty', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-extra', 'album-extra', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');
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
                    ('artist-primary', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-track-first', 'Track Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-track-second', 'Track Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('album-a', 'Album Title A', 'artist-primary', 2026, 'release-a', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES ('release-a', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES ('track-a', 'release-a', 'Track Title A', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('track-artist-second', 'track-a', 'artist-track-second', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('track-artist-first', 'track-a', 'artist-track-first', 0, 'stamp', '2026-01-01T00:00:00Z');
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
                id: QueueEntryId("entry-a".to_string()),
                track_id: "track-a".to_string(),
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
                    ('artist-primary', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-track-first', 'Track Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-track-second', 'Track Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('album-a', 'Album Title A', 'artist-primary', 2026, 'release-a', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES ('release-a', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES
                    ('track-b', 'release-a', 'Track Title B', 1, 2, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('track-a', 'release-a', 'Track Title A', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('track-artist-second', 'track-a', 'artist-track-second', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('track-artist-first', 'track-a', 'artist-track-first', 0, 'stamp', '2026-01-01T00:00:00Z');
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
            .find_release_detail("release-a")
            .await
            .unwrap()
            .expect("release detail");

        let track_ids: Vec<&str> = detail
            .tracks
            .iter()
            .map(|track| track.track.id.as_str())
            .collect();
        assert_eq!(track_ids, vec!["track-a", "track-b"]);

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
}

#[cfg(test)]
mod connection_boundary_tests {
    use super::super::*;
    use coven::SystemClock;
    use std::sync::Arc;

    #[tokio::test]
    async fn coven_connection_enforces_foreign_keys_for_bae_schema() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();

        let track = DbTrack::new_test("missing-release", "track-a", "Track Title A", Some(1));
        let error = db
            .insert_track(&track)
            .await
            .expect_err("track insert without a release must violate the foreign key");

        assert!(
            error.0.contains("FOREIGN KEY constraint failed"),
            "expected a foreign-key violation, got {error}"
        );
    }
}

#[cfg(test)]
mod readable_cloud_path_tests {
    use super::super::*;

    /// An in-memory DB on the real schema with one artist/album/release, so the
    /// connection-level resolvers can look up a release's album id.
    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .unwrap();
        let now = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('artist-1', 'Artist Name', ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
             VALUES ('album-1', 'Album Title', 'artist-1', 0, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
             VALUES ('rel-1', 'album-1', 'file_tags', 1, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn audio_key_omits_source_folder_when_release_has_none() {
        // The seeded release has no source_folder_name (a non-folder import). The
        // stored key is namespace-relative; coven prepends the `release_files`
        // namespace when it reads/writes the blob.
        let conn = seeded_conn();
        let key = resolve_audio_cloud_path(&conn, "rel-1", "01 Track Title.flac").unwrap();
        assert_eq!(key, "album-1/rel-1/01 Track Title.flac");
    }

    #[test]
    fn audio_key_includes_source_folder_from_the_release_row() {
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET source_folder_name = 'Album Folder [FLAC]' WHERE id = 'rel-1'",
            [],
        )
        .unwrap();
        let key = resolve_audio_cloud_path(&conn, "rel-1", "01 Track Title.flac").unwrap();
        assert_eq!(key, "album-1/rel-1/Album Folder [FLAC]/01 Track Title.flac");
    }

    #[test]
    fn cover_key_is_album_release_cover() {
        let conn = seeded_conn();
        let key = resolve_cover_cloud_path(&conn, "rel-1", &ContentType::Jpeg).unwrap();
        assert_eq!(key, "album-1/rel-1/cover.jpg");
    }

    #[test]
    fn artist_key_is_artist_id() {
        // Keyed by the artist id alone -- no DB lookup.
        let key = resolve_artist_cloud_path("artist-1", &ContentType::Png);
        assert_eq!(key, "artist-1/artist.png");
    }

    #[test]
    fn missing_release_is_an_error() {
        // The release row must exist when a blob is keyed; its absence is a
        // broken invariant surfaced as an error, not masked.
        let conn = seeded_conn();
        assert!(resolve_audio_cloud_path(&conn, "no-such-release", "x.flac").is_err());
    }
}

#[cfg(test)]
mod row_mapper_error_tests {
    use super::super::*;

    /// An in-memory DB on the real schema with one artist/album/release whose
    /// `created_at`/`metadata_source` are valid, so a test can corrupt one
    /// column and prove the mapper rejects it.
    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .unwrap();
        let now = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('artist-1', 'Artist Name', ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
             VALUES ('album-1', 'Album Title', 'artist-1', 0, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
             VALUES ('rel-1', 'album-1', 'file_tags', 1, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn row_to_release_rejects_malformed_created_at() {
        // A corrupt timestamp must propagate as an error, not panic the mapper.
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET created_at = 'not-a-timestamp' WHERE id = 'rel-1'",
            [],
        )
        .unwrap();
        let result = conn.query_row(
            "SELECT * FROM releases WHERE id = 'rel-1'",
            [],
            row_to_release,
        );
        assert!(result.is_err());
    }

    #[test]
    fn row_to_release_rejects_unknown_metadata_source() {
        // An unknown enum string must propagate, not panic via expect.
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET metadata_source = 'bogus' WHERE id = 'rel-1'",
            [],
        )
        .unwrap();
        let result = conn.query_row(
            "SELECT * FROM releases WHERE id = 'rel-1'",
            [],
            row_to_release,
        );
        assert!(result.is_err());
    }

    #[test]
    fn row_to_import_rejects_unknown_status() {
        // An unrecognized status must surface as an error rather than silently
        // defaulting to `Importing`.
        let conn = seeded_conn();
        conn.execute(
            "INSERT INTO imports \
                 (id, status, album_title, artist_name, folder_path, created_at, updated_at) \
             VALUES ('imp-1', 'bogus', 'Album', 'Artist', '/tmp/x', 0, 0)",
            [],
        )
        .unwrap();
        let result = conn.query_row(
            "SELECT * FROM imports WHERE id = 'imp-1'",
            [],
            row_to_import,
        );
        assert!(result.is_err());
    }

    /// A `cloud_outbox` shaped like the real `pending_outbox` SELECT, so the
    /// positional reads in `row_to_outbox_entry` line up.
    fn outbox_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE cloud_outbox ( \
                 id INTEGER PRIMARY KEY, operation TEXT, file_id TEXT, cloud_key TEXT, \
                 source_path TEXT, scope TEXT, retain_pinned INTEGER, \
                 attempt_count INTEGER, last_attempt_at INTEGER )",
        )
        .unwrap();
        conn
    }

    const PENDING_OUTBOX_SELECT: &str = "SELECT id, operation, file_id, cloud_key, source_path, \
                                          scope, retain_pinned, attempt_count, last_attempt_at \
                                          FROM cloud_outbox WHERE id = 1";

    #[test]
    fn row_to_outbox_entry_rejects_unknown_operation() {
        // An unknown operation must propagate, not panic.
        let conn = outbox_conn();
        conn.execute(
            "INSERT INTO cloud_outbox (id, operation, attempt_count) VALUES (1, 'bogus', 0)",
            [],
        )
        .unwrap();
        let result = conn.query_row(PENDING_OUTBOX_SELECT, [], row_to_outbox_entry);
        assert!(result.is_err());
    }

    #[test]
    fn row_to_outbox_entry_rejects_unknown_scope() {
        // An upload row with an unknown scope must propagate, not panic.
        let conn = outbox_conn();
        conn.execute(
            "INSERT INTO cloud_outbox \
                 (id, operation, file_id, cloud_key, source_path, scope, retain_pinned, attempt_count) \
             VALUES (1, 'upload', 'file-1', 'cloud-key', '/tmp/x', 'bogus', 0, 0)",
            [],
        )
        .unwrap();
        let result = conn.query_row(PENDING_OUTBOX_SELECT, [], row_to_outbox_entry);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod cloud_outbox_tests {
    use super::super::*;
    use coven::SystemClock;

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        (db, tmp)
    }

    /// `remove_cloud_outbox_entry` deletes the row by id — the seam
    /// `cancel_outbox_item` uses to drop a queued upload the user cancelled.
    #[tokio::test]
    async fn remove_cloud_outbox_entry_deletes_by_id() {
        let (db, _tmp) = empty_db().await;
        db.add_cloud_outbox_upload("file-1", "upload-key", None, false)
            .await
            .unwrap();
        let uploads = db.get_pending_cloud_uploads().await.unwrap();
        assert_eq!(uploads.len(), 1);

        db.remove_cloud_outbox_entry(uploads[0].id).await.unwrap();

        assert!(db.get_pending_cloud_uploads().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn outbox_items_returns_uploads_and_deletes_not_cancels() {
        let (db, _tmp) = empty_db().await;
        db.add_cloud_outbox_upload("file-1", "upload-key", None, false)
            .await
            .unwrap();
        db.add_cloud_outbox_delete("delete-key").await.unwrap();
        db.add_cloud_outbox_cancel("cancel-key").await.unwrap();

        let rows = db.outbox_items().await.unwrap();
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[0].operation, DbOutboxOperation::Upload));
        assert_eq!(rows[0].cloud_key, "upload-key");
        assert!(matches!(rows[1].operation, DbOutboxOperation::Delete));
        assert_eq!(rows[1].cloud_key, "delete-key");
    }

    #[tokio::test]
    async fn outbox_items_rejects_unknown_operation() {
        let (db, _tmp) = empty_db().await;
        db.add_cloud_outbox_delete("cloud-key").await.unwrap();

        db.call(|conn| {
            conn.execute_batch("PRAGMA ignore_check_constraints = ON;")
                .map_err(DbError::from)?;
            conn.execute(
                "UPDATE cloud_outbox SET operation = 'bogus' WHERE cloud_key = 'cloud-key'",
                [],
            )
            .map_err(DbError::from)?;
            conn.execute_batch("PRAGMA ignore_check_constraints = OFF;")
                .map_err(DbError::from)
        })
        .await
        .unwrap();

        let result = db.outbox_items().await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod composer_mode_tests {
    use super::super::*;
    use coven::SystemClock;

    /// A replacement with no blob/outbox cleanup — the released-in-place local
    /// files these album-cleanup tests use carry no cloud state to tear down.
    fn empty_cleanup_plan() -> DeleteCleanupPlan {
        DeleteCleanupPlan {
            cloud_delete_keys: Vec::new(),
            in_flight_make_remote_blobs: Vec::new(),
            external_blob_ids_to_clear: Vec::new(),
            make_remote_release_ids_to_clear: Vec::new(),
        }
    }

    /// Shared arrange for the two reimport-replacement tests. Seeds `album-old` with
    /// `existing_release_ids` and its `primary_release_id` at `replaced_release_id`,
    /// then finalizes a reimport whose new release `rel-new` lands in the fresh
    /// `album-new`, carrying an `ImportReplacementDelete` for `replaced_release_id`.
    /// Returns the finalize outcomes, so each test asserts the album's fate itself.
    async fn finalize_reimport_replacing_release(
        db: &Database,
        tmp: &tempfile::TempDir,
        now: chrono::DateTime<chrono::Utc>,
        existing_release_ids: &[&str],
        replaced_release_id: &str,
    ) -> Vec<ImportReplacementOutcome> {
        let artist = DbArtist {
            id: "artist-a".to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album_old = DbAlbum {
            id: "album-old".to_string(),
            title: "Album Title Old".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        db.insert_album(&album_old).await.unwrap();
        for id in existing_release_ids {
            db.insert_release(&DbRelease::new_test(&album_old.id, id))
                .await
                .unwrap();
        }
        db.set_album_primary_release(&album_old.id, replaced_release_id)
            .await
            .unwrap();

        // The reimport lands its new release in a fresh album.
        db.insert_import(&DbImport::new(
            "import-new",
            "Album Title New",
            "Artist Name A",
            tmp.path().to_str().unwrap(),
            now,
        ))
        .await
        .unwrap();
        let album_new = DbAlbum {
            id: "album-new".to_string(),
            title: "Album Title New".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release_new = DbRelease::new_test(&album_new.id, "rel-new");
        let track = DbTrack {
            id: "track-new".to_string(),
            release_id: release_new.id.clone(),
            title: "Track Title New".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release_new.id,
            "Track Title New.flac",
            1024,
            ContentType::Flac,
            "file-new".to_string(),
            now,
            None,
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title New.flac"),
        }];

        let replacement = ImportReplacementDelete {
            release_id: replaced_release_id.to_string(),
            album_id: album_old.id.clone(),
            cleanup: empty_cleanup_plan(),
        };
        db.finalize_import_atomic(
            Some(&album_new),
            &release_new,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            None,
            "import-new",
            ImportOperationStatus::Complete,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[replacement],
        )
        .await
        .unwrap()
    }

    async fn seeded_db() -> (Database, tempfile::TempDir) {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('artist-album', 'Album Artist A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('artist-composer', 'Displayed Composer A', 'Hidden Composer Sort A', NULL, 'mb-artist-composer-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('album-a', 'Album Title A', 'artist-album', 2026, 'release-a', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, release_name, year, disc_id, metadata_source, metadata_source_release_id, format, label, catalog_number, country, barcode, remote, source_folder_name, content_hash, album_loudness_lufs, album_peak_linear, _updated_at, created_at)
                VALUES ('release-a', 'album-a', NULL, 2026, NULL, 'musicbrainz', 'mb-release-a', 'CD', NULL, NULL, NULL, NULL, 1, NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES ('track-a', 'release-a', 'Track Title A', 1, 1, 1000, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, disambiguation, work_type, _updated_at, created_at)
                VALUES
                    ('work-parent-a', 'Parent Work A', NULL, 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-child-a', 'Displayed Work A', NULL, 'part', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES ('work-artist-a', 'work-child-a', 'artist-composer', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_parts (id, parent_work_id, child_work_id, position, source, _updated_at, created_at)
                VALUES ('work-part-a', 'work-parent-a', 'work-child-a', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO track_works (id, track_id, work_id, position, source, _updated_at, created_at)
                VALUES ('track-work-a', 'track-a', 'work-child-a', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
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
    async fn finalize_import_persists_composer_work_and_role_rows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let album_artist = DbArtist {
            id: "artist-album".to_string(),
            name: "Album Artist A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        let composer = DbArtist {
            id: "artist-composer".to_string(),
            name: "Composer Artist A".to_string(),
            sort_name: Some("Composer Artist A".to_string()),
            discogs_artist_id: None,
            musicbrainz_artist_id: Some("mb-artist-composer-a".to_string()),
            created_at: now,
        };
        db.insert_artist(&album_artist).await.unwrap();
        db.insert_artist(&composer).await.unwrap();

        let album = DbAlbum {
            id: "album-a".to_string(),
            title: "Album Title A".to_string(),
            artist_id: album_artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease {
            id: "release-a".to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: Pressing {
                year: Some(2026),
                format: Some("CD".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::MusicBrainz,
            metadata_source_release_id: Some("mb-release-a".to_string()),
            remote: true,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = DbTrack {
            id: "track-a".to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track.flac"),
        }];
        let works = vec![DbWork::new(
            "work-a",
            "Work Title A",
            None,
            Some("work".to_string()),
            now,
        )];
        let work_artists = vec![DbWorkArtist::new(
            "work-a",
            &composer.id,
            0,
            crate::import::MetadataSource::MusicBrainz,
            "work-artist-a".to_string(),
            now,
        )];
        let track_works = vec![DbTrackWork::new(
            "track-a",
            "work-a",
            0,
            crate::import::MetadataSource::MusicBrainz,
            "track-work-a".to_string(),
            now,
        )];
        let release_roles = vec![DbReleaseArtistRole::new(
            &release.id,
            &composer.id,
            0,
            crate::import::MetadataSource::Discogs,
            Some("Conducted By".to_string()),
            "release-role-a".to_string(),
            now,
        )];
        let track_roles = vec![DbTrackArtistRole::new(
            "track-a",
            &composer.id,
            0,
            crate::import::MetadataSource::MusicBrainz,
            Some("arranger".to_string()),
            "track-role-a".to_string(),
            now,
        )];

        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &[],
            &works,
            &work_artists,
            &[],
            &track_works,
            &release_roles,
            &track_roles,
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            "import-a",
            ImportOperationStatus::Complete,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        let composer_detail = db
            .find_composer_detail(&composer.id)
            .await
            .unwrap()
            .expect("composer detail");
        assert_eq!(composer_detail.work_groups.len(), 1);
        assert_eq!(composer_detail.work_groups[0].works[0].work.id, "work-a");
        let release_role_count = db
            .call(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM release_artist_roles WHERE id = 'release-role-a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(DbError::from)
            })
            .await
            .unwrap();
        assert_eq!(release_role_count, 1);

        let work_detail = db
            .find_work_detail("work-a")
            .await
            .unwrap()
            .expect("work detail");
        assert_eq!(work_detail.tracks.len(), 1);
        let track_role_count = db
            .call(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM track_artist_roles WHERE id = 'track-role-a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(DbError::from)
            })
            .await
            .unwrap();
        assert_eq!(track_role_count, 1);
    }

    #[tokio::test]
    async fn fail_import_and_delete_release_removes_finalized_import_state_atomically() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        db.insert_import(&DbImport::new(
            "import-a",
            "Album Title A",
            "Artist Name A",
            tmp.path().to_str().unwrap(),
            now,
        ))
        .await
        .unwrap();

        let artist = DbArtist {
            id: "artist-a".to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: "album-a".to_string(),
            title: "Album Title A".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease {
            id: "release-a".to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: Pressing {
                year: Some(2026),
                format: Some("FLAC".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = DbTrack {
            id: "track-a".to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release.id,
            "Track Title A.flac",
            1024,
            ContentType::Flac,
            "file-a".to_string(),
            now,
            None,
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title A.flac"),
        }];

        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            "import-a",
            ImportOperationStatus::Complete,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();
        assert!(db.external_blob("file-a").await.unwrap().is_some());

        db.fail_import_and_delete_release("import-a", "release-a", "remote upload failed")
            .await
            .unwrap();

        assert!(db.find_release_by_id("release-a").await.unwrap().is_none());
        assert!(db.find_album_by_id("album-a").await.unwrap().is_none());
        assert!(db.external_blob("file-a").await.unwrap().is_none());
        let import = db
            .find_import_by_id("import-a")
            .await
            .unwrap()
            .expect("import remains visible as failed");
        assert_eq!(import.status, ImportOperationStatus::Failed);
        assert!(import.release_id.is_none());
        assert_eq!(
            import.error_message.as_deref(),
            Some("remote upload failed")
        );
    }

    /// Reimport replacing one of several releases in an album: the prior release
    /// leaves, the album survives, and a `primary_release_id` pointing at the
    /// departed release goes NULL — read paths fall back to the first release left.
    #[tokio::test]
    async fn finalize_replacement_in_surviving_album_clears_dangling_primary() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        // album-old holds rel-one (primary) and rel-two; the reimport replaces
        // rel-one, so the album survives on rel-two.
        let outcomes =
            finalize_reimport_replacing_release(&db, &tmp, now, &["rel-one", "rel-two"], "rel-one")
                .await;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].album_id, "album-old");
        assert_eq!(outcomes[0].release_id, "rel-one");
        assert!(!outcomes[0].album_deleted);

        let surviving = db
            .find_album_by_id("album-old")
            .await
            .unwrap()
            .expect("album survives while rel-two remains");
        assert_eq!(surviving.primary_release_id, None);
        assert!(db.find_release_by_id("rel-one").await.unwrap().is_none());
        assert!(db.find_release_by_id("rel-two").await.unwrap().is_some());
    }

    /// Reimport replacing an album's sole release, landing in a new album: the prior
    /// album empties and is deleted, and the outcome reports that.
    #[tokio::test]
    async fn finalize_replacement_of_last_release_deletes_prior_album() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        // album-old holds only rel-old; replacing it empties and deletes it.
        let outcomes =
            finalize_reimport_replacing_release(&db, &tmp, now, &["rel-old"], "rel-old").await;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].album_id, "album-old");
        assert!(outcomes[0].album_deleted);

        assert!(db.find_album_by_id("album-old").await.unwrap().is_none());
        assert!(db.find_release_by_id("rel-old").await.unwrap().is_none());
        assert!(db.find_album_by_id("album-new").await.unwrap().is_some());
        assert!(db.find_release_by_id("rel-new").await.unwrap().is_some());
    }

    /// Failed-import rollback of one of several releases in an album: the album
    /// survives, a `primary_release_id` pointing at the failed release goes NULL,
    /// and the sibling release is untouched.
    #[tokio::test]
    async fn fail_import_and_delete_release_in_surviving_album_clears_dangling_primary() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        db.insert_import(&DbImport::new(
            "import-a",
            "Album Title A",
            "Artist Name A",
            tmp.path().to_str().unwrap(),
            now,
        ))
        .await
        .unwrap();

        let artist = DbArtist {
            id: "artist-a".to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: "album-a".to_string(),
            title: "Album Title A".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease::new_test(&album.id, "rel-a");
        let track = DbTrack {
            id: "track-a".to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release.id,
            "Track Title A.flac",
            1024,
            ContentType::Flac,
            "file-a".to_string(),
            now,
            None,
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title A.flac"),
        }];

        // Finalize the import, pointing the album's primary at the release
        // this import created.
        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            "import-a",
            ImportOperationStatus::Complete,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        // A sibling release in the same album keeps it alive through the
        // rollback.
        let sibling = DbRelease::new_test(&album.id, "rel-b");
        db.insert_release(&sibling).await.unwrap();

        db.fail_import_and_delete_release("import-a", "rel-a", "remote upload failed")
            .await
            .unwrap();

        let surviving = db
            .find_album_by_id("album-a")
            .await
            .unwrap()
            .expect("album survives while sibling remains");
        assert_eq!(surviving.primary_release_id, None);
        assert!(db.find_release_by_id("rel-a").await.unwrap().is_none());
        assert!(db.find_release_by_id("rel-b").await.unwrap().is_some());
        let import = db
            .find_import_by_id("import-a")
            .await
            .unwrap()
            .expect("import remains visible as failed");
        assert_eq!(import.status, ImportOperationStatus::Failed);
        assert!(import.release_id.is_none());
    }

    /// A failed remote import's cover and artist-image blobs live only in coven's
    /// on-device store, since the release never went remote. The DB transaction drops
    /// their rows but can't reach the blob store, so `fail_import_and_delete_release`
    /// returns the blobs it orphaned for the caller to evict: the cover and each
    /// deleted artist's image, but not the image of an artist a surviving release
    /// still references.
    #[tokio::test]
    async fn fail_import_and_delete_release_returns_orphaned_image_blobs_to_evict() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let artist = |id: &str| DbArtist {
            id: id.to_string(),
            name: id.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist("artist-exclusive")).await.unwrap();
        db.insert_artist(&artist("artist-shared")).await.unwrap();

        let pressing = || Pressing {
            year: Some(2026),
            format: Some("FLAC".to_string()),
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        };
        let album = |id: &str, artist_id: &str| DbAlbum {
            id: id.to_string(),
            title: id.to_string(),
            artist_id: artist_id.to_string(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = |id: &str, album_id: &str| DbRelease {
            id: id.to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            pressing: pressing(),
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = |id: &str, release_id: &str| DbTrack {
            id: id.to_string(),
            release_id: release_id.to_string(),
            title: id.to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };

        // A prior surviving album references artist-shared, so the failed
        // import below must keep artist-shared and its image.
        db.insert_album_with_release_and_tracks(
            &album("album-prior", "artist-shared"),
            &release("release-prior", "album-prior"),
            &[track("track-prior", "release-prior")],
            &[],
            &[],
        )
        .await
        .unwrap();

        db.insert_import(&DbImport::new(
            "import-a",
            "Album A",
            "artist-exclusive",
            tmp.path().to_str().unwrap(),
            now,
        ))
        .await
        .unwrap();

        let album_a = album("album-a", "artist-exclusive");
        let release_a = release("release-a", "album-a");
        let file_a = DbFile::new(
            "release-a",
            "Track A.flac",
            1024,
            ContentType::Flac,
            "file-a".to_string(),
            now,
            None,
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track("track-a", "release-a"),
            file_path: tmp.path().join("Track A.flac"),
        }];
        // The failed release also credits artist-shared, so both artists are
        // rollback candidates; only artist-exclusive should be deleted.
        let album_artists = vec![DbAlbumArtist {
            id: "aa-shared".to_string(),
            album_id: "album-a".to_string(),
            artist_id: "artist-shared".to_string(),
            position: 1,
            created_at: now,
        }];
        let image = |id: &str, image_type: LibraryImageType| DbLibraryImage {
            id: id.to_string(),
            image_type,
            content_type: ContentType::Jpeg,
            file_size: 3,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            cloud_path: None,
            content_hash: Some(crate::util::fs::hash_bytes(&[1u8, 2, 3])),
            created_at: now,
        };
        let cover = image("release-a", LibraryImageType::Cover);
        let img_exclusive = image("artist-exclusive", LibraryImageType::Artist);
        let img_shared = image("artist-shared", LibraryImageType::Artist);
        let bytes = [1u8, 2, 3];

        db.finalize_import_atomic(
            Some(&album_a),
            &release_a,
            &track_files,
            &[],
            &[],
            &album_artists,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file_a],
            &[],
            &[],
            Some((&cover, &bytes)),
            &[(&img_exclusive, &bytes), (&img_shared, &bytes)],
            Some((&album_a.id, &release_a.id)),
            "import-a",
            ImportOperationStatus::Complete,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        let orphaned = db
            .fail_import_and_delete_release("import-a", "release-a", "remote upload failed")
            .await
            .unwrap();

        assert!(
            orphaned.contains(&OrphanedImageBlob {
                namespace: crate::sync::COVERS_NAMESPACE,
                id: "release-a".to_string(),
                cloud_path: None,
            }),
            "the cover blob is returned for eviction: {orphaned:?}"
        );
        assert!(
            orphaned.contains(&OrphanedImageBlob {
                namespace: crate::sync::ARTIST_IMAGES_NAMESPACE,
                id: "artist-exclusive".to_string(),
                cloud_path: None,
            }),
            "the deleted artist's image blob is returned for eviction: {orphaned:?}"
        );
        assert!(
            !orphaned.iter().any(|b| b.id == "artist-shared"),
            "the shared artist survives, so its image is not evicted: {orphaned:?}"
        );
        assert_eq!(
            orphaned.len(),
            2,
            "only the cover and the deleted artist's image: {orphaned:?}"
        );

        // The shared artist and its image row survive; the exclusive one is gone.
        assert!(db
            .find_artist_by_id("artist-shared")
            .await
            .unwrap()
            .is_some());
        assert!(db
            .find_artist_by_id("artist-exclusive")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn complete_import_for_release_marks_active_release_import_complete() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        db.insert_import(&DbImport::new(
            "import-a",
            "Album Title A",
            "Artist Name A",
            tmp.path().to_str().unwrap(),
            now,
        ))
        .await
        .unwrap();

        let artist = DbArtist {
            id: "artist-a".to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: "album-a".to_string(),
            title: "Album Title A".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease {
            id: "release-a".to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: Pressing {
                year: Some(2026),
                format: Some("FLAC".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = DbTrack {
            id: "track-a".to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release.id,
            "Track Title A.flac",
            1024,
            ContentType::Flac,
            "file-a".to_string(),
            now,
            None,
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title A.flac"),
        }];

        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            "import-a",
            ImportOperationStatus::Importing,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        db.complete_import_for_release("release-a").await.unwrap();

        let import = db
            .find_import_by_id("import-a")
            .await
            .unwrap()
            .expect("import remains linked to release");
        assert_eq!(import.status, ImportOperationStatus::Complete);
        assert_eq!(import.release_id.as_deref(), Some("release-a"));
    }

    #[tokio::test]
    async fn search_library_matches_composer_and_work_sort_names() {
        let (db, _tmp) = seeded_db().await;

        let composer_results = db.search_library("Hidden Composer", 10).await.unwrap();
        assert_eq!(composer_results.composers.len(), 1);
        assert_eq!(composer_results.composers[0].artist.id, "artist-composer");

        let work_results = db.search_library("Displayed Work", 10).await.unwrap();
        assert_eq!(work_results.works.len(), 1);
        assert_eq!(work_results.works[0].work.id, "work-child-a");
        assert_eq!(
            work_results.works[0].parent_work_id.as_deref(),
            Some("work-parent-a")
        );
        assert_eq!(
            work_results.works[0].representative_release_id.as_deref(),
            Some("release-a")
        );
    }

    #[tokio::test]
    async fn search_library_treats_like_metacharacters_as_literals() {
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
                VALUES ('artist-primary', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('album-percent', '50% Album Title', 'artist-primary', 2026, 'release-percent', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-percent-wildcard', '500 Album Title', 'artist-primary', 2026, 'release-percent-wildcard', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-underscore', 'A_B Album Title', 'artist-primary', 2026, 'release-underscore', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-underscore-wildcard', 'ACB Album Title', 'artist-primary', 2026, 'release-underscore-wildcard', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-percent', 'album-percent', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-percent-wildcard', 'album-percent-wildcard', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-underscore', 'album-underscore', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-underscore-wildcard', 'album-underscore-wildcard', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES
                    ('track-percent', 'release-percent', '50% Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('track-percent-wildcard', 'release-percent-wildcard', '500 Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('track-underscore', 'release-underscore', 'A_B Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('track-underscore-wildcard', 'release-underscore-wildcard', 'ACB Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let percent_results = db.search_library("50%", 10).await.unwrap();
        assert_eq!(percent_results.albums.len(), 1);
        assert_eq!(percent_results.albums[0].id, "album-percent");
        assert_eq!(percent_results.tracks.len(), 1);
        assert_eq!(percent_results.tracks[0].id, "track-percent");

        let underscore_results = db.search_library("A_B", 10).await.unwrap();
        assert_eq!(underscore_results.albums.len(), 1);
        assert_eq!(underscore_results.albums[0].id, "album-underscore");
        assert_eq!(underscore_results.tracks.len(), 1);
        assert_eq!(underscore_results.tracks[0].id, "track-underscore");
    }

    #[tokio::test]
    async fn composer_detail_carries_work_parent_and_representative_release() {
        let (db, _tmp) = seeded_db().await;

        let detail = db
            .find_composer_detail("artist-composer")
            .await
            .unwrap()
            .expect("composer detail");

        assert_eq!(detail.work_groups.len(), 1);
        let group = &detail.work_groups[0];
        assert_eq!(
            group.parent.as_ref().map(|work| work.work.id.as_str()),
            Some("work-parent-a")
        );
        assert_eq!(group.works.len(), 1);
        assert_eq!(group.works[0].work.id, "work-child-a");
        assert_eq!(
            group.works[0].parent_work_id.as_deref(),
            Some("work-parent-a")
        );
        assert_eq!(
            group.works[0].representative_release_id.as_deref(),
            Some("release-a")
        );
    }

    #[tokio::test]
    async fn work_detail_lists_child_works_with_their_representative_release() {
        let (db, _tmp) = seeded_db().await;

        let detail = db
            .find_work_detail("work-parent-a")
            .await
            .unwrap()
            .expect("work detail");

        assert_eq!(detail.child_works.len(), 1);
        assert_eq!(detail.child_works[0].work.id, "work-child-a");
        assert_eq!(
            detail.child_works[0].representative_release_id.as_deref(),
            Some("release-a")
        );
    }

    #[tokio::test]
    async fn work_detail_release_rows_carry_album_release_display_fields() {
        let (db, _tmp) = seeded_db().await;

        let detail = db
            .find_work_detail("work-child-a")
            .await
            .unwrap()
            .expect("work detail");

        assert_eq!(detail.releases.len(), 1);
        let release = &detail.releases[0];
        assert_eq!(release.release_id, "release-a");
        assert_eq!(release.album_id, "album-a");
        assert_eq!(release.album_title, "Album Title A");
        assert_eq!(release.release_name, None);
        assert_eq!(release.year, Some(2026));
        assert_eq!(release.format.as_deref(), Some("CD"));
        assert_eq!(release.release_index, 1);
    }

    #[tokio::test]
    async fn composer_page_uses_id_tiebreaker() {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('composer-c', 'Composer Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('composer-a', 'Composer Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('composer-b', 'Composer Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, work_type, _updated_at, created_at)
                VALUES
                    ('work-a', 'Work Title A', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-b', 'Work Title B', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-c', 'Work Title C', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES
                    ('work-artist-c', 'work-c', 'composer-c', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-artist-a', 'work-a', 'composer-a', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-artist-b', 'work-b', 'composer-b', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [ComposerSortCriterion {
            field: ComposerSortField::WorkCount,
            direction: SortDirection::Descending,
        }];
        let mut page_ids = Vec::new();
        for offset in 0..3 {
            let page = db.get_composer_page(&sort, offset, 1).await.unwrap();
            assert_eq!(page.len(), 1);
            page_ids.push(page[0].artist.id.clone());
        }

        assert_eq!(page_ids, vec!["composer-a", "composer-b", "composer-c"]);
    }

    /// A secondary criterion applies before the name-ASC tail. Two composers tie on
    /// `WorkCount` but differ in name, so `[WorkCount DESC, Name DESC]` must order
    /// the tied pair by name descending — a single-criterion implementation would
    /// fall through to the tail's `composer.name ASC` and order them the other way.
    #[tokio::test]
    async fn composer_page_applies_secondary_criterion() {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('composer-a', 'Composer Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('composer-b', 'Composer Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('composer-solo', 'Composer Name Solo', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, work_type, _updated_at, created_at)
                VALUES
                    ('work-a1', 'Work Title A1', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-a2', 'Work Title A2', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-b1', 'Work Title B1', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-b2', 'Work Title B2', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-solo', 'Work Title Solo', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES
                    ('work-artist-a1', 'work-a1', 'composer-a', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-artist-a2', 'work-a2', 'composer-a', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-artist-b1', 'work-b1', 'composer-b', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-artist-b2', 'work-b2', 'composer-b', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('work-artist-solo', 'work-solo', 'composer-solo', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [
            ComposerSortCriterion {
                field: ComposerSortField::WorkCount,
                direction: SortDirection::Descending,
            },
            ComposerSortCriterion {
                field: ComposerSortField::Name,
                direction: SortDirection::Descending,
            },
        ];
        let page = db.get_composer_page(&sort, 0, 10).await.unwrap();
        let ids: Vec<&str> = page.iter().map(|c| c.artist.id.as_str()).collect();

        // composer-a and composer-b tie on work_count (2 each); the secondary
        // Name DESC criterion orders composer-b before composer-a.
        assert_eq!(ids, vec!["composer-b", "composer-a", "composer-solo"]);
    }
}

#[cfg(test)]
mod artist_mode_tests {
    use super::super::*;
    use coven::SystemClock;

    /// Artists covering every membership case: a primary-FK artist that is
    /// also a junction artist elsewhere, a junction-only artist, the Various
    /// Artists row as a compilation's primary, a work-only composer (no album
    /// links), and a fully unlinked artist.
    async fn seeded_db() -> (Database, tempfile::TempDir) {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('artist-primary', 'Artist Name B', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-extra', 'Artist Name A', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-various', 'Various Artists', NULL, '194', NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-work-only', 'Composer Name A', NULL, NULL, 'mb-artist-work-only', 'stamp', '2026-01-01T00:00:00Z'),
                    ('artist-unlinked', 'Artist Name C', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('album-a', 'Album Title A', 'artist-primary', 2001, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-b', 'Album Title B', 'artist-primary', 1999, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('album-compilation', 'Compilation Title A', 'artist-various', 2005, NULL, 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-a', 'album-a', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-b', 'album-b', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('release-compilation', 'album-compilation', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                -- artist-extra joins album-b; artist-primary's junction row on
                -- album-a duplicates its primary FK and must not double-count.
                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('aa-extra-b', 'album-b', 'artist-extra', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('aa-primary-a', 'album-a', 'artist-primary', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, disambiguation, work_type, _updated_at, created_at)
                VALUES ('work-a', 'Work Title A', NULL, 'work', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES ('work-artist-a', 'work-a', 'artist-work-only', 0, 'musicbrainz', 'stamp', '2026-01-01T00:00:00Z');
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
    async fn artist_page_lists_album_artists_with_distinct_album_counts() {
        let (db, _tmp) = seeded_db().await;

        let sort = [ArtistSortCriterion {
            field: ArtistSortField::Name,
            direction: SortDirection::Ascending,
        }];
        let page = db.get_artist_page(&sort, 0, 10).await.unwrap();

        let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["artist-extra", "artist-primary", "artist-various"]
        );

        let counts: Vec<i64> = page.iter().map(|a| a.album_count).collect();
        assert_eq!(counts, vec![1, 2, 1]);

        assert_eq!(db.get_artist_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn artist_page_sorts_by_album_count() {
        let (db, _tmp) = seeded_db().await;

        let sort = [ArtistSortCriterion {
            field: ArtistSortField::AlbumCount,
            direction: SortDirection::Descending,
        }];
        let page = db.get_artist_page(&sort, 0, 10).await.unwrap();

        let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["artist-primary", "artist-extra", "artist-various"]
        );
    }

    #[tokio::test]
    async fn artist_page_uses_id_tiebreaker() {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('artist-c', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('artist-a', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('artist-b', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('album-c', 'Album Title C', 'artist-c', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-a', 'Album Title A', 'artist-a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-b', 'Album Title B', 'artist-b', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-c', 'album-c', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-a', 'album-a', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-b', 'album-b', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [ArtistSortCriterion {
            field: ArtistSortField::AlbumCount,
            direction: SortDirection::Descending,
        }];
        let mut page_ids = Vec::new();
        for offset in 0..3 {
            let page = db.get_artist_page(&sort, offset, 1).await.unwrap();
            assert_eq!(page.len(), 1);
            page_ids.push(page[0].artist.id.clone());
        }

        assert_eq!(page_ids, vec!["artist-a", "artist-b", "artist-c"]);
    }

    /// A secondary criterion applies before the name-ASC tail. Two artists tie on
    /// `AlbumCount` but differ in name, so `[AlbumCount DESC, Name DESC]` must order
    /// the tied pair by name descending — a single-criterion implementation would
    /// fall through to the tail's `ar.name ASC` and order them the other way.
    #[tokio::test]
    async fn artist_page_applies_secondary_criterion() {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('artist-a', 'Artist Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('artist-b', 'Artist Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('artist-solo', 'Artist Name Solo', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('album-a1', 'Album Title A1', 'artist-a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-a2', 'Album Title A2', 'artist-a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-b1', 'Album Title B1', 'artist-b', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-b2', 'Album Title B2', 'artist-b', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-solo', 'Album Title Solo', 'artist-solo', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-a1', 'album-a1', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-a2', 'album-a2', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-b1', 'album-b1', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-b2', 'album-b2', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-solo', 'album-solo', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [
            ArtistSortCriterion {
                field: ArtistSortField::AlbumCount,
                direction: SortDirection::Descending,
            },
            ArtistSortCriterion {
                field: ArtistSortField::Name,
                direction: SortDirection::Descending,
            },
        ];
        let page = db.get_artist_page(&sort, 0, 10).await.unwrap();
        let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();

        // artist-a and artist-b tie on album_count (2 each); the secondary
        // Name DESC criterion orders artist-b before artist-a.
        assert_eq!(ids, vec!["artist-b", "artist-a", "artist-solo"]);
    }

    #[tokio::test]
    async fn artist_detail_orders_albums_year_then_title_with_unknown_years_last() {
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
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('artist-a', 'Artist Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('artist-other', 'Artist Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('album-null', 'Album Title Null', 'artist-a', NULL, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-2001-upper', 'Album Title B', 'artist-a', 2001, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-2001-lower', 'album title a', 'artist-a', 2001, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-1999', 'Album Title 1999', 'artist-a', 1999, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-junction', 'Album Title Junction', 'artist-other', 2005, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('album-unrelated', 'Album Title Unrelated', 'artist-other', 1990, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('release-null', 'album-null', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-2001-upper', 'album-2001-upper', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-2001-lower', 'album-2001-lower', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-1999', 'album-1999', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-junction', 'album-junction', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('release-unrelated', 'album-unrelated', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES ('aa-junction', 'album-junction', 'artist-a', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let detail = db.find_artist_detail("artist-a").await.unwrap();
        let detail = detail.expect("artist-a has album links and must resolve");

        assert_eq!(detail.artist.artist.id, "artist-a");
        assert_eq!(detail.artist.album_count, 5);

        let album_ids: Vec<&str> = detail.albums.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            album_ids,
            vec![
                "album-1999",
                "album-2001-lower",
                "album-2001-upper",
                "album-junction",
                "album-null",
            ]
        );
    }

    #[tokio::test]
    async fn artist_detail_absent_or_album_less_artist_is_none() {
        let (db, _tmp) = seeded_db().await;

        assert!(db
            .find_artist_detail("artist-absent")
            .await
            .unwrap()
            .is_none());
        assert!(db
            .find_artist_detail("artist-work-only")
            .await
            .unwrap()
            .is_none());
    }
}

#[cfg(test)]
mod playback_state_load_tests {
    use super::super::*;
    use coven::SystemClock;

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        (db, tmp)
    }

    /// `source` and `cursor` are written together, so a row carrying one without
    /// the other is corrupt: `load_playback_state` reports `Corrupt` rather than
    /// inventing a cursor or masking it as an absent cache.
    #[tokio::test]
    async fn mismatched_source_and_cursor_discards_the_cache() {
        let (db, _tmp) = empty_db().await;

        // Write a row by hand with a present source but a NULL cursor --
        // `save_playback_state` never produces this, so we insert it directly.
        db.call(|conn| {
            conn.execute(
                "INSERT INTO playback_state \
                     (id, source, shuffle_seed, cursor, manual, repeat, \
                      current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', 'rel-1', NULL, NULL, '[]', 'off', \
                      NULL, NULL, 1.0, 0)",
                [],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        assert!(matches!(
            db.load_playback_state().await.unwrap(),
            LoadedPlaybackState::Corrupt
        ));
    }
}

/// The ids this layer mints itself — a release's `release_identities` rows, and
/// the `album_artists` rows copied when a `set_identity` moves a release to a new
/// album — come from the injected [`coven::IdProvider`], like every other id in
/// the process. Minting them with a raw `Uuid::new_v4()` would put an id source
/// nobody injected inside the DB, and a test running a deterministic provider
/// would still get random ones.
#[cfg(test)]
mod injected_ids_tests {
    use super::super::*;
    use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, ReleaseMetadataSource};
    use crate::import::{MetadataSource, ReleaseIdentity};
    use chrono::Utc;
    use coven::SystemClock;
    use std::sync::Arc;

    async fn db_with_sequential_ids() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            Arc::new(coven::SequentialIdProvider::new("db-ids")),
        )
        .await
        .unwrap();
        (db, tmp)
    }

    fn identity(source: MetadataSource, release_id: &str) -> ReleaseIdentity {
        ReleaseIdentity {
            source,
            source_group_id: format!("group-{release_id}"),
            source_release_id: Some(release_id.to_string()),
        }
    }

    #[tokio::test]
    async fn identity_rows_take_their_ids_from_the_injected_provider() {
        let (db, _tmp) = db_with_sequential_ids().await;
        let now = Utc::now();

        let artist = DbArtist {
            id: "artist-1".to_string(),
            name: "Artist".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let mut album = DbAlbum::new_test("Album", &artist.id);
        album.id = "album-1".to_string();
        db.insert_album(&album).await.unwrap();
        db.insert_album_artist(&DbAlbumArtist::new(
            &album.id,
            &artist.id,
            0,
            "album-artist-1".to_string(),
            now,
        ))
        .await
        .unwrap();

        let release = DbRelease::new_test(&album.id, "release-1");
        db.insert_release(&release).await.unwrap();

        db.insert_release_identities(
            &release.id,
            &[identity(MetadataSource::Discogs, "discogs-release-1")],
        )
        .await
        .unwrap();

        let ids: Vec<String> = db
            .read(|conn| {
                let mut stmt = conn.prepare("SELECT id FROM release_identities")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                    .map_err(DbError::from)
            })
            .await
            .unwrap();

        assert_eq!(ids.len(), 1, "one identity row was written, got {ids:?}",);
        assert!(
            ids[0].starts_with("db-ids"),
            "the identity row's id comes from the injected provider, got {:?}",
            ids[0],
        );

        // Moving the release to a fresh album copies its album_artists rows; those
        // PKs are minted here too.
        let target = DbAlbum::new_test("Target Album", &artist.id);
        db.set_identity_atomic(
            &release.id,
            &[identity(MetadataSource::MusicBrainz, "mb-release-1")],
            ReleaseMetadataSource::MusicBrainz,
            Some("mb-release-1"),
            &album.id,
            &target.id,
            Some(&target),
            &[],
        )
        .await
        .unwrap();

        let copied: Vec<String> = db
            .read(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT id FROM album_artists WHERE album_id != 'album-1'")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                    .map_err(DbError::from)
            })
            .await
            .unwrap();

        assert_eq!(
            copied.len(),
            1,
            "the album_artists row was copied, got {copied:?}"
        );
        assert!(
            copied[0].starts_with("db-ids"),
            "the copied album_artists row's id comes from the injected provider, got {:?}",
            copied[0],
        );
    }
}
