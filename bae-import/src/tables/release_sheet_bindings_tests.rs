//! The release sheet bindings' declaration: the table syncs, keyed by its
//! sheet file and `FILE` reference.

use std::collections::BTreeSet;

/// The part's one table is declared synced, as a shared key over its primary
/// key, and no table the part creates is left undeclared.
#[test]
fn the_sheet_bindings_table_is_the_part_s_one_synced_table() {
    let create =
        regex::Regex::new(r"(?m)^\s*CREATE (?:VIRTUAL )?TABLE (?:IF NOT EXISTS )?(\w+)").unwrap();
    let schema = bae_store::Schema::new(&[&[super::SCHEMA]]);
    let created: BTreeSet<&str> = schema
        .sql()
        .flat_map(|sql| create.captures_iter(sql))
        .map(|captures| captures.get(1).unwrap().as_str())
        .collect();
    let tables = schema.synced_tables();
    let declared: BTreeSet<&str> = tables.iter().map(coven::SyncedTable::name).collect();
    assert_eq!(created, BTreeSet::from(["release_sheet_bindings"]));
    assert_eq!(declared, created);
    let table = &tables[0];
    assert_eq!(table.identity(), &coven::RowIdentity::SharedKey);
    assert_eq!(table.get_key_columns(), ["sheet_file_id", "file_reference"]);
    assert!(table.file_columns().is_empty());
}
