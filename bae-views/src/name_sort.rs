//! The language this device orders names in, and the rewrite of every stored
//! name key when it, or the collation data the keys are written with, changes.
//!
//! Its table is `name_sort.sql`; the keys are the columns the listings'
//! triggers write (`album/listing.sql`, `artist.sql`, `storage.sql`).

use super::*;
use bae_util::name_key::{InvalidLanguage, NameKeys, DATA_VERSION};

/// The SQL creating this part's table.
pub(crate) const SQL: &str = include_str!("name_sort.sql");

/// A language the device's names could not be ordered in.
#[derive(Debug, thiserror::Error)]
pub enum SortLanguageError {
    /// The tag is not a language tag.
    #[error(transparent)]
    Invalid(#[from] InvalidLanguage),
    #[error(transparent)]
    Database(#[from] DbError),
}

/// Every listing's key columns written again from the rows they come from,
/// in the language of `name_sort`: the albums are listed again, which writes
/// their links' and releases' keys through the triggers that copy them, and
/// the artists' and composers' keys are written from their names, and the
/// composers' pages' rows are listed again.
const REKEY: &[&str] = &[
    "INSERT INTO album_listing_refresh (album_id) SELECT album_id FROM album_listing",
    "UPDATE artist_listing SET \
         name_sort_key = bae_name_key(artists.name, (SELECT language FROM name_sort)), \
         sort_name_sort_key = bae_name_key(COALESCE(artists.sort_name, artists.name), \
                                      (SELECT language FROM name_sort)) \
     FROM artists WHERE artists.id = artist_listing.artist_id",
    "UPDATE composer_listing SET \
         name_sort_key = bae_name_key(artists.name, (SELECT language FROM name_sort)) \
     FROM artists WHERE artists.id = composer_listing.artist_id",
    "INSERT INTO composer_detail_work_refresh (artist_id, work_id) \
     SELECT DISTINCT artist_id, item_id FROM composer_detail_row WHERE section = 0",
    "INSERT INTO composer_detail_credit_refresh (credit_id) \
     SELECT DISTINCT item_id FROM composer_detail_row WHERE section <> 0",
];

/// The sort key of the name `name` (a SQL expression) in the device's
/// language, for a list too small to keep keys beside its names: it orders
/// by the same key the stored ones are.
pub(crate) fn sort_key_sql(name: &str) -> String {
    format!("bae_name_key({name}, (SELECT language FROM name_sort))")
}

impl LibraryViews {
    /// Order this device's names in the language `tag`: every stored name key
    /// is written again in it, and the lists' versions advance with the keys,
    /// in one write that commits whole or not at all. A device already
    /// ordering in `tag` with keys of the current collation data writes
    /// nothing, so this is safe to call at every start, and to call again
    /// after it failed.
    pub async fn set_sort_language(&self, tag: &str) -> Result<(), SortLanguageError> {
        let language = NameKeys::for_language(tag)?.tag().to_string();
        Ok(self.write_sort_language(Some(language)).await?)
    }

    /// Write every stored name key again, in the language the device already
    /// orders in, when they were written with other collation data than this
    /// build's. A library opens through this before anything reads or writes
    /// it, so no row is ever keyed by data different from the rest.
    pub async fn settle_name_keys(&self) -> Result<(), DbError> {
        self.write_sort_language(None).await
    }

    /// Order in `language` (the stored one when `None`), writing the keys
    /// again unless they already stand in it with this build's data.
    async fn write_sort_language(&self, language: Option<String>) -> Result<(), DbError> {
        self.store
            .call_bulk(move |sql| {
                let (stored, version): (String, u32) =
                    sql.query_row("SELECT language, data_version FROM name_sort", [], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?;
                let language = language.unwrap_or_else(|| stored.clone());
                if stored == language && version == DATA_VERSION {
                    return Ok(());
                }
                sql.execute(
                    "UPDATE name_sort SET language = ?1, data_version = ?2",
                    params![language, DATA_VERSION],
                )?;
                for statement in REKEY {
                    sql.execute(statement, [])?;
                }
                Ok(())
            })
            .await
    }

    /// The language this device orders names in: `und`, the root order, until
    /// it is set.
    pub async fn sort_language(&self) -> Result<String, DbError> {
        self.store
            .read(|sql| Ok(sql.query_row("SELECT language FROM name_sort", [], |row| row.get(0))?))
            .await
    }
}

#[cfg(test)]
#[path = "name_sort_tests.rs"]
mod tests;
