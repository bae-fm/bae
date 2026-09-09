use super::*;

/// Dropping the matched barcode rebuilds the verdict table, and the match rows
/// hang off it — so the rebuild has to carry them across rather than let the
/// old parent cascade them away.
#[tokio::test]
#[serial]
async fn the_verdict_rebuild_keeps_the_releases_it_found() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-text-pool",
        version_twenty_four(),
    )
    .expect("open version twenty-four");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('found-hash', '/music/found');
                 INSERT INTO import_candidate_verdict (
                     content_hash, kind, track_count, matched_barcode,
                     probed_total_duration_ms, identified_at
                 ) VALUES (
                     'found-hash', 'found', 3, '0123456789012', 1000,
                     '2026-01-01T00:00:00Z'
                 );
                 INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title,
                     by_disc_id, by_barcode, by_catalog, narrowed_out
                 ) VALUES
                     ('found-hash', 0, 'musicbrainz', 'mb-1', 'Album', 1, 0, 0, 0),
                     ('found-hash', 1, 'discogs', 'dg-1', 'Album', 0, 1, 0, 1);",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-twenty-four rows");
    drop(handle);

    let handle = open(store_dir, "migration-text-pool", all()).expect("migrate to the text pool");
    handle
        .read(|sql| {
            let matches = sql.query(
                "SELECT release_id, by_barcode, narrowed_out FROM import_candidate_match \
                 ORDER BY position",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, bool>(2)?,
                    ))
                },
            )?;
            assert_eq!(
                matches,
                vec![
                    ("mb-1".to_string(), false, false),
                    ("dg-1".to_string(), true, true),
                ],
                "the releases a verdict found survive the rebuild that drops its \
                 matched barcode, narrowed-out marks and all"
            );

            let pool: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_text_line",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(pool, 0, "a candidate migrated in has no text pool yet");
            Ok(())
        })
        .await
        .expect("read the rebuilt rows");
}
