use super::*;

/// The match rows survive with a blank barcode — the column is new, and no
/// stored match knows one. The failure rows do not: `failures_json` now names
/// the provider that failed, so a row written under the old shape describes a
/// failure the reducer can no longer read. They are device-local derived
/// state, so dropping them costs a re-run of the queue sweep rather than an
/// answer.
#[tokio::test]
#[serial]
async fn records_a_blank_match_barcode_and_drops_stale_identify_failures() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-match-barcode",
        version_eleven(),
    )
    .expect("open version eleven");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (
                     content_hash, folder_path, verdict_kind, verdict_track_count,
                     probed_total_duration_ms, identified_at
                 ) VALUES (
                     'found-hash', '/music/found', 'found', 1, 1000,
                     '2026-01-02T03:04:05Z'
                 ), (
                     'failed-hash', '/music/failed', NULL, NULL, NULL, NULL
                 );
                 INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title, artist, year,
                     format, label, catalog_number, country, cover_url,
                     cover_thumbnail_url, cover_label, cover_source, source_group_id,
                     source_tracks_kind, source_tracks_count, source_tracks_total_ms,
                     by_disc_id, by_barcode, by_catalog
                 ) VALUES (
                     'found-hash', 0, 'musicbrainz', 'rel-123', 'Album Title',
                     'Artist Name', 2001, 'CD', 'Label Name', 'CAT-1', 'US',
                     NULL, NULL, NULL, NULL, 'group-1', 'listed', 1, 1000, 1, 0, 0
                 );
                 INSERT INTO import_candidate_identify_failure (
                     content_hash, failures_json, track_count,
                     probed_total_duration_ms, identified_at
                 ) VALUES (
                     'failed-hash', '[{\"Barcode\":\"Network\"}]', 1, 1000,
                     '2026-01-02T03:04:05Z'
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-eleven verdicts");
    drop(handle);

    let handle =
        open(store_dir, "migration-match-barcode", all()).expect("migrate the match barcode");
    handle
        .read(|sql| {
            let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            assert_eq!(version, i64::try_from(all().len()).expect("ladder fits"));
            let (release_id, barcode): (String, Option<String>) = sql.query_row(
                "SELECT release_id, barcode FROM import_candidate_match \
                 WHERE content_hash = 'found-hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(release_id, "rel-123");
            assert_eq!(barcode, None, "no stored match knows its barcode yet");
            let failures: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_verdict WHERE kind = 'failed'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(failures, 0, "stale failure rows are dropped");
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());
            Ok(())
        })
        .await
        .expect("read the migrated verdicts");
}
