//! These tests check how the release sheet decisions part declares its
//! tables. Both tables sync:
//! `release_sheet_discs` is keyed by the sheet file, and
//! `release_sheet_bindings` by the sheet file and the `FILE` reference.

use std::collections::BTreeSet;

/// The part's SCHEMA declares both of its tables synced, each with a shared
/// key on its primary-key columns, and declares every table the part creates.
#[test]
fn the_sheet_decision_tables_are_the_part_s_synced_tables() {
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
    assert_eq!(created, BTreeSet::from(["release_sheet_bindings", "release_sheet_discs"]));
    assert_eq!(declared, created);
    for table in &tables {
        assert_eq!(table.identity(), &coven::RowIdentity::SharedKey);
        assert!(table.file_columns().is_empty());
        let keys: &[&str] = match table.name() {
            "release_sheet_bindings" => &["sheet_file_id", "file_reference"],
            _ => &["sheet_file_id"],
        };
        assert_eq!(table.get_key_columns(), keys);
    }
}
