//! What a person decided one candidate's identification asks about, as one
//! header row plus the left-out barcodes and the chosen and struck-out catalog
//! numbers hanging off it.
//!
//! Written whole: the caller hands over the next value of all of it, and what
//! the new value does not name is gone. A candidate with no header row has
//! decided nothing, which reads back as [`LookupChoices::default`].
//! Its tables are `lookup_choice_rows.sql`, which every device's library holds.

use super::*;
use bae_identify::{ChoiceChange, LookupChoices, SearchWords};
use bae_store::statement_optional;

// What a person decided the identification of each candidate that has decided
// anything asks about: the header row, with its lists as JSON arrays.
sqlive_compose::fragment!(
    LookupChoiceRow {
        choices_disc_id_excluded: bool,
        choices_chosen_catalogs: String,
        choices_discounted_catalogs: String,
        choices_excluded_barcodes: String
    },
    r#"
    SELECT choices.content_hash AS choices_hash,
        choices.disc_id_excluded AS choices_disc_id_excluded,
        choices.search_album AS choices_search_album,
        choices.search_artist AS choices_search_artist,
        (SELECT json_group_array(chosen.value ORDER BY chosen.position)
         FROM import_candidate_chosen_catalog AS chosen
         WHERE chosen.content_hash = choices.content_hash) AS choices_chosen_catalogs,
        (SELECT json_group_array(struck.value ORDER BY struck.value)
         FROM import_candidate_discounted_catalog AS struck
         WHERE struck.content_hash = choices.content_hash) AS choices_discounted_catalogs,
        (SELECT json_group_array(left_out.value ORDER BY left_out.value)
         FROM import_candidate_excluded_barcode AS left_out
         WHERE left_out.content_hash = choices.content_hash) AS choices_excluded_barcodes
    FROM import_candidate_lookup_choices AS choices
"#
);

sqlive_compose::read!(
    LookupChoicesOf(content_hash: String),
    r#"
    SELECT choices.*
    FROM @crate::tables::import_state::lookup_choice_rows::LookupChoiceRow AS choices
    WHERE choices.choices_hash = :content_hash
"#
);

impl LookupChoiceRow {
    pub(super) fn choices(&self) -> Result<LookupChoices, DbError> {
        let list = |json: &str, what: &str| {
            serde_json::from_str::<Vec<String>>(json)
                .map_err(|error| DbError::unreadable_json(format!("stored {what}"), json, error))
        };
        let search_words = match (&self.choices_search_album, &self.choices_search_artist) {
            (Some(album), Some(artist)) => {
                Some(SearchWords { album: album.clone(), artist: artist.clone() })
            }
            _ => None,
        };
        Ok(LookupChoices {
            disc_id_excluded: self.choices_disc_id_excluded,
            excluded_barcodes: list(&self.choices_excluded_barcodes, "left-out barcodes")?,
            chosen_catalogs: list(&self.choices_chosen_catalogs, "chosen catalog numbers")?,
            search_words,
            discounted_catalogs: list(
                &self.choices_discounted_catalogs,
                "struck-out catalog numbers",
            )?,
        })
    }
}

/// What the candidate `content_hash` decided its identification asks about:
/// the default when it has decided nothing.
fn load_lookup_choices_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<LookupChoices, DbError> {
    let request = LookupChoicesOfParams { content_hash: content_hash.to_string() };
    Ok(statement_optional(sql, &request)?
        .map(|row| row.choices.choices())
        .transpose()?
        .unwrap_or_default())
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
    // A pane's edit reads the candidate listed in this transaction, and a
    // listed candidate holds a state row.
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
    /// Make `edit` to what the identification of the candidate at `key` asks
    /// about, and answer what the change asks of it. One write reads the
    /// candidate's entry and its stored choices and stores the edited ones,
    /// so each change is made to the value the one before it left. It
    /// refuses a candidate the import list doesn't show, and one whose files
    /// this device imported.
    pub(crate) async fn edit_pane_lookup_choices(
        &self,
        key: &crate::CandidateKey,
        edit: bae_identify::LookupChoiceEdit,
    ) -> Result<ChoiceChange, crate::ImportError> {
        let key = key.clone();
        self.store
            .write(move |sql| {
                let sql = &sql;
                let entry = crate::tables::release_groupings::load_candidate_entry_on(sql, &key)?
                    .filter(|entry| entry.listed())
                    .ok_or_else(|| crate::ImportError::Internal {
                        detail: format!("{key} is not a scanned folder candidate"),
                    })?;
                if let Some(release_id) = entry.imported {
                    return Err(crate::ImportError::FilesAlreadyImported { release_id });
                }
                let content_hash = entry.candidate.files.content_hash();
                let stored = load_lookup_choices_on(sql, &content_hash)?;
                let choices = stored.clone().edited(edit);
                let change = match choices.asks_the_same_as(&stored) {
                    true => ChoiceChange::Ranking,
                    false => ChoiceChange::Lookups,
                };
                replace_lookup_choices_on(sql, &content_hash, &choices)?;
                Ok(change)
            })
            .await
    }

    /// Record what a candidate's identification asks about, whole, for a
    /// test: the rows under `content_hash` become exactly this value.
    ///
    /// One transaction, so a reader never sees the header of one decision
    /// beside another's catalog numbers.
    #[cfg(test)]
    pub(crate) async fn save_import_candidate_lookup_choices(
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
