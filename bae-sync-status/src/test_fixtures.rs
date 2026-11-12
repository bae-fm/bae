//! Test stores on a schema, for the tests of this crate and every crate that
//! builds on it.

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, people's and membership's, and the sync status's.
pub const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_people::SCHEMA_PARTS,
    crate::SCHEMA_PARTS,
]);

/// The sync status over `store`.
pub fn test_sync_status(store: &bae_store::Store) -> crate::SyncStatus {
    crate::SyncStatus::new(store.clone())
}

/// Plant damage for a test: the `coven_operations` view, which lists the
/// storage setups, is gone from the library stored under `directory`, as a damaged
/// file would lack it, so every read and write of the list fails.
pub fn lose_operations_view_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "coven_operations",
        "DROP VIEW coven_operations;",
    );
}
