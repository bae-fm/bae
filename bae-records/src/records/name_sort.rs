//! The language this device orders names in: the `name_sort` table
//! (`records/name_sort.sql`), its write, and the fragment a read takes the
//! language from to order a name by.
//!
//! The lists that keep a sort key beside each name write those keys again
//! with their own triggers on `name_sort`, in the write that changes it.

use bae_store::DbError;
use bae_store::WriteStatements as _;
use bae_util::name_key::{InvalidLanguage, NameKeys, DATA_VERSION};

/// A language bae cannot order the device's names in.
#[derive(Debug, thiserror::Error)]
pub enum SortLanguageError {
    /// The tag is not a language tag.
    #[error(transparent)]
    Invalid(#[from] InvalidLanguage),
    #[error(transparent)]
    Database(#[from] DbError),
}

// The language this device orders names in, as the one `name_sort` row holds
// it.
coven::fragment!(
    NameSortLanguage,
    r#"
    SELECT sort.language
    FROM name_sort AS sort
"#
);

// The version of the collation data this device's name keys were written
// with, which other domains' tests compare their keys' version to.
#[cfg(any(test, feature = "test-utils"))]
coven::fragment!(
    NameSortDataVersion,
    r#"
    SELECT sort.data_version
    FROM name_sort AS sort
"#
);

// The language this device orders names in, and the collation data its keys
// were written with.
coven::read!(
    writer NameSortState { data_version: u32 },
    r#"
    SELECT sort.language, sort.data_version
    FROM name_sort AS sort
"#
);

coven::write!(
    SetNameSort(language: &'p str, data_version: u32),
    "UPDATE name_sort SET language = :language, data_version = :data_version"
);

#[cfg(any(test, feature = "test-utils"))]
coven::write!(AgeNameKeys(data_version: u32), "UPDATE name_sort SET data_version = :data_version");

impl super::Records {
    /// Order this device's names in the language `tag`: the lists that keep
    /// name keys write every key again in it, in one write that commits whole
    /// or not at all. A device already ordering in `tag` with keys of the
    /// current collation data writes nothing, so this is safe to call at
    /// every start, and to call again after it failed.
    pub async fn set_sort_language(&self, tag: &str) -> Result<(), SortLanguageError> {
        let language = NameKeys::for_language(tag)?.tag().to_string();
        Ok(self.write_sort_language(Some(language)).await?)
    }

    /// Write every stored name key again, in the language the device already
    /// orders in, when an earlier build wrote them with other collation data
    /// than this build's. bae calls this as a library opens, before anything
    /// reads or writes it, so no row has a key from different data than the
    /// rest.
    pub async fn settle_name_keys(&self) -> Result<(), DbError> {
        self.write_sort_language(None).await
    }

    /// Order in `language` (the stored one when `None`) with this build's
    /// collation data, unless the device already does.
    async fn write_sort_language(&self, language: Option<String>) -> Result<(), DbError> {
        self.store
            .call_bulk(move |sql| {
                let NameSortStateRow { language: stored, data_version: version } =
                    bae_store::statement_row(sql, &NameSortStateParams {})?;
                let language = language.unwrap_or_else(|| stored.clone());
                if stored == language && version == DATA_VERSION {
                    return Ok(());
                }
                sql.run(&SetNameSortParams { language: &language, data_version: DATA_VERSION })?;
                Ok(())
            })
            .await
    }

    /// Record that an earlier build wrote the stored name keys with other
    /// collation data than this build's.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn age_name_keys_for_test(&self) -> Result<(), DbError> {
        self.store
            .call_bulk(|sql| {
                sql.run(&AgeNameKeysParams { data_version: DATA_VERSION + 1 })?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "name_sort_tests.rs"]
mod tests;
