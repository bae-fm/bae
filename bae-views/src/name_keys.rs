//! The trigger on bae-records' `name_sort` (`name_keys.sql`) that writes
//! every stored name key again when the device's language, or the version of
//! the collation data bae writes the keys with, changes. The keys are the
//! columns the listings' triggers write (`album/listing.sql`, `artist.sql`,
//! `composer.sql`, `composer_detail.sql`, `storage.sql`).

/// The SQL creating this part's trigger.
pub(crate) const SQL: &str = include_str!("name_keys.sql");

#[cfg(test)]
#[path = "name_keys_tests.rs"]
mod tests;
