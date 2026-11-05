//! Where each release's files are on this device: the `release_storage`
//! table, which triggers keep from the release's files, coven's records of
//! them, pins and the upload queue.

/// Release storage's part of the schema: `release_storage.sql`.
pub const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("release_storage.sql")]);

#[cfg(test)]
#[path = "release_storage_tests.rs"]
mod tests;
