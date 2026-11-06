/// An entry written again clears every table that hangs off its row: the
/// tables naming `scan_candidate` in a foreign key are exactly those.
#[test]
fn an_entry_written_again_clears_every_table_below_its_row() {
    let conn = crate::tables::tests::bare_library();
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
