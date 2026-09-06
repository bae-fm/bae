use super::*;

/// One candidate's whole subtree, counted after the rebuild.
fn subtree_counts(sql: &coven::SqlReadContext<'_>) -> Result<Vec<(String, i64)>, coven::DbError> {
    [
        "import_candidate_state",
        "import_candidate_verdict",
        "import_candidate_match",
        "import_candidate_draft_provenance",
        "import_candidate_provenance_partner",
        "import_candidate_edit",
        "import_candidate_track",
        "import_candidate_track_artist_assignment",
        "import_candidate_album_artist_assignment",
        "import_candidate_cover",
        "import_candidate_remote_cover_asset",
        "import_candidate_failure",
        "import_candidate_artist_identity_conflict",
        "import_candidate_file_edit",
        "import_candidate_session",
        "import_candidate_signals",
        "import_candidate_signal_value",
        "import_candidate_asset_preparation",
        "import_candidate_artist_asset",
        "import_candidate_source_artist",
        "import_candidate_watched_root",
    ]
    .into_iter()
    .map(|table| {
        let count: i64 = sql.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?;
        Ok((table.to_string(), count))
    })
    .collect()
}

/// The verdict leaves the candidate row for one of its own — a failed outcome
/// included, so a candidate can no longer hold two verdicts — and the draft's
/// provenance and author move onto the draft. The candidate's whole subtree is
/// rebuilt around the trimmed row, so every table under it must come across
/// intact rather than being cascaded away with the old parent.
#[tokio::test]
#[serial]
async fn migration_twenty_splits_the_verdict_and_the_provenance_off_the_candidate() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-candidate-verdict",
        version_nineteen(),
    )
    .expect("open version nineteen");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO watched_import_folders (path, position) VALUES ('/music', 0);
                 INSERT INTO artists (id, name, sort_name, created_at, _updated_at)
                     VALUES ('11111111-1111-4111-8111-111111111111', 'D', 'D', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                            ('22222222-2222-4222-8222-222222222222', 'M', 'M', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                 INSERT INTO import_candidate_state (
                     content_hash, folder_path, verdict_kind, verdict_track_count,
                     verdict_matched_barcode, probed_total_duration_ms, identified_at,
                     provenance_kind, provenance_source, provenance_release_id,
                     provenance_author, metadata_revision, edit_revision
                 ) VALUES (
                     'found-hash', '/music/found', 'found', 3, '0123456789012', 1000,
                     '2026-01-01T00:00:00Z', 'external_release', 'musicbrainz', 'mb-1',
                     'user', 2, 1
                 );
                 INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('failed-hash', '/music/failed'), ('blank-hash', '/music/blank');
                 INSERT INTO import_candidate_identify_failure (
                     content_hash, failures_json, track_count, probed_total_duration_ms, identified_at
                 ) VALUES ('failed-hash', '[{\"kind\":\"network\"}]', 5, 900, '2026-01-02T00:00:00Z');

                 INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title, barcode,
                     by_disc_id, by_barcode, by_catalog
                 ) VALUES ('found-hash', 0, 'musicbrainz', 'mb-1', 'Album', '0123456789012', 1, 0, 0);
                 INSERT INTO import_candidate_provenance_partner (content_hash, source, release_id)
                     VALUES ('found-hash', 'discogs', 'dg-1');

                 INSERT INTO import_candidate_edit (
                     content_hash, album_title, album_year, year, format, label,
                     catalog_number, country, barcode
                 ) VALUES ('found-hash', 'Album', '1999', '', '', '', '', '', ''),
                          ('failed-hash', 'Failed', '', '', '', '', '', '', ''),
                          ('blank-hash', '', '', '', '', '', '', '', '');
                 INSERT INTO import_candidate_track (
                     content_hash, track_id, position, title, artist_assignment_kind, side,
                     track_number, named_by_source, dropped, file_author
                 ) VALUES ('found-hash', 'track-0', 0, 'One', 'explicit', 1, 1, 1, 0, 'user');
                 INSERT INTO import_candidate_track_artist_assignment (
                     content_hash, track_id, position, assignment_kind, name
                 ) VALUES ('found-hash', 'track-0', 0, 'new', 'Track Artist');
                 INSERT INTO import_candidate_album_artist_assignment (
                     content_hash, position, assignment_kind, name
                 ) VALUES ('found-hash', 0, 'new', 'Album Artist');

                 INSERT INTO import_candidate_cover (content_hash, kind, url, source)
                     VALUES ('found-hash', 'remote', 'https://cover', 'musicbrainz');
                 INSERT INTO import_candidate_remote_cover_asset (content_hash, content_type, bytes)
                     VALUES ('found-hash', 'image/jpeg', x'0011');
                 INSERT INTO import_candidate_failure (content_hash, error, failed_at)
                     VALUES ('found-hash', 'boom', '2026-01-03T00:00:00Z');
                 INSERT INTO import_candidate_artist_identity_conflict (
                     content_hash, incoming_artist_name, discogs_artist_id, musicbrainz_artist_id,
                     discogs_library_artist_id, musicbrainz_library_artist_id
                 ) VALUES ('found-hash', 'Album Artist', 'dg-a', 'mb-a', '11111111-1111-4111-8111-111111111111', '22222222-2222-4222-8222-222222222222');
                 INSERT INTO import_candidate_file_edit (content_hash, relative_path, role_choice)
                     VALUES ('found-hash', 'a.flac', 'audio');
                 INSERT INTO import_candidate_session (
                     content_hash, presentation, search_tab, search_artist, search_album,
                     search_catalog, search_barcode
                 ) VALUES ('found-hash', 'draft', 'general', '', '', '', '');
                 INSERT INTO import_candidate_signals (
                     content_hash, disc_id_state, track_count, barcode_state, text_state
                 ) VALUES ('found-hash', 'absent', 3, 'settled', 'settled');
                 INSERT INTO import_candidate_signal_value (
                     content_hash, list, position, value, origin
                 ) VALUES ('found-hash', 'barcode', 0, '0123456789012', 'artwork');
                 INSERT INTO import_candidate_asset_preparation (content_hash)
                     VALUES ('found-hash');
                 INSERT INTO import_candidate_artist_asset (content_hash, discogs_artist_id, answer)
                     VALUES ('found-hash', 'dg-a', 'nothing');
                 INSERT INTO import_candidate_source_artist (content_hash, discogs_artist_id)
                     VALUES ('found-hash', 'dg-a');
                 INSERT INTO import_candidate_watched_root (content_hash, watched_folder_path)
                     VALUES ('found-hash', '/music');",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-nineteen rows");
    drop(handle);

    let handle =
        open(store_dir, "migration-candidate-verdict", all()).expect("migrate to the split rows");
    handle
        .read(|sql| {
            let verdicts = sql.query(
                "SELECT content_hash, kind, track_count, matched_barcode, failures_json, \
                        probed_total_duration_ms, identified_at \
                 FROM import_candidate_verdict ORDER BY content_hash",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )?;
            assert_eq!(
                verdicts,
                vec![
                    (
                        "failed-hash".to_string(),
                        "failed".to_string(),
                        Some(5),
                        None,
                        Some("[{\"kind\":\"network\"}]".to_string()),
                        900,
                        "2026-01-02T00:00:00Z".to_string(),
                    ),
                    (
                        "found-hash".to_string(),
                        "found".to_string(),
                        Some(3),
                        Some("0123456789012".to_string()),
                        None,
                        1000,
                        "2026-01-01T00:00:00Z".to_string(),
                    ),
                ],
                "both shapes of verdict became rows of the one verdict table"
            );

            let provenance = sql.query(
                "SELECT content_hash, kind, source, release_id, author \
                 FROM import_candidate_draft_provenance",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )?;
            assert_eq!(
                provenance,
                vec![(
                    "found-hash".to_string(),
                    "external_release".to_string(),
                    Some("musicbrainz".to_string()),
                    Some("mb-1".to_string()),
                    "user".to_string(),
                )],
                "the provenance and its author moved onto the draft"
            );

            let candidate_columns = sql.query(
                "SELECT name FROM pragma_table_info('import_candidate_state') ORDER BY name",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert_eq!(
                candidate_columns,
                vec![
                    "content_hash".to_string(),
                    "edit_revision".to_string(),
                    "folder_path".to_string(),
                    "metadata_revision".to_string(),
                ],
                "the candidate row keeps only the candidate"
            );

            let counts = subtree_counts(&sql)?;
            assert_eq!(
                counts,
                vec![
                    ("import_candidate_state".to_string(), 3),
                    ("import_candidate_verdict".to_string(), 2),
                    ("import_candidate_match".to_string(), 1),
                    ("import_candidate_draft_provenance".to_string(), 1),
                    ("import_candidate_provenance_partner".to_string(), 1),
                    ("import_candidate_edit".to_string(), 3),
                    ("import_candidate_track".to_string(), 1),
                    ("import_candidate_track_artist_assignment".to_string(), 1),
                    ("import_candidate_album_artist_assignment".to_string(), 1),
                    ("import_candidate_cover".to_string(), 1),
                    ("import_candidate_remote_cover_asset".to_string(), 1),
                    ("import_candidate_failure".to_string(), 1),
                    ("import_candidate_artist_identity_conflict".to_string(), 1),
                    ("import_candidate_file_edit".to_string(), 1),
                    ("import_candidate_session".to_string(), 1),
                    ("import_candidate_signals".to_string(), 1),
                    ("import_candidate_signal_value".to_string(), 1),
                    ("import_candidate_asset_preparation".to_string(), 1),
                    ("import_candidate_artist_asset".to_string(), 1),
                    ("import_candidate_source_artist".to_string(), 1),
                    ("import_candidate_watched_root".to_string(), 1),
                ],
                "the rebuild carried every row of the candidate's subtree"
            );

            let violations: i64 =
                sql.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(violations, 0, "every rebuilt row points at its new parent");

            let leftovers: i64 = sql.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
                 AND (name LIKE 'import_candidate%\\_v_' ESCAPE '\\' \
                      OR name = 'import_candidate_identify_failure')",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(leftovers, 0, "no rebuilt-out table stayed behind");
            Ok(())
        })
        .await
        .expect("read the split rows");

    // The verdict owns its matches now: clearing it clears them.
    handle
        .write(|sql| {
            sql.execute(
                "DELETE FROM import_candidate_verdict WHERE content_hash = 'found-hash'",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("clear the verdict");
    // The draft owns its provenance: replacing it takes the provenance and the
    // partner releases the same pick claimed.
    handle
        .write(|sql| {
            sql.execute(
                "DELETE FROM import_candidate_edit WHERE content_hash = 'found-hash'",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("forget the draft");
    handle
        .read(|sql| {
            let remaining: Vec<i64> = sql.query(
                "SELECT COUNT(*) FROM import_candidate_match \
                 UNION ALL SELECT COUNT(*) FROM import_candidate_draft_provenance \
                 UNION ALL SELECT COUNT(*) FROM import_candidate_provenance_partner \
                 UNION ALL SELECT COUNT(*) FROM import_candidate_track",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                remaining,
                vec![0, 0, 0, 0],
                "matches go with the verdict; the provenance and its partners go with the draft"
            );
            Ok(())
        })
        .await
        .expect("count what the cascades left");
}
