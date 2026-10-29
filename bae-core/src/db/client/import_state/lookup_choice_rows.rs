//! What a person decided one candidate's identification asks about, as one
//! header row plus the left-out barcodes and the chosen and struck-out catalog
//! numbers hanging off it.
//!
//! Written whole: the caller hands over the next value of all of it, and what
//! the new value does not name is gone. A candidate with no header row has
//! decided nothing, which reads back as [`LookupChoices::default`].

use super::*;
use crate::import::{LookupChoices, SearchWords};

/// A candidate's lookup choices as columns of a read that joins its header
/// row as `choices`, its lists riding on it as JSON arrays: the chosen
/// catalog numbers in their order, the struck-out ones and the left-out
/// barcodes by value. A candidate with no header row reads `choices_row` as
/// NULL.
pub(super) const LOOKUP_CHOICES_COLUMNS: &str = "choices.content_hash AS choices_row, \
     choices.disc_id_excluded AS choices_disc_id_excluded, \
     choices.search_album AS choices_search_album, \
     choices.search_artist AS choices_search_artist, \
     (SELECT json_group_array(chosen.value ORDER BY chosen.position) \
      FROM import_candidate_chosen_catalog AS chosen \
      WHERE chosen.content_hash = choices.content_hash) AS choices_chosen_catalogs, \
     (SELECT json_group_array(struck.value ORDER BY struck.value) \
      FROM import_candidate_discounted_catalog AS struck \
      WHERE struck.content_hash = choices.content_hash) AS choices_discounted_catalogs, \
     (SELECT json_group_array(left_out.value ORDER BY left_out.value) \
      FROM import_candidate_excluded_barcode AS left_out \
      WHERE left_out.content_hash = choices.content_hash) AS choices_excluded_barcodes";

/// One candidate's [`LOOKUP_CHOICES_COLUMNS`] as SQLite hands them over,
/// its lists still JSON.
pub(super) struct StoredLookupChoices {
    disc_id_excluded: bool,
    search_words: Option<SearchWords>,
    chosen_catalogs: String,
    discounted_catalogs: String,
    excluded_barcodes: String,
}

/// The [`LOOKUP_CHOICES_COLUMNS`] of `row`; none for a candidate that has
/// decided nothing.
pub(super) fn read_lookup_choices(row: &Row<'_>) -> rusqlite::Result<Option<StoredLookupChoices>> {
    if row.get::<_, Option<String>>("choices_row")?.is_none() {
        return Ok(None);
    }
    let search_words = match (
        row.get::<_, Option<String>>("choices_search_album")?,
        row.get::<_, Option<String>>("choices_search_artist")?,
    ) {
        (Some(album), Some(artist)) => Some(SearchWords { album, artist }),
        _ => None,
    };
    Ok(Some(StoredLookupChoices {
        disc_id_excluded: row.get("choices_disc_id_excluded")?,
        search_words,
        chosen_catalogs: row.get("choices_chosen_catalogs")?,
        discounted_catalogs: row.get("choices_discounted_catalogs")?,
        excluded_barcodes: row.get("choices_excluded_barcodes")?,
    }))
}

impl StoredLookupChoices {
    pub(super) fn choices(self) -> Result<LookupChoices, DbError> {
        let list = |json: &str, what: &str| {
            serde_json::from_str::<Vec<String>>(json)
                .map_err(|error| DbError::Message(format!("stored {what} {json:?}: {error}")))
        };
        Ok(LookupChoices {
            disc_id_excluded: self.disc_id_excluded,
            excluded_barcodes: list(&self.excluded_barcodes, "left-out barcodes")?,
            chosen_catalogs: list(&self.chosen_catalogs, "chosen catalog numbers")?,
            search_words: self.search_words,
            discounted_catalogs: list(&self.discounted_catalogs, "struck-out catalog numbers")?,
        })
    }
}

/// The rows under `content_hash` become exactly `choices`, on the
/// connection the caller holds — its own transaction, or a candidate save's.
/// A hash no candidate row stands under has nothing to hold a decision, and
/// the write says so rather than landing nowhere.
pub(super) fn replace_lookup_choices_on(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    choices: &LookupChoices,
) -> Result<(), DbError> {
    let affected = sql.execute(
        "INSERT INTO import_candidate_lookup_choices (\
             content_hash, disc_id_excluded, search_album, search_artist) \
         SELECT ?, ?, ?, ? \
         WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             disc_id_excluded = excluded.disc_id_excluded, \
             search_album = excluded.search_album, \
             search_artist = excluded.search_artist",
        params![
            content_hash,
            i64::from(choices.disc_id_excluded),
            choices
                .search_words
                .as_ref()
                .map(|words| words.album.as_str()),
            choices
                .search_words
                .as_ref()
                .map(|words| words.artist.as_str()),
            content_hash,
        ],
    )?;
    if affected == 0 {
        return Err(DbError::Message(
            "a candidate's lookup choices have no candidate state row to hang off".to_string(),
        ));
    }
    sql.execute(
        "DELETE FROM import_candidate_excluded_barcode WHERE content_hash = ?",
        [content_hash],
    )?;
    // A set, like the struck-out numbers: a value named twice is the
    // caller contradicting itself, and the primary key says so rather
    // than the second insert being quietly dropped.
    for value in &choices.excluded_barcodes {
        sql.execute(
            "INSERT INTO import_candidate_excluded_barcode (content_hash, value) \
             VALUES (?, ?)",
            params![content_hash, value],
        )?;
    }
    sql.execute(
        "DELETE FROM import_candidate_chosen_catalog WHERE content_hash = ?",
        [content_hash],
    )?;
    for (position, value) in choices.chosen_catalogs.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_chosen_catalog (content_hash, position, value) \
             VALUES (?, ?, ?)",
            params![content_hash, position as i64, value],
        )?;
    }
    sql.execute(
        "DELETE FROM import_candidate_discounted_catalog WHERE content_hash = ?",
        [content_hash],
    )?;
    // A set: a value named twice is the caller contradicting itself,
    // and the primary key says so rather than the second insert being
    // quietly dropped.
    for value in &choices.discounted_catalogs {
        sql.execute(
            "INSERT INTO import_candidate_discounted_catalog (content_hash, value) \
             VALUES (?, ?)",
            params![content_hash, value],
        )?;
    }
    Ok(())
}

impl Database {
    /// Record what a candidate's identification asks about, whole: the rows
    /// under `content_hash` become exactly this value.
    ///
    /// One transaction, so a reader never sees the header of one decision
    /// beside another's catalog numbers.
    pub async fn save_import_candidate_lookup_choices(
        &self,
        content_hash: &str,
        choices: &LookupChoices,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let choices = choices.clone();
        self.call(move |sql| replace_lookup_choices_on(sql, &content_hash, &choices))
            .await
    }
}
