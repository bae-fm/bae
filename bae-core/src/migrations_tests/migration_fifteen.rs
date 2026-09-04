use super::*;

#[tokio::test]
#[serial]
async fn stores_the_audio_a_bound_sheet_describes() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-sheet-audio-file",
        version_fourteen(),
    )
    .expect("open version fourteen");
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
            Ok(())
        })
        .await
        .expect("seed a cached scan whose bound sheet names no audio rows");
    drop(handle);

    let handle = open(store_dir, "migration-sheet-audio-file", all())
        .expect("migrate the sheet audio pairing");
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
                 );
                 INSERT INTO scan_candidate_file (
                     watched_folder_path, candidate_path, relative_path, position,
                     absolute_path, size, modified_at_ns, audio_content_type,
                     audio_duration_ms, audio_sample_rate_hz, audio_bits_per_sample,
                     audio_channels, file_name, proposed_audio, role
                 ) VALUES (
                     '/music', '/music/release', 'disc.flac', 1,
                     '/music/release/disc.flac', 1000, 1, 'audio/flac',
                     60000, 44100, 16, 2, 'disc.flac', 1, 'audio'
                 );
                 INSERT INTO scan_sheet_audio_file (
                     watched_folder_path, candidate_path, sheet_relative_path, position,
                     file_reference, audio_relative_path
                 ) VALUES (
                     '/music', '/music/release', 'disc.cue', 0, 'disc.wav', 'disc.flac'
                 );",
            )?;
            let violations = sql.query("PRAGMA foreign_key_check", [], |row| {
                row.get::<_, String>(0)
            })?;
            assert!(violations.is_empty());

            let named_absent_audio = sql.execute(
                "INSERT INTO scan_sheet_audio_file (
                     watched_folder_path, candidate_path, sheet_relative_path, position,
                     file_reference, audio_relative_path
                 ) VALUES (
                     '/music', '/music/release', 'disc.cue', 1, 'other.wav', 'missing.flac'
                 )",
                [],
            );
            assert!(
                named_absent_audio.is_err(),
                "a pairing must name a file of the same candidate"
            );

            sql.execute(
                "DELETE FROM scan_candidate_file \
                 WHERE watched_folder_path = '/music' AND candidate_path = '/music/release' \
                   AND relative_path = 'disc.flac'",
                [],
            )?;
            let pairings: i64 =
                sql.query_row("SELECT COUNT(*) FROM scan_sheet_audio_file", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(pairings, 0, "a pairing goes with the audio it names");
            Ok(())
        })
        .await
        .expect("write the stored pairing shape");
}
