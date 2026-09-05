use super::*;

/// The pane's session table hangs off the candidate's state row: a session
/// for a candidate with no state row is refused, and one whose candidate is
/// forgotten goes with it.
#[tokio::test]
#[serial]
async fn candidate_session_rows_follow_their_candidate() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir, "migration-candidate-session", all())
        .expect("migrate to the session table");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('candidate-hash', '/music/release');
                 INSERT INTO import_candidate_session (
                     content_hash, presentation, search_tab, search_artist,
                     search_album, search_catalog, search_barcode, error
                 ) VALUES (
                     'candidate-hash', 'find_online', 'general', 'Artist', '', '', '', NULL
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed a candidate and its session");

    let orphan = handle
        .write(|sql| {
            sql.execute(
                "INSERT INTO import_candidate_session (
                     content_hash, presentation, search_tab, search_artist,
                     search_album, search_catalog, search_barcode, error
                 ) VALUES ('no-such-candidate', 'draft', 'general', '', '', '', '', NULL)",
                [],
            )?;
            Ok(())
        })
        .await;
    assert!(orphan.is_err(), "a session needs a candidate state row");

    handle
        .write(|sql| {
            sql.execute(
                "DELETE FROM import_candidate_state WHERE content_hash = 'candidate-hash'",
                [],
            )?;
            Ok(())
        })
        .await
        .expect("forget the candidate");
    let remaining: i64 = handle
        .read(|sql| {
            sql.query_row("SELECT COUNT(*) FROM import_candidate_session", [], |row| {
                row.get(0)
            })
            .map_err(Into::into)
        })
        .await
        .expect("count sessions");
    assert_eq!(remaining, 0, "the session goes with its candidate");
}
