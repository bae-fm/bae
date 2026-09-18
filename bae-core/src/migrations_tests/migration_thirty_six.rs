use super::*;

#[tokio::test]
#[serial]
async fn stored_readings_survive_without_inventing_per_value_proof() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let mut migrations = all();
    migrations.truncate(35);
    let handle = open(store_dir.clone(), "mark-corroboration", migrations).unwrap();
    handle.write(|sql| {
        sql.execute_batch(
            "INSERT INTO artists (id, name, _updated_at, created_at)
                 VALUES ('aaaaaaaa-0000-4000-8000-000000000001', 'Artist Name', 'h1', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at)
                 VALUES ('bbbbbbbb-0000-4000-8000-000000000001', 'Album Title',
                     'aaaaaaaa-0000-4000-8000-000000000001', 0, 'h1', '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, remote, identified_by, _updated_at, created_at)
                 VALUES ('cccccccc-0000-4000-8000-000000000001',
                     'bbbbbbbb-0000-4000-8000-000000000001', 0, 'barcode', 'h1', '2026-01-01T00:00:00Z');
             INSERT INTO release_marks (id, release_id, position, kind, value, origin, origin_path, _updated_at, created_at)
                 VALUES ('dddddddd-0000-4000-8000-000000000001',
                     'cccccccc-0000-4000-8000-000000000001', 0, 'barcode', '1234567890123', 'artwork', 'back.jpg',
                     'h1', '2026-01-01T00:00:00Z');"
        )?;
        Ok(())
    }).await.unwrap();
    drop(handle);
    let handle = open(store_dir, "mark-corroboration", all()).unwrap();
    handle
        .read(|sql| {
            let mark: (String, String, bool) = sql.query_row(
                "SELECT value, origin_path, corroborated FROM release_marks",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            assert_eq!(mark, ("1234567890123".into(), "back.jpg".into(), false));
            let identified: String =
                sql.query_row("SELECT identified_by FROM releases", [], |row| row.get(0))?;
            assert_eq!(
                identified, "barcode",
                "the known aggregate match remains recorded"
            );
            Ok(())
        })
        .await
        .unwrap();
}
