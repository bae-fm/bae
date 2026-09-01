use super::*;

#[tokio::test]
#[serial]
async fn records_live_roots_and_removes_unowned_candidate_state() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-candidate-roots",
        version_nine(),
    )
    .expect("open version nine");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position)
                     VALUES ('/first', 0), ('/second', 1);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/first', 1, 'complete'), ('/second', 2, 'complete');
                 INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('shared-hash', '/first/release'),
                            ('pruned-hash', 'release');
                 INSERT INTO scan_candidate (
                     watched_folder_path, path, generation, kind, name, display_path,
                     file_root, scope, content_hash, initial_metadata_source
                 ) VALUES
                     ('/first', '/first/release', 1, 'valid', 'release', 'release',
                      '/first/release', 'direct', 'shared-hash', 'none'),
                     ('/second', '/second/release', 2, 'valid', 'release', 'release',
                      '/second/release', 'direct', 'shared-hash', 'none');",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-nine shared candidate");
    drop(handle);

    let handle = open(store_dir, "migration-candidate-roots", all())
        .expect("migrate candidate watched roots");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, 10);
            let roots = sql.query(
                "SELECT watched_folder_path FROM import_candidate_watched_root
                 WHERE content_hash = 'shared-hash' ORDER BY watched_folder_path",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert_eq!(roots, vec!["/first".to_string(), "/second".to_string()]);
            let pruned_state = sql.query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM import_candidate_state WHERE content_hash = 'pruned-hash'
                 )",
                [],
                |row| row.get::<_, bool>(0),
            )?;
            assert!(!pruned_state);
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("verify candidate root ownership");
}
