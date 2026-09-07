use super::*;

#[tokio::test]
#[serial]
async fn migration_twenty_two_preserves_candidates_and_seeds_above_live_revisions() {
    for revision in [91, i64::MAX] {
        let temp = tempfile::tempdir().unwrap();
        let directory = StoreDir::new_ephemeral(temp.path());
        let handle = open(
            directory.clone(),
            "candidate-revision-migration",
            all().into_iter().take(21).collect(),
        )
        .unwrap();
        handle.write(move |sql| {
            sql.execute("INSERT INTO import_candidate_state (content_hash, folder_path, metadata_revision) VALUES ('candidate', '/candidate', ?)", [revision])?;
            sql.execute("INSERT INTO import_candidate_edit (content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode) VALUES ('candidate', 'Preserved album', '1999', '2001', 'CD', 'Test Label', 'TEST-001', 'GB', '0123456789012')", [])?;
            sql.execute("INSERT INTO import_candidate_failure (content_hash, error, failed_at) VALUES ('candidate', 'Preserved failure', '2026-01-02T03:04:05Z')", [])?;
            Ok(())
        }).await.unwrap();
        drop(handle);
        let handle = open(directory, "candidate-revision-migration", all()).unwrap();
        handle.read(move |sql| {
            let stored = sql.query_row("SELECT state.metadata_revision, draft.album_title, failure.error FROM import_candidate_state state JOIN import_candidate_edit draft USING (content_hash) JOIN import_candidate_failure failure USING (content_hash)", [], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?;
            assert_eq!(stored, (revision, "Preserved album".into(), "Preserved failure".into()));
            let draft: Vec<String> = sql.query_row("SELECT album_title, album_year, year, format, label, catalog_number, country, barcode FROM import_candidate_edit", [], |row| (0..8).map(|index| row.get(index)).collect())?;
            assert_eq!(draft, ["Preserved album", "1999", "2001", "CD", "Test Label", "TEST-001", "GB", "0123456789012"]);
            let allocated: i64 = sql.query_row("SELECT last_revision FROM import_candidate_revision WHERE singleton = 1", [], |row| row.get(0))?;
            assert_eq!(allocated, revision);
            Ok(())
        }).await.unwrap();
    }
}

async fn archive_snapshot(handle: &coven::CovenHandle, metadata_revision: i64) {
    handle.write(move |sql| {
        sql.execute("INSERT INTO import_candidate_state (content_hash, folder_path, metadata_revision) VALUES ('candidate', '/watched/candidate', ?)", [metadata_revision])?;
        sql.execute_batch("INSERT INTO watched_import_folders (path, position) VALUES ('/watched', 0);
            INSERT INTO folder_scan_roots (watched_folder_path, generation, status) VALUES ('/watched', 7, 'complete');
            INSERT INTO scan_candidate (watched_folder_path, path, generation, kind, name, display_path, file_root, scope, content_hash, file_edit_revision, initial_metadata_source)
                VALUES ('/watched', '/watched/candidate', 7, 'valid', 'Candidate', 'candidate', '/watched/candidate', 'direct', 'candidate', 3, 'file_tags');
            INSERT INTO scan_candidate_file (watched_folder_path, candidate_path, relative_path, position, absolute_path, size, modified_at_ns, file_name, proposed_audio, role)
                VALUES ('/watched', '/watched/candidate', 'track.flac', 0, '/watched/candidate/track.flac', 123, 456, 'track.flac', 0, 'other');
            INSERT INTO scan_candidate_tag_snapshot (watched_folder_path, candidate_path, scan_generation, file_edit_revision, embedded_cover_source_relative_path, embedded_cover_content_type, embedded_cover_data)
                VALUES ('/watched', '/watched/candidate', 7, 3, 'track.flac', 'image/png', X'010203');
            INSERT INTO scan_candidate_file_tag (watched_folder_path, candidate_path, relative_path, file_size, modified_at_ns, title, album_title)
                VALUES ('/watched', '/watched/candidate', 'track.flac', 123, 456, 'Preserved track', 'Preserved album');")?;
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
#[serial]
async fn migration_twenty_two_keeps_snapshot_facts_artwork_and_cascades() {
    let temp = tempfile::tempdir().unwrap();
    let directory = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        directory.clone(),
        "snapshot-revision-migration",
        all().into_iter().take(21).collect(),
    )
    .unwrap();
    archive_snapshot(&handle, 91).await;
    drop(handle);
    let handle = open(directory, "snapshot-revision-migration", all()).unwrap();
    handle.write(|sql| {
        let header = sql.query_row("SELECT scan_generation, file_edit_revision, revision, embedded_cover_source_relative_path, embedded_cover_content_type, embedded_cover_data FROM scan_candidate_tag_snapshot", [], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, Vec<u8>>(5)?)))?;
        assert_eq!(header, (7, 3, 92, "track.flac".into(), "image/png".into(), vec![1,2,3]));
        let facts = sql.query_row("SELECT file_size, modified_at_ns, title, album_title FROM scan_candidate_file_tag", [], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)))?;
        assert_eq!(facts, (123, 456, "Preserved track".into(), "Preserved album".into()));
        let violations = sql.query("PRAGMA foreign_key_check", [], |row| row.get::<_, String>(0))?;
        assert!(violations.is_empty());
        sql.execute("DELETE FROM scan_candidate WHERE path = '/watched/candidate'", [])?;
        let remaining: i64 = sql.query_row("SELECT (SELECT COUNT(*) FROM scan_candidate_tag_snapshot) + (SELECT COUNT(*) FROM scan_candidate_file_tag)", [], |row| row.get(0))?;
        assert_eq!(remaining, 0);
        let high_water: i64 = sql.query_row("SELECT last_revision FROM import_candidate_revision WHERE singleton = 1", [], |row| row.get(0))?;
        assert_eq!(high_water, 92, "deleting the candidate does not reclaim its version");
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
#[serial]
async fn migration_twenty_two_rolls_back_when_snapshot_versions_are_exhausted() {
    let temp = tempfile::tempdir().unwrap();
    let directory = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        directory.clone(),
        "snapshot-revision-exhausted",
        all().into_iter().take(21).collect(),
    )
    .unwrap();
    archive_snapshot(&handle, i64::MAX).await;
    drop(handle);
    assert!(matches!(
        open(directory.clone(), "snapshot-revision-exhausted", all()),
        Err(CovenError::Migration(MigrationError::Failed {
            version: 22,
            ..
        }))
    ));
    let sql = coven::rusqlite::Connection::open(directory.db_path()).unwrap();
    let version: i64 = sql
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 21);
    let bytes: Vec<u8> = sql
        .query_row(
            "SELECT embedded_cover_data FROM scan_candidate_tag_snapshot",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bytes, vec![1, 2, 3]);
    let title: String = sql
        .query_row("SELECT title FROM scan_candidate_file_tag", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(title, "Preserved track");
}
