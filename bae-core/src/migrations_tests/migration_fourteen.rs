use super::*;

#[tokio::test]
#[serial]
async fn replaces_cached_cue_bindings_with_the_resolved_shape() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-cue-binding",
        version_thirteen(),
    )
    .expect("open version thirteen");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position)
                     VALUES ('/music', 0);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 1, 'complete');
                 INSERT INTO scan_candidate (
                     watched_folder_path, path, generation, kind, name, display_path,
                     file_root, scope, content_hash, initial_metadata_source
                 ) VALUES (
                     '/music', '/music/release', 1, 'valid', 'release', 'release',
                     '/music/release', 'direct', 'candidate-hash', 'none'
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed a cached scan");
    drop(handle);

    let handle =
        open(store_dir, "migration-cue-binding", all()).expect("migrate CUE binding resolution");
    handle
        .write(|sql| {
            let watched: i64 = sql.query_row(
                "SELECT COUNT(*) FROM watched_import_folders WHERE path = '/music'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(watched, 1, "the configured watched folder remains");
            let cached: i64 =
                sql.query_row("SELECT COUNT(*) FROM scan_candidate", [], |row| row.get(0))?;
            assert_eq!(cached, 0, "old derived scans are removed");

            sql.execute_batch(
                "INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 2, 'scanning');
                 INSERT INTO scan_candidate (
                     watched_folder_path, path, generation, kind, name, display_path,
                     file_root, scope, content_hash, initial_metadata_source
                 ) VALUES (
                     '/music', '/music/release', 2, 'valid', 'release', 'release',
                     '/music/release', 'direct', 'candidate-hash', 'none'
                 );
                 INSERT INTO scan_candidate_file (
                     watched_folder_path, candidate_path, relative_path, position,
                     absolute_path, size, modified_at_ns, file_name, proposed_audio,
                     role, sheet_binding, sheet_disc, sheet_disc_number
                 ) VALUES (
                     '/music', '/music/release', 'disc.cue', 0,
                     '/music/release/disc.cue', 100, 1, 'disc.cue', 0,
                     'track_sheet', 'resolved', 'disc', 1
                 );",
            )?;
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("write the resolved binding shape");
}
