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
        // Keyed by the artist id alone — no DB lookup.
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

        // Write a row by hand with a present source but a NULL cursor —
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
