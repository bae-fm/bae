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
mod aggregate_ordering_tests {
    use super::super::*;
    use crate::playback::QueueEntryId;
    use coven::SystemClock;
    use std::sync::Arc;

    async fn aggregate_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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

    async fn queue_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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
        // The seeded release has no source_folder_name (a non-folder import).
        // The stored key is namespace-relative; coven prepends the `storage/`
        // audio namespace when it reads/writes the blob.
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
mod outbox_items_tests {
    use super::super::*;
    use coven::SystemClock;

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
            .await
            .unwrap();
        (db, tmp)
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

    async fn seeded_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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
            None,
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
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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
            &[file],
            &[],
            &[],
            None,
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

    #[tokio::test]
    async fn complete_import_for_release_marks_active_release_import_complete() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
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
            &[file],
            &[],
            &[],
            None,
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
}

#[cfg(test)]
mod playback_state_load_tests {
    use super::super::*;
    use coven::SystemClock;

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(path.to_str().unwrap(), Arc::new(SystemClock))
            .await
            .unwrap();
        (db, tmp)
    }

    /// `source` and `cursor` are written together, so a row carrying one without
    /// the other is corrupt: `load_playback_state` discards the whole cache and
    /// returns `None` rather than inventing a cursor.
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

        assert!(db.load_playback_state().await.unwrap().is_none());
    }
}
