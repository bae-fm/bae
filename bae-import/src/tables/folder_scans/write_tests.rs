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
    let mut referencing = bae_store::test_fixtures::schema_checks::tables_referencing(
        crate::TEST_SCHEMA,
        "scan_candidate",
    );
    referencing.sort();
    let mut cleared: Vec<String> = super::ENTRY_ROW_CLEARS
        .iter()
        .map(|delete| {
            let mut words = delete.split_whitespace().skip_while(|word| *word != "FROM");
            words.nth(1).expect("a delete names its table")
        })
        .chain(KEPT_ACROSS_REWRITE)
        .map(str::to_string)
        .collect();
    cleared.sort();
    assert_eq!(cleared, referencing);
}
