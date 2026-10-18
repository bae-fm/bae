use super::{exec, exec_batch, temp_db, ARTIST_A, ARTIST_B};

#[tokio::test]
async fn deleting_an_artist_removes_local_draft_references() {
    for removed in [ARTIST_A, ARTIST_B] {
        let (db, _tmp) = temp_db().await;
        for artist in [ARTIST_A, ARTIST_B] {
            exec(
                &db,
                "INSERT INTO artists (id, name, name_key, created_at) VALUES (?, 'Artist Name', 'artist name', '2026-01-01T00:00:00Z')",
                &[artist],
            )
            .await;
        }
        exec_batch(
            &db,
            "INSERT INTO import_candidate_state (content_hash, folder_path)
             VALUES ('candidate', '/music/candidate');
             INSERT INTO import_candidate_edit
                 (content_hash, album_title, album_year, year, barcode, author, draft_blank, draft_valid)
             VALUES ('candidate', 'Album Title', '', '', '', 'person', 0, 1);
             INSERT INTO import_candidate_track
                 (content_hash, track_id, position, title, artist_assignment_kind,
                  track_number, file_kind, file_id)
             VALUES ('candidate', 'track', 0, 'Track Title', 'explicit', 1, 'standalone', 'file');
             INSERT INTO import_candidate_failure (content_hash, error, failed_at)
             VALUES ('candidate', 'Artist identity conflict', '2026-01-01T00:00:00Z');",
        )
        .await;
        for (position, artist) in [("0", ARTIST_A), ("1", ARTIST_B)] {
            exec(
                &db,
                "INSERT INTO import_candidate_album_artist_assignment
                     (content_hash, position, assignment_kind, artist_id)
                 VALUES ('candidate', ?, 'picked', ?)",
                &[position, artist],
            )
            .await;
            exec(
                &db,
                "INSERT INTO import_candidate_track_artist_assignment
                     (content_hash, track_id, position, assignment_kind, artist_id)
                 VALUES ('candidate', 'track', ?, 'picked', ?)",
                &[position, artist],
            )
            .await;
        }
        exec(
            &db,
            "INSERT INTO import_candidate_artist_identity_conflict
                 (content_hash, incoming_artist_name, discogs_artist_id, musicbrainz_artist_id,
                  discogs_library_artist_id, musicbrainz_library_artist_id)
             VALUES ('candidate', 'Artist Name', 'discogs', 'musicbrainz', ?, ?)",
            &[ARTIST_A, ARTIST_B],
        )
        .await;

        exec(&db, "DELETE FROM artists WHERE id = ?", &[removed]).await;

        let remaining = db
            .read(|sql| {
                Ok((
                    sql.query_row(
                        "SELECT count(*) FROM import_candidate_album_artist_assignment",
                        [],
                        |row| row.get::<_, i64>(0),
                    )?,
                    sql.query_row(
                        "SELECT count(*) FROM import_candidate_track_artist_assignment",
                        [],
                        |row| row.get::<_, i64>(0),
                    )?,
                    sql.query_row(
                        "SELECT count(*) FROM import_candidate_artist_identity_conflict",
                        [],
                        |row| row.get::<_, i64>(0),
                    )?,
                    sql.query_row("SELECT count(*) FROM import_candidate_edit", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                ))
            })
            .await
            .unwrap();
        assert_eq!(remaining, (1, 1, 0, 1));
    }
}
