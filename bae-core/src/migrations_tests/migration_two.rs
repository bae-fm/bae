use super::*;

#[tokio::test]
#[serial]
async fn migration_two_preserves_candidate_graph_and_normalizes_artist_edits() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-preserves", version_one()).expect("open version one");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (
                     content_hash, folder_path, verdict_kind, verdict_track_count,
                     probed_total_duration_ms, identified_at, pick_kind, pick_source,
                     pick_release_id, identity_pick_author
                 ) VALUES
                     ('external-hash', '/candidate/external', 'found', 1, 1000,
                      '2026-01-02T03:04:05Z', 'release', 'musicbrainz',
                      'source-release', 'user'),
                     ('tags-hash', '/candidate/tags', NULL, NULL, NULL, NULL,
                      'unknown', NULL, NULL, 'user'),
                     ('neutral-hash', '/candidate/neutral', NULL, NULL, NULL, NULL,
                      NULL, NULL, NULL, NULL);

                 INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title, artist, year,
                     format, label, catalog_number, country, cover_url,
                     cover_thumbnail_url, cover_label, cover_source, source_group_id,
                     source_tracks_kind, source_tracks_count, source_tracks_total_ms,
                     by_disc_id, by_barcode, by_catalog
                 ) VALUES (
                     'external-hash', 0, 'musicbrainz', 'source-release', 'Album Alpha',
                     'Artist Alpha', 2001, 'CD', 'Label Alpha', 'CAT-1', 'US',
                     NULL, NULL, NULL, NULL, 'source-group', 'listed', 1, 1000, 1, 0, 0
                 );
                 INSERT INTO import_candidate_file_edit
                     (content_hash, relative_path, role_choice)
                     VALUES ('external-hash', '01.flac', 'audio');
                 INSERT INTO import_candidate_file_duration
                     (content_hash, kind, relative_path, duration_ms)
                     VALUES ('external-hash', 'file', '01.flac', 1000);
                 INSERT INTO import_candidate_signals (
                     content_hash, disc_id_state, disc_id, disc_id_source_file,
                     track_count, barcode_state, text_state
                 ) VALUES (
                     'external-hash', 'computed', 'disc-id', 'rip.log', 1,
                     'settled', 'settled'
                 );
                 INSERT INTO import_candidate_signal_value
                     (content_hash, list, position, value, origin, origin_path)
                     VALUES ('external-hash', 'free_text', 0, 'query text', NULL, NULL);
                 INSERT INTO import_candidate_failure
                     (content_hash, error, failed_at)
                     VALUES ('external-hash', 'failed import', '2026-01-02T03:04:05Z');
                 INSERT INTO import_candidate_cover
                     (content_hash, kind, file_id, url, source)
                     VALUES ('external-hash', 'local', 'cover.jpg', NULL, NULL);
                 INSERT INTO import_candidate_edit (
                     content_hash, album_title, album_artist_text
                 ) VALUES (
                     'external-hash', 'Edited Album', ' Artist Alpha, Artist Beta '
                 );
                 INSERT INTO import_candidate_track_edit (
                     content_hash, track_id, dropped, title, artist_text, side,
                     track_number, file_kind, file_id, sheet_id, slice_index
                 ) VALUES (
                     'external-hash', 'import-track:0', 0, 'Edited Track',
                     'Artist Gamma, Artist Delta', 1, 1, 'standalone', '01.flac',
                     NULL, NULL
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-one candidate graph");
    drop(handle);

    let handle =
        open(store_dir, "migration-preserves", version_two()).expect("migrate to version two");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 2);
            let seeds = sql.query(
                "SELECT content_hash, seed_kind, seed_source, seed_release_id,
                        metadata_seed_author
                 FROM import_candidate_state ORDER BY content_hash",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )?;
            assert_eq!(
                seeds,
                vec![
                    (
                        "external-hash".to_string(),
                        Some("external_release".to_string()),
                        Some("musicbrainz".to_string()),
                        Some("source-release".to_string()),
                        Some("user".to_string()),
                    ),
                    ("neutral-hash".to_string(), None, None, None, None),
                    (
                        "tags-hash".to_string(),
                        Some("file_tags".to_string()),
                        None,
                        None,
                        Some("user".to_string()),
                    ),
                ]
            );
            let album_artists = sql.query(
                "SELECT position, name FROM import_candidate_album_artist_assignment
                 WHERE content_hash = 'external-hash' ORDER BY position",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?;
            assert_eq!(
                album_artists,
                vec![
                    (0, "Artist Alpha".to_string()),
                    (1, "Artist Beta".to_string())
                ]
            );
            let track_artists = sql.query(
                "SELECT position, name FROM import_candidate_track_artist_assignment
                 WHERE content_hash = 'external-hash' AND track_id = 'import-track:0'
                 ORDER BY position",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?;
            assert_eq!(
                track_artists,
                vec![
                    (0, "Artist Gamma".to_string()),
                    (1, "Artist Delta".to_string())
                ]
            );
            for table in [
                "import_candidate_match",
                "import_candidate_file_edit",
                "import_candidate_signals",
                "import_candidate_signal_value",
                "import_candidate_failure",
                "import_candidate_cover",
                "import_candidate_edit",
                "import_candidate_track_edit",
            ] {
                let count: i64 =
                    sql.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
                assert_eq!(count, 1, "{table} row preserved");
            }
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("read migrated candidate graph");
}

#[tokio::test]
#[serial]
async fn migration_two_renames_file_tag_track_edit_ids() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-track-ids", version_one()).expect("open version one");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (
                     content_hash, folder_path, pick_kind, identity_pick_author
                 ) VALUES ('tags-hash', '/candidate/tags', 'unknown', 'user');
                 INSERT INTO import_candidate_track_edit (
                     content_hash, track_id, dropped, title, artist_text, side,
                     track_number, file_kind, file_id, sheet_id, slice_index
                 ) VALUES (
                     'tags-hash', 'unknown-track-0', 0, 'Track Title',
                     'Artist Name', 1, 1, 'standalone', '01.flac', NULL, NULL
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-one File Tags edit");
    drop(handle);

    let handle =
        open(store_dir, "migration-track-ids", version_two()).expect("migrate File Tags edit");
    handle
        .read(|sql| {
            let track_id: String = sql.query_row(
                "SELECT track_id FROM import_candidate_track_edit",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(track_id, "file-tag-track-0");
            let assignment_track_id: String = sql.query_row(
                "SELECT track_id FROM import_candidate_track_artist_assignment",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(assignment_track_id, "file-tag-track-0");
            Ok(())
        })
        .await
        .expect("read migrated File Tags edit");
}

#[tokio::test]
#[serial]
async fn rejected_artist_backfill_rolls_back_the_whole_migration() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle =
        open(store_dir.clone(), "migration-rollback", version_one()).expect("open version one");
    handle
        .write(|sql| {
            sql.execute(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                 VALUES ('invalid-edit', '/candidate/invalid')",
                [],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_edit (content_hash, album_artist_text)
                 VALUES ('invalid-edit', ' , , ')",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("seed invalid version-one edit");
    drop(handle);

    let error = match open(store_dir.clone(), "migration-rollback", version_two()) {
        Ok(_) => panic!("empty artist override must reject migration"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        CovenError::Migration(MigrationError::Failed { version: 2, .. })
    ));

    let connection =
        coven::rusqlite::Connection::open(store_dir.db_path()).expect("open rolled-back database");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("read rolled-back version");
    assert_eq!(version, 1);
    let columns: Vec<String> = connection
        .prepare("PRAGMA table_info(import_candidate_state)")
        .expect("prepare column read")
        .query_map([], |row| row.get(1))
        .expect("read columns")
        .collect::<Result<_, _>>()
        .expect("collect columns");
    assert!(columns.iter().any(|column| column == "pick_kind"));
    assert!(!columns.iter().any(|column| column == "seed_kind"));
}
