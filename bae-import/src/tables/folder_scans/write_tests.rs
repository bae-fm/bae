use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

/// An entry written again clears every table that hangs off its row: the
/// tables naming `scan_candidate` in a foreign key are exactly those.
#[test]
fn an_entry_written_again_clears_every_table_below_its_row() {
    let conn = bae_db::schema::bare_library_connection();
    let mut referencing: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT m.name FROM sqlite_schema AS m, pragma_foreign_key_list(m.name) AS f \
             WHERE m.type = 'table' AND f.\"table\" = 'scan_candidate'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    referencing.sort();
    let mut cleared: Vec<String> =
        super::ENTRY_ROW_TABLES.iter().map(|table| table.to_string()).collect();
    cleared.sort();
    assert_eq!(cleared, referencing);
}

/// A candidate discovery stored with a blank draft: nobody wrote it.
#[tokio::test]
async fn a_candidate_with_no_pick_has_no_author() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").metadata_author,
        crate::MetadataAuthor::Nobody
    );
}
