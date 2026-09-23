//! What a person decided one candidate's identification asks about, as one
//! header row plus the left-out barcodes and the chosen and struck-out catalog
//! numbers hanging off it.
//!
//! Written whole: the caller hands over the next value of all of it, and what
//! the new value does not name is gone. A candidate with no header row has
//! decided nothing, which reads back as [`LookupChoices::default`].

use super::super::query::QueryRows;
use super::*;
use crate::import::{LookupChoices, SearchWords};

/// The choices rows of every candidate, or of the one `only` names, read on
/// whichever connection the caller holds — a read snapshot, or a candidate
/// save's own transaction — and assembled after.
pub(super) struct LookupChoiceRows {
    catalogs: Vec<(String, String)>,
    discounted: Vec<(String, String)>,
    left_out: Vec<(String, String)>,
    rows: Vec<(String, bool, Option<SearchWords>)>,
}

pub(super) fn load_lookup_choice_rows_on(
    sql: &impl QueryRows,
    only: Option<&str>,
) -> Result<LookupChoiceRows, DbError> {
    let catalogs = sql.query(
        "SELECT content_hash, value FROM import_candidate_chosen_catalog \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position",
        named_params! { ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let discounted = sql.query(
        "SELECT content_hash, value FROM import_candidate_discounted_catalog \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, value",
        named_params! { ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let left_out = sql.query(
        "SELECT content_hash, value FROM import_candidate_excluded_barcode \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, value",
        named_params! { ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let rows = sql.query(
        "SELECT content_hash, disc_id_excluded, search_album, search_artist \
         FROM import_candidate_lookup_choices \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            let search_words = match (
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ) {
                (Some(album), Some(artist)) => Some(SearchWords { album, artist }),
                _ => None,
            };
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                search_words,
            ))
        },
    )?;
    Ok(LookupChoiceRows {
        catalogs,
        discounted,
        left_out,
        rows,
    })
}

impl LookupChoiceRows {
    /// Each candidate's choices, keyed by hash. A candidate with no header
    /// row is absent; the caller defaults it.
    pub(super) fn assemble(self) -> HashMap<String, LookupChoices> {
        let Self {
            catalogs,
            discounted,
            left_out,
            rows,
        } = self;
        let mut chosen: HashMap<String, Vec<String>> = HashMap::new();
        for (content_hash, value) in catalogs {
            chosen.entry(content_hash).or_default().push(value);
        }
        let mut struck_out: HashMap<String, Vec<String>> = HashMap::new();
        for (content_hash, value) in discounted {
            struck_out.entry(content_hash).or_default().push(value);
        }
        let mut barcodes: HashMap<String, Vec<String>> = HashMap::new();
        for (content_hash, value) in left_out {
            barcodes.entry(content_hash).or_default().push(value);
        }
        let mut out = HashMap::with_capacity(rows.len());
        for (content_hash, disc_id_excluded, search_words) in rows {
            let chosen_catalogs = chosen.remove(&content_hash).unwrap_or_default();
            let discounted_catalogs = struck_out.remove(&content_hash).unwrap_or_default();
            let excluded_barcodes = barcodes.remove(&content_hash).unwrap_or_default();
            out.insert(
                content_hash,
                LookupChoices {
                    disc_id_excluded,
                    excluded_barcodes,
                    chosen_catalogs,
                    search_words,
                    discounted_catalogs,
                },
            );
        }
        out
    }
}

/// Every candidate's lookup choices, or the one `only` names, as a read
/// snapshot's deferred assembly. A candidate with no row is absent from the
/// map; the caller defaults it.
pub(super) fn load_lookup_choices_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, LookupChoices>, DbError> + Send + 'static,
    DbError,
> {
    let rows = load_lookup_choice_rows_on(sql, only)?;
    Ok(move || Ok(rows.assemble()))
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
