//! Test stores on a schema, for the tests of this crate and every crate that
//! builds on it.

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, people's and membership's.
pub const TEST_SCHEMA: bae_store::Schema =
    bae_store::Schema::new(&[bae_records::SCHEMA_PARTS, crate::SCHEMA_PARTS]);
