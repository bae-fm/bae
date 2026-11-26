//! What a person decided one candidate's identification asks about, as one
//! header row plus the left-out barcodes and the chosen and struck-out catalog
//! numbers hanging off it.
//!
//! Written whole: the caller hands over the next value of all of it, and what
//! the new value does not name is gone. A candidate with no header row has
//! decided nothing, which reads back as [`LookupChoices::default`].
//! Its tables are `lookup_choice_rows.sql`, which every device's library holds.

use super::*;
use bae_identify::{LookupChoices, SearchWords};

/// What a candidate decided its identification asks about, as the state's
/// columns hold it, its lists still JSON.
pub(super) struct StoredLookupChoices {
    disc_id_excluded: bool,
    search_words: Option<SearchWords>,
    chosen_catalogs: String,
    discounted_catalogs: String,
    excluded_barcodes: String,
}

impl StoredLookupChoices {
    /// The choices a pane's read selects, as
    /// [`super::state_fragments::CurrentState`] names them; none for a
    /// candidate that has decided nothing.
    pub(super) fn of(
        state: &super::state_fragments::CurrentState,
    ) -> Result<Option<Self>, DbError> {
        if state.choices_row.is_none() {
            return Ok(None);
        }
        let search_words =
            match (state.choices_search_album.clone(), state.choices_search_artist.clone()) {
                (Some(album), Some(artist)) => Some(SearchWords { album, artist }),
                _ => None,
            };
        Ok(Some(Self {
            disc_id_excluded: super::required(
                state.choices_disc_id_excluded,
                "choices_disc_id_excluded",
            )?,
            search_words,
            chosen_catalogs: state.choices_chosen_catalogs.clone(),
            discounted_catalogs: state.choices_discounted_catalogs.clone(),
            excluded_barcodes: state.choices_excluded_barcodes.clone(),
        }))
    }
}

impl StoredLookupChoices {
    pub(super) fn choices(self) -> Result<LookupChoices, DbError> {
        let list = |json: &str, what: &str| {
            serde_json::from_str::<Vec<String>>(json)
                .map_err(|error| DbError::unreadable_json(format!("stored {what}"), json, error))
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
/// Forget what the person let the lookups of `content_hash` use; the barcode
/// and catalog lists hang off the choices row and go with it.
pub(super) fn delete_lookup_choices(
    sql: &CovenWrite<'_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_lookup_choices WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

pub(super) fn replace_lookup_choices_on(
    sql: &CovenWrite<'_>,
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
            choices.search_words.as_ref().map(|words| words.album.as_str()),
            choices.search_words.as_ref().map(|words| words.artist.as_str()),
            content_hash,
        ],
    )?;
    // Written under the folder-state lock, after a read under it found the
    // candidate listed, and a listed candidate holds a state row.
    assert!(affected == 1, "a candidate's lookup choices have no candidate state row to hang off");
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
    // Check and group the verdict's rows again against the text, without the
    // catalog numbers the person discounted.
    super::verdict_layout::lay_out_matches_on(
        sql,
        super::verdict_rows::Owner::Candidate(content_hash),
    )
}

impl ImportTables {
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
        self.store.call(move |sql| replace_lookup_choices_on(sql, &content_hash, &choices)).await
    }
}

#[cfg(test)]
#[path = "lookup_choice_rows_tests.rs"]
mod tests;
