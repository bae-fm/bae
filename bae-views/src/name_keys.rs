//! The rewrite of every stored name key when the device's language, or the
//! collation data the keys are written with, changes: a trigger on
//! bae-records' `name_sort` (`name_keys.sql`) over the key columns the
//! listings' triggers write (`album/listing.sql`, `artist.sql`,
//! `composer.sql`, `composer_detail.sql`, `storage.sql`).

/// The SQL creating this part's trigger.
pub(crate) const SQL: &str = include_str!("name_keys.sql");

#[cfg(test)]
#[path = "name_keys_tests.rs"]
mod tests;
