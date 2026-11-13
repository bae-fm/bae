use super::*;
use coven::Binding;
use std::collections::BTreeSet;

#[test]
fn people_declarations_match_the_store_audience_and_file_contract() {
    let tables = synced_tables();
    let names: BTreeSet<_> = tables.iter().map(SyncedTable::name).collect();
    assert_eq!(
        names,
        BTreeSet::from(["people", "history", "ratings", "playlists", "playlist_entries"])
    );
    for table in &tables {
        let shared = matches!(table.name(), "people" | "ratings");
        assert_eq!(
            table.identity(),
            if shared { &RowIdentity::SharedKey } else { &RowIdentity::IndependentUuid }
        );
        let keys: &[&str] = match table.name() {
            "playlist_entries" => &["playlist_id", "position_id"],
            _ => &["id"],
        };
        assert_eq!(table.get_key_columns(), keys);
        assert_eq!(
            table.get_audience_column(),
            matches!(table.name(), "playlists" | "history").then_some("audience")
        );
        assert_eq!(
            table.get_audience_from(),
            (table.name() == "playlist_entries").then_some("playlist_id")
        );
        assert!(table.is_owner_only(), "{}", table.name());
        let columns: Vec<(&str, &Binding, bool)> = table
            .file_columns()
            .iter()
            .map(|column| (column.column(), column.binding(), column.is_write_once()))
            .collect();
        let bound = Binding::Bound { limit: coven::BOUND_FILE_LIMIT };
        let expected: &[(&str, &Binding, bool)] = match table.name() {
            "people" => &[("avatar_id", &bound, false)],
            _ => &[],
        };
        assert_eq!(columns, expected, "{}", table.name());
    }
    let declared = namespaces();
    let namespaces: Vec<(&str, &Provenance, u64)> = declared
        .iter()
        .map(|namespace| (namespace.name(), namespace.kind(), namespace.cache_budget()))
        .collect();
    assert_eq!(namespaces, [(AVATARS_NAMESPACE, &Provenance::AppProvided, 64 * 1024 * 1024)]);
}

/// Tables of this crate's part of the schema whose rows never leave this
/// device. A table the part creates is either declared in `synced_tables()` or
/// named here, so a new table is a decision rather than a row that silently
/// stays on one device.
const DEVICE_LOCAL_TABLES: &[&str] = &[
    // This device's own copies: plays before a person exists.
    "local_history",
    // The profile this device's person asked for, while their private
    // circle is being made.
    "profile_request",
    // The name the library's creator typed, kept until they are a member and
    // a write asks for their profile under it.
    "creator_name",
    // What each admin change this device asked for changes, while coven
    // lists its operation.
    "admin_operations",
];

#[test]
fn every_schema_table_is_synced_or_declared_device_local() {
    let create =
        regex::Regex::new(r"(?m)^\s*CREATE (?:VIRTUAL )?TABLE (?:IF NOT EXISTS )?(\w+)").unwrap();
    let schema: BTreeSet<&str> = bae_store::Schema::new(&[crate::SCHEMA_PARTS])
        .sql()
        .flat_map(|sql| create.captures_iter(sql))
        .map(|captures| captures.get(1).unwrap().as_str())
        .collect();
    let tables = bae_store::Schema::new(&[crate::SCHEMA_PARTS]).synced_tables();
    let synced: BTreeSet<&str> = tables.iter().map(SyncedTable::name).collect();
    let local: BTreeSet<&str> = DEVICE_LOCAL_TABLES.iter().copied().collect();
    assert_eq!(local.len(), DEVICE_LOCAL_TABLES.len(), "a device-local table is listed twice");
    let undeclared: Vec<_> =
        schema.iter().filter(|table| !synced.contains(*table) && !local.contains(*table)).collect();
    assert!(
        undeclared.is_empty(),
        "tables neither synced nor listed as device-local: {undeclared:?}"
    );
    let both: Vec<_> = synced.intersection(&local).collect();
    assert!(both.is_empty(), "tables both synced and device-local: {both:?}");
    let missing: Vec<_> = synced.union(&local).filter(|table| !schema.contains(*table)).collect();
    assert!(missing.is_empty(), "declared tables the schema doesn't create: {missing:?}");
}
