/// The tables that hang off a `scan_candidate` row by foreign key and that an
/// entry written again keeps: the copies of its pictures are of files at the
/// version the scan read, so a rescan that finds the same file reads no copy
/// again, and one that finds a replaced file asks for a copy of its new
/// version beside the old. They go with the candidate's row.
const KEPT_ACROSS_REWRITE: [&str; 1] = ["import_picture_copy"];

/// An entry written again clears every table that hangs off its row, except
/// the ones that are kept on purpose: the tables naming `scan_candidate` in a
/// foreign key are exactly those two lists.
#[test]
fn an_entry_written_again_clears_every_table_below_its_row() {
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    crate::TEST_SCHEMA.open_bare_library(&conn);
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
    let mut cleared: Vec<String> = super::ENTRY_ROW_TABLES
        .iter()
        .chain(&KEPT_ACROSS_REWRITE)
        .map(|table| table.to_string())
        .collect();
    cleared.sort();
    assert_eq!(cleared, referencing);
}
