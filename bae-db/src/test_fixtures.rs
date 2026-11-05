//! Test databases on a schema, for the tests of this crate and every crate
//! that builds on it.

/// The schema this crate's tests open their stores on, and the tests of the
/// crates above it that hold no part of the schema: records', release
/// storage's and bae-db's parts.
pub const TEST_SCHEMA: bae_store::Schema =
    bae_store::Schema::new(&[bae_records::SCHEMA_PARTS, crate::SCHEMA_PARTS]);

/// The database over `store`.
pub fn test_database(store: &bae_store::Store) -> crate::Database {
    crate::Database::new(store.clone())
}
