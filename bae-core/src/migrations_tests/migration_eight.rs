use super::*;

#[tokio::test]
#[serial]
async fn migration_eight_preserves_pressing_year_and_adds_blank_album_year() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(store_dir.clone(), "migration-album-year", version_seven())
        .expect("open version seven");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('candidate-hash', '/music/release');
                 INSERT INTO import_candidate_edit (
                     content_hash, album_title, year, format, label,
                     catalog_number, country, barcode
                 ) VALUES (
                     'candidate-hash', 'Album Title', '2004', '', '', '', '', ''
                 );",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-seven draft");
    drop(handle);

    let handle =
        open(store_dir, "migration-album-year", version_eight()).expect("migrate to version eight");
    handle
        .read(|sql| {
            let values: (String, String) = sql.query_row(
                "SELECT album_year, year FROM import_candidate_edit WHERE content_hash = 'candidate-hash'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(values, (String::new(), "2004".to_string()));
            Ok(())
        })
        .await
        .expect("read migrated draft");
}
