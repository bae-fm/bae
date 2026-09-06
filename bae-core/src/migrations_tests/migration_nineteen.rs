use super::*;

/// One merged row, as the migration wrote it.
#[derive(Debug, PartialEq)]
struct MergedTrackRow {
    track_id: String,
    position: i64,
    title: String,
    named_by_source: i64,
    dropped: i64,
    file_author: String,
    file_kind: Option<String>,
    file_id: Option<String>,
}

/// The two track tables become one row per track. A draft track keeps its
/// mapping's decisions; one whose mapping row was missing takes a fresh
/// projection's decisions; a mapping row naming no draft track — the
/// inconsistency the merge exists to refuse — does not come across. The
/// track artist assignments follow their track, cascade included.
#[tokio::test]
#[serial]
async fn migration_nineteen_merges_each_track_with_its_decisions() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-candidate-track",
        version_eighteen(),
    )
    .expect("open version eighteen");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('candidate-hash', '/music/release');
                 INSERT INTO import_candidate_edit (
                     content_hash, album_title, album_year, year, format, label,
                     catalog_number, country, barcode
                 ) VALUES ('candidate-hash', 'Album', '', '', '', '', '', '', '');
                 INSERT INTO import_candidate_track_edit (
                     content_hash, track_id, position, title, artist_assignment_kind, side, track_number
                 ) VALUES
                     ('candidate-hash', 'candidate-track-0', 0, 'Mapped', 'explicit', 1, 1),
                     ('candidate-hash', 'candidate-track-1', 1, 'Unmapped', 'album_artists', 1, 2);
                 INSERT INTO import_candidate_track_artist_assignment (
                     content_hash, track_id, position, assignment_kind, name
                 ) VALUES ('candidate-hash', 'candidate-track-0', 0, 'new', 'Track Artist');
                 INSERT INTO import_candidate_track_mapping (
                     content_hash, track_id, named_by_source, dropped, file_author,
                     file_kind, file_id, sheet_id, slice_index
                 ) VALUES
                     ('candidate-hash', 'candidate-track-0', 0, 0, 'user', 'standalone', 'a.flac', NULL, NULL),
                     ('candidate-hash', 'candidate-track-9', 1, 0, 'automatic', NULL, NULL, NULL, NULL);",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-eighteen rows");
    drop(handle);

    let handle = open(store_dir, "migration-candidate-track", all())
        .expect("migrate to the merged track table");
    handle
        .read(|sql| {
            let rows = sql.query(
                "SELECT track_id, position, title, named_by_source, dropped, file_author, \
                        file_kind, file_id \
                 FROM import_candidate_track WHERE content_hash = 'candidate-hash' \
                 ORDER BY position",
                [],
                |row| {
                    Ok(MergedTrackRow {
                        track_id: row.get(0)?,
                        position: row.get(1)?,
                        title: row.get(2)?,
                        named_by_source: row.get(3)?,
                        dropped: row.get(4)?,
                        file_author: row.get(5)?,
                        file_kind: row.get(6)?,
                        file_id: row.get(7)?,
                    })
                },
            )?;
            assert_eq!(
                rows,
                vec![
                    MergedTrackRow {
                        track_id: "candidate-track-0".to_string(),
                        position: 0,
                        title: "Mapped".to_string(),
                        named_by_source: 0,
                        dropped: 0,
                        file_author: "user".to_string(),
                        file_kind: Some("standalone".to_string()),
                        file_id: Some("a.flac".to_string()),
                    },
                    MergedTrackRow {
                        track_id: "candidate-track-1".to_string(),
                        position: 1,
                        title: "Unmapped".to_string(),
                        named_by_source: 1,
                        dropped: 0,
                        file_author: "automatic".to_string(),
                        file_kind: None,
                        file_id: None,
                    },
                ],
                "each draft track carries its decisions; the orphan mapping is gone"
            );
            let assignments: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_track_artist_assignment \
                 WHERE content_hash = 'candidate-hash' AND track_id = 'candidate-track-0'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(assignments, 1, "the track's artist assignment came across");
            let old_tables: i64 = sql.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
                 AND name IN ('import_candidate_track_edit', 'import_candidate_track_mapping')",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(old_tables, 0, "the two halves are gone");
            Ok(())
        })
        .await
        .expect("read migrated tracks");

    handle
        .write(|sql| {
            sql.execute(
                "DELETE FROM import_candidate_edit WHERE content_hash = 'candidate-hash'",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("forget the draft");
    handle
        .read(|sql| {
            let remaining: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_track \
                 UNION ALL SELECT COUNT(*) FROM import_candidate_track_artist_assignment",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                remaining, 0,
                "tracks and their assignments go with the draft"
            );
            Ok(())
        })
        .await
        .expect("count what the cascade left");
}
