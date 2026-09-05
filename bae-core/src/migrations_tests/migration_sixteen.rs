use super::*;

#[tokio::test]
#[serial]
async fn candidate_dates_preserve_candidates_and_require_fresh_directory_observation() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let mut previous = all();
    previous.truncate(15);
    let handle = open(store_dir.clone(), "migration-candidate-dates", previous)
        .expect("open version fifteen");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
             INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                 VALUES ('/music', 1, 'complete');
             INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at)
                 VALUES ('/music', '/music/release', 1234);
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
        .expect("seed candidate and directory cache");
    drop(handle);

    let handle =
        open(store_dir, "migration-candidate-dates", all()).expect("migrate candidate dates");
    handle
        .read(|sql| {
            let row: (String, Option<i64>, Option<i64>, Option<String>) = sql.query_row(
                "SELECT content_hash, first_seen_at, source_date, source_date_kind
             FROM scan_candidate WHERE path = '/music/release'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
            assert_eq!(row, ("candidate-hash".to_owned(), None, None, None));
            let cached: i64 =
                sql.query_row("SELECT COUNT(*) FROM folder_scan_directory", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(cached, 0);
            let roots: i64 =
                sql.query_row("SELECT COUNT(*) FROM folder_scan_roots", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(roots, 1);
            Ok(())
        })
        .await
        .expect("verify migration preserves candidates without invented dates");
    handle
        .write(|sql| {
            for (date, kind) in [
                (Some(123_i64), None),
                (None, Some("created")),
                (Some(123), Some("unknown")),
            ] {
                assert!(sql
                    .execute(
                        "UPDATE scan_candidate SET source_date = ?1, source_date_kind = ?2",
                        coven::rusqlite::params![date, kind],
                    )
                    .is_err());
            }
            sql.execute(
                "UPDATE scan_candidate SET source_date = 123, source_date_kind = 'created'",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("verify timestamp and source must be stored together");
}
