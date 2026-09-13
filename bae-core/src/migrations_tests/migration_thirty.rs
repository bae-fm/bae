use super::*;

/// Moving the barcode decision off the header row rebuilds the lookup-choices
/// table. The chosen and the struck-out catalog numbers hang off it by a
/// cascading key, so the rebuild has to carry them across — and a stored "all
/// barcodes out" named no codes, so it cannot be carried and is dropped.
#[tokio::test]
#[serial]
async fn the_lookup_choices_rebuild_keeps_the_catalog_numbers_hanging_off_it() {
    let temp = tempfile::tempdir().expect("temp store");
    let store_dir = StoreDir::new_ephemeral(temp.path());
    let handle = open(
        store_dir.clone(),
        "migration-excluded-barcode",
        version_twenty_nine(),
    )
    .expect("open version twenty-nine");
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('kept-hash', '/music/Kept'), ('out-hash', '/music/Out');
                 INSERT INTO import_candidate_lookup_choices
                     (content_hash, disc_id_excluded, barcode_excluded)
                     VALUES ('kept-hash', 1, 0), ('out-hash', 0, 1);
                 INSERT INTO import_candidate_chosen_catalog
                     (content_hash, position, value)
                     VALUES ('kept-hash', 0, 'LBL 002'), ('kept-hash', 1, 'LBL 001');
                 INSERT INTO import_candidate_discounted_catalog (content_hash, value)
                     VALUES ('kept-hash', 'LBL 100');",
            )?;
            Ok(())
        })
        .await
        .expect("seed version-twenty-nine lookup choices");
    drop(handle);

    let handle = open(store_dir, "migration-excluded-barcode", all())
        .expect("migrate the barcode decision off the header row");
    handle
        .read(|sql| {
            let columns = sql.query(
                "SELECT name FROM pragma_table_info('import_candidate_lookup_choices') \
                 ORDER BY name",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert_eq!(
                columns,
                vec!["content_hash".to_string(), "disc_id_excluded".to_string(),],
                "the barcode flag is gone and the disc-ID one stays"
            );

            let disc_id_excluded = sql.query(
                "SELECT content_hash, disc_id_excluded FROM import_candidate_lookup_choices \
                 ORDER BY content_hash",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )?;
            assert_eq!(
                disc_id_excluded,
                vec![("kept-hash".to_string(), 1), ("out-hash".to_string(), 0)],
                "every candidate's disc-ID decision survives the rebuild"
            );

            let chosen = sql.query(
                "SELECT value FROM import_candidate_chosen_catalog \
                 WHERE content_hash = 'kept-hash' ORDER BY position",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert_eq!(
                chosen,
                vec!["LBL 002".to_string(), "LBL 001".to_string()],
                "the chosen numbers keep the order they were chosen in"
            );

            let struck_out = sql.query(
                "SELECT value FROM import_candidate_discounted_catalog \
                 WHERE content_hash = 'kept-hash'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            assert_eq!(struck_out, vec!["LBL 100".to_string()]);

            let left_out: i64 = sql.query_row(
                "SELECT COUNT(*) FROM import_candidate_excluded_barcode",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(
                left_out, 0,
                "a flag that named no code cannot name one now, so nothing is left out"
            );
            Ok(())
        })
        .await
        .expect("read the rebuilt lookup choices");
}
