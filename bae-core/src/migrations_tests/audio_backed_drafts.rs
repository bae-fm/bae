use super::*;

#[tokio::test]
#[serial]
async fn audio_backed_drafts_preserve_included_edits_and_source_positions() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let mut migrations = all();
    migrations.truncate(36);
    let handle = open(store.clone(), "audio-draft", migrations).unwrap();
    handle.write(|sql| {
        sql.execute_batch(
            "INSERT INTO import_candidate_state(content_hash, folder_path) VALUES ('hash', '/music/album');
             INSERT INTO import_candidate_edit(content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode)
                 VALUES ('hash', 'Typed album', '', '', '', '', '', '', '');
             INSERT INTO import_candidate_draft_provenance(content_hash, kind, source, release_id, author)
                 VALUES ('hash', 'external_release', 'musicbrainz', 'release', 'user');
             INSERT INTO import_candidate_track(content_hash, track_id, position, title, artist_assignment_kind, side, track_number, named_by_source, dropped, file_author, file_kind, file_id)
                 VALUES ('hash', 'kept', 2, 'Typed track', 'explicit', 1, NULL, 1, 0, 'user', 'standalone', 'kept.flac'),
                        ('hash', 'removed', 0, 'Removed', 'album_artists', 1, 1, 1, 1, 'automatic', NULL, NULL),
                        ('hash', 'missing', 1, 'Missing', 'album_artists', 1, 2, 1, 0, 'automatic', NULL, NULL);
             INSERT INTO import_candidate_track_artist_assignment(content_hash, track_id, position, assignment_kind, name)
                 VALUES ('hash', 'kept', 0, 'new', 'Typed Artist');"
        )?;
        Ok(())
    }).await.unwrap();
    drop(handle);
    let handle = open(store, "audio-draft", all()).unwrap();
    handle.read(|sql| {
        let tracks = sql.query("SELECT track_id, title, track_number, source_index, file_id FROM import_candidate_track ORDER BY position", [], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?, row.get::<_, i32>(3)?, row.get::<_, String>(4)?))
        })?;
        assert_eq!(tracks, vec![("kept".into(), "Typed track".into(), 3, 2, "kept.flac".into())]);
        let artist: String = sql.query_row("SELECT name FROM import_candidate_track_artist_assignment", [], |row| row.get(0))?;
        assert_eq!(artist, "Typed Artist");
        let violations = sql.query("PRAGMA foreign_key_check", [], |row| row.get::<_, String>(0))?;
        assert!(violations.is_empty());
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
#[serial]
async fn cue_reference_migration_keeps_association_separate_from_selection() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let mut migrations = all();
    migrations.truncate(36);
    let handle = open(store.clone(), "cue-reference", migrations).unwrap();
    handle.write(|sql| {
        sql.execute_batch(
            "INSERT INTO watched_import_folders(path, position) VALUES ('/music', 0);
             INSERT INTO folder_scan_roots(watched_folder_path, generation, status) VALUES ('/music', 1, 'complete');
             INSERT INTO scan_candidate(watched_folder_path, path, generation, kind, name, display_path, file_root, scope, content_hash)
                 VALUES ('/music', '/music/album', 1, 'valid', 'album', 'album', '/music/album', 'direct', 'hash');
             INSERT INTO scan_candidate_file(watched_folder_path, candidate_path, relative_path, position, absolute_path, size, modified_at_ns, file_name, proposed_audio, role, sheet_binding, sheet_disc)
                 VALUES ('/music', '/music/album', 'disc.cue', 0, '/music/album/disc.cue', 100, 1, 'disc.cue', 0, 'track_sheet', 'unresolved', 'ignored');
             INSERT INTO scan_cue_sheet(watched_folder_path, candidate_path, sheet_relative_path) VALUES ('/music', '/music/album', 'disc.cue');
             INSERT INTO scan_cue_track(watched_folder_path, candidate_path, sheet_relative_path, position, number, mode, file_reference, start_cue_frames, pregap_kind)
                 VALUES ('/music', '/music/album', 'disc.cue', 0, 1, 'audio', 'disc.wav', 0, 'none');
             INSERT INTO import_candidate_state(content_hash, folder_path) VALUES ('hash', '/music/album');
             INSERT INTO import_candidate_file_edit(content_hash, relative_path, sheet_binding, sheet_binding_file_id, sheet_disc)
                 VALUES ('hash', 'disc.cue', 'describes', 'disc.flac', 'ignored');"
        )?;
        Ok(())
    }).await.unwrap();
    drop(handle);
    let handle = open(store, "cue-reference", all()).unwrap();
    handle
        .read(|sql| {
            let association: (String, String, String) = sql.query_row(
                "SELECT sheet_id, file_reference, file_id FROM import_candidate_sheet_reference",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            assert_eq!(
                association,
                ("disc.cue".into(), "disc.wav".into(), "disc.flac".into())
            );
            let selection: String = sql.query_row(
                "SELECT sheet_disc FROM import_candidate_file_edit",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(selection, "ignored");
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn applied_source_migration_preserves_the_archived_document() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let mut migrations = all();
    migrations.truncate(39);
    let handle = open(store.clone(), "applied-source", migrations).unwrap();
    handle.write(|sql| {
        sql.execute_batch(
            "INSERT INTO import_candidate_state(content_hash, folder_path) VALUES ('hash', '/music/album');
             INSERT INTO import_candidate_edit(content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode)
                 VALUES ('hash', 'Typed album', '', '', '', '', '', '', '');
             INSERT INTO import_candidate_draft_provenance(content_hash, kind, source, release_id, author)
                 VALUES ('hash', 'external_release', 'musicbrainz', 'release', 'user');
             INSERT INTO source_release_payloads(source, source_release_id, json, fetched_at)
                 VALUES ('musicbrainz', 'release', '{\"id\":\"release\",\"title\":\"Original document\",\"media\":[],\"cover-art-archive\":{\"front\":false,\"darkened\":false}}', '2026-01-01T00:00:00Z');"
        )?;
        Ok(())
    }).await.unwrap();
    drop(handle);
    let handle = open(store, "applied-source", all()).unwrap();
    handle
        .write(|sql| {
            sql.execute("UPDATE source_release_payloads SET json = '{}'", [])?;
            Ok(())
        })
        .await
        .unwrap();
    handle
        .read(|sql| {
            let json: String = sql.query_row(
                "SELECT snapshot FROM import_candidate_applied_source WHERE content_hash = 'hash'",
                [],
                |row| row.get(0),
            )?;
            let applied: crate::import::payloads::AppliedSource =
                serde_json::from_str(&json).unwrap();
            assert_eq!(applied.payloads.release().key, "release");
            assert!(serde_json::to_string(&applied.payloads)
                .unwrap()
                .contains("Original document"));
            Ok(())
        })
        .await
        .unwrap();
}
