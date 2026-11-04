//! The library's schema, which a library's store is created, opened and
//! restored on: each crate's parts of it, in crate order.

/// The library's schema: bae-db's part, then import's.
pub const SCHEMA: bae_db::Schema =
    bae_db::Schema::new(&[bae_db::SCHEMA_PARTS, bae_import::SCHEMA_PARTS]);

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
