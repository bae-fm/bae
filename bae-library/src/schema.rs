//! The library's schema, which a library's store is created, opened and
//! restored on: each crate's parts of it, in crate order.

/// The library's schema: bae-db's part, then preferences', playback's,
/// import's, the transfers' and the library views'.
#[cfg(desktop)]
pub const SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_people::SCHEMA_PARTS,
    bae_db::SCHEMA_PARTS,
    bae_preferences::SCHEMA_PARTS,
    bae_playback::SCHEMA_PARTS,
    bae_import::SCHEMA_PARTS,
    bae_transfers::SCHEMA_PARTS,
    bae_views::SCHEMA_PARTS,
]);

/// The library's schema: bae-db's part, then preferences', playback's, the
/// transfers' and the library views'. A phone's library holds no import tables, since they never sync and
/// nothing on a phone reads them.
#[cfg(not(desktop))]
pub const SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_people::SCHEMA_PARTS,
    bae_db::SCHEMA_PARTS,
    bae_preferences::SCHEMA_PARTS,
    bae_playback::SCHEMA_PARTS,
    bae_transfers::SCHEMA_PARTS,
    bae_views::SCHEMA_PARTS,
]);

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
