//! The release sheet decisions' declaration: both tables sync, keyed by their
//! sheet file (and `FILE` reference).

use std::collections::BTreeSet;

/// The part's two tables are declared synced, each a shared key over its
/// primary key, and no table the part creates is left undeclared.
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
