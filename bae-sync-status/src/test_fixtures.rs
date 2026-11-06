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
