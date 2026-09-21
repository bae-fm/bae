use super::*;

fn version_forty_four() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(44);
    migrations
}

/// Four candidates as an earlier schema left them: one whose folder holds
/// images, one whose audio embeds artwork, one the person already chose a
/// cover for, and one whose folder holds no image at all.
async fn seed_candidates(handle: &coven::CovenHandle) {
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 1, 'complete');
                 INSERT INTO scan_candidate
                     (watched_folder_path, path, generation, kind, name, display_path,
                      file_root, scope, content_hash, file_edit_revision)
                 VALUES
                     ('/music', '/music/illustrated', 1, 'valid', 'Illustrated', 'illustrated',
                      '/music/illustrated', 'direct', 'hash-illustrated', 0),
                     ('/music', '/music/tagged', 1, 'valid', 'Tagged', 'tagged',
                      '/music/tagged', 'direct', 'hash-tagged', 0),
                     ('/music', '/music/chosen', 1, 'valid', 'Chosen', 'chosen',
                      '/music/chosen', 'direct', 'hash-chosen', 0),
                     ('/music', '/music/bare', 1, 'valid', 'Bare', 'bare',
                      '/music/bare', 'direct', 'hash-bare', 0);
                 INSERT INTO import_candidate_state (content_hash, folder_path) VALUES
                     ('hash-illustrated', '/music/illustrated'),
                     ('hash-tagged', '/music/tagged'),
                     ('hash-chosen', '/music/chosen'),
                     ('hash-bare', '/music/bare');
                 INSERT INTO scan_candidate_file
                     (watched_folder_path, candidate_path, relative_path, position,
                      absolute_path, size, modified_at_ns, file_name, dir_prefix,
                      proposed_audio, role)
                 VALUES
                     ('/music', '/music/illustrated', 'back.jpg', 0,
                      '/music/illustrated/back.jpg', 20, 1, 'back.jpg', NULL, 0, 'artwork'),
                     ('/music', '/music/illustrated', 'cover.jpg', 1,
                      '/music/illustrated/cover.jpg', 10, 1, 'cover.jpg', NULL, 0, 'artwork'),
                     ('/music', '/music/tagged', 'folder.png', 0,
                      '/music/tagged/folder.png', 10, 1, 'folder.png', NULL, 0, 'artwork'),
                     ('/music', '/music/chosen', 'cover.jpg', 0,
                      '/music/chosen/cover.jpg', 10, 1, 'cover.jpg', NULL, 0, 'artwork'),
                     ('/music', '/music/bare', 'notes.txt', 0,
                      '/music/bare/notes.txt', 10, 1, 'notes.txt', NULL, 0, 'document');
                 INSERT INTO scan_candidate_tag_snapshot
                     (watched_folder_path, candidate_path, scan_generation, file_edit_revision,
                      embedded_cover_source_relative_path, embedded_cover_content_type,
                      embedded_cover_data)
                 VALUES ('/music', '/music/tagged', 1, 0, '01.flac', 'image/jpeg', X'00');
                 INSERT INTO import_candidate_cover (content_hash, kind, file_id)
                     VALUES ('hash-chosen', 'local', 'chosen.jpg');",
            )?;
            Ok(())
        })
        .await
        .unwrap();
}

/// A candidate whose cover was never stored gets the one its folder gives
/// it — the artwork its audio embeds, else the folder's own image — so
/// nothing loses the cover it was showing. A folder with no image gets no
/// row, and a selection somebody already made stands.
#[tokio::test]
#[serial]
async fn empty_cover_selections_are_filled_from_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let handle = open(store.clone(), "folder-covers", version_forty_four()).unwrap();
    seed_candidates(&handle).await;
    drop(handle);

    let db = crate::db::Database::open(
        store.clone(),
        config("folder-covers"),
        Arc::new(FixedClock(
            Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
                .single()
                .expect("valid migration test instant"),
        )),
        Arc::new(coven::UuidProvider),
        fixture_synced_tables(),
        None,
    )
    .unwrap();
    drop(db);

    let connection = coven::rusqlite::Connection::open(store.db_path()).unwrap();
    let covers: Vec<(String, String, Option<String>)> = connection
        .prepare(
            "SELECT content_hash, kind, file_id FROM import_candidate_cover ORDER BY content_hash",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        covers,
        vec![
            (
                "hash-chosen".to_string(),
                "local".to_string(),
                Some("chosen.jpg".to_string())
            ),
            (
                "hash-illustrated".to_string(),
                "local".to_string(),
                Some("cover.jpg".to_string())
            ),
            (
                "hash-tagged".to_string(),
                "embedded".to_string(),
                Some("01.flac".to_string())
            ),
        ],
        "the bare folder gets no cover, and nothing overwrites a stored one"
    );
}
