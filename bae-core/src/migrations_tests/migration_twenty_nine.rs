use super::*;

/// Dropping the numbering choice rebuilds the combination table. Its members
/// hang off it by a cascading key, and its own scan row is deleted by a trigger
/// on it — so the rebuild has to carry the members across and leave the scan row
/// alone.
#[tokio::test]
#[serial]
async fn the_combination_rebuild_keeps_its_folders_and_its_queue_row() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-combination-order",
        version_twenty_eight(),
    )
    .expect("open version twenty-eight");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO folder_scan_roots (watched_folder_path, generation, status)
                     VALUES ('/music', 4, 'complete');
                 INSERT INTO scan_candidate (
                     watched_folder_path, path, generation, kind, name, display_path,
                     content_hash, source_kind, first_seen_at
                 ) VALUES (
                     '/music', 'combination:one', 4, 'valid', 'Collected Volumes',
                     'Collected Volumes', 'combined-hash', 'combination', 1000
                 );
                 INSERT INTO candidate_combination (
                     candidate_key, watched_folder_path, name, track_order, skipped,
                     created_at, error
                 ) VALUES (
                     'combination:one', '/music', 'Collected Volumes', 'continuous', 1,
                     1000, 'Source folder changed: Volume A'
                 );
                 INSERT INTO candidate_combination_member (
                     combination_key, position, candidate_key, watched_folder_path,
                     folder_name, file_prefix, first_disc, disc_count, track_count
                 ) VALUES
                     ('combination:one', 0, '/music/Volume A', '/music', 'Volume A',
                      '01 - Volume A/', 1, 1, 2),
                     ('combination:one', 1, '/music/Volume B', '/music', 'Volume B',
                      '02 - Volume B/', 2, 1, 3);",
            )?;
            Ok(())
        })
        .await
        .expect("seed a version-twenty-eight combination");
    drop(handle);

    let handle = open(store_dir, "migration-combination-order", all())
        .expect("migrate past the numbering choice");
    handle
        .read(|sql| {
            let columns = sql.query(
                "SELECT name FROM pragma_table_info('candidate_combination') ORDER BY name",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert_eq!(
                columns,
                vec![
                    "candidate_key".to_string(),
                    "created_at".to_string(),
                    "error".to_string(),
                    "name".to_string(),
                    "skipped".to_string(),
                    "watched_folder_path".to_string(),
                ],
                "the numbering choice is gone and every other column stays"
            );

            let combination: (String, String, bool, i64, Option<String>) = sql.query_row(
                "SELECT watched_folder_path, name, skipped, created_at, error \
                 FROM candidate_combination WHERE candidate_key = 'combination:one'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )?;
            assert_eq!(
                combination,
                (
                    "/music".to_string(),
                    "Collected Volumes".to_string(),
                    true,
                    1000,
                    Some("Source folder changed: Volume A".to_string()),
                ),
                "the combination keeps everything the rebuild was not about"
            );

            let members = sql.query(
                "SELECT candidate_key, folder_name, file_prefix, first_disc, disc_count, \
                 track_count FROM candidate_combination_member \
                 WHERE combination_key = 'combination:one' ORDER BY position",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )?;
            assert_eq!(
                members,
                vec![
                    (
                        "/music/Volume A".to_string(),
                        "Volume A".to_string(),
                        "01 - Volume A/".to_string(),
                        1,
                        1,
                        2,
                    ),
                    (
                        "/music/Volume B".to_string(),
                        "Volume B".to_string(),
                        "02 - Volume B/".to_string(),
                        2,
                        1,
                        3,
                    ),
                ],
                "the folders the combination is made of survive its rebuild"
            );

            let queued: i64 = sql.query_row(
                "SELECT COUNT(*) FROM scan_candidate WHERE path = 'combination:one'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                queued, 1,
                "the combination's own queue row is not swept by its delete trigger"
            );
            Ok(())
        })
        .await
        .expect("read the rebuilt combination");
}
