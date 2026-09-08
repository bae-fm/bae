//! What a person decided one candidate's identification asks about, as one
//! header row plus the chosen catalog numbers hanging off it.
//!
//! Written whole: the caller hands over the next value of all of it, and what
//! the new value does not name is gone. A candidate with no header row has
//! decided nothing, which reads back as [`LookupChoices::default`].

use super::*;
use crate::import::LookupChoices;

/// Every candidate's lookup choices, or the one `only` names. A candidate with
/// no row is absent from the map; the caller defaults it.
pub(super) fn load_lookup_choices_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<
    impl FnOnce() -> Result<HashMap<String, LookupChoices>, DbError> + Send + 'static,
    DbError,
> {
    let catalogs = sql.query(
        "SELECT content_hash, value FROM import_candidate_chosen_catalog \
         WHERE :only IS NULL OR content_hash = :only \
         ORDER BY content_hash, position",
        named_params! { ":only": only },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let rows = sql.query(
        "SELECT content_hash, disc_id_excluded, barcode_excluded \
         FROM import_candidate_lookup_choices \
         WHERE :only IS NULL OR content_hash = :only",
        named_params! { ":only": only },
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                row.get::<_, i64>(2)? != 0,
            ))
        },
    )?;
    Ok(move || {
        let mut chosen: HashMap<String, Vec<String>> = HashMap::new();
        for (content_hash, value) in catalogs {
            chosen.entry(content_hash).or_default().push(value);
        }
        let mut out = HashMap::with_capacity(rows.len());
        for (content_hash, disc_id_excluded, barcode_excluded) in rows {
            let chosen_catalogs = chosen.remove(&content_hash).unwrap_or_default();
            out.insert(
                content_hash,
                LookupChoices {
                    disc_id_excluded,
                    barcode_excluded,
                    chosen_catalogs,
                },
            );
        }
        Ok(out)
    })
}

impl Database {
    /// Record what a candidate's identification asks about, whole: the rows
    /// under `content_hash` become exactly this value.
    ///
    /// One transaction, so a reader never sees the header of one decision
    /// beside another's catalog numbers. A hash no candidate row stands under
    /// has nothing to hold a decision, and the write says so rather than
    /// landing nowhere.
    pub async fn save_import_candidate_lookup_choices(
        &self,
        content_hash: &str,
        choices: &LookupChoices,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let choices = choices.clone();
        self.call(move |sql| {
            let affected = sql.execute(
                "INSERT INTO import_candidate_lookup_choices (\
                     content_hash, disc_id_excluded, barcode_excluded) \
                 SELECT ?, ?, ? \
                 WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     disc_id_excluded = excluded.disc_id_excluded, \
                     barcode_excluded = excluded.barcode_excluded",
                params![
                    content_hash,
                    i64::from(choices.disc_id_excluded),
                    i64::from(choices.barcode_excluded),
                    content_hash,
                ],
            )?;
            if affected == 0 {
                return Err(DbError::Message(
                    "a candidate's lookup choices have no candidate state row to hang off"
                        .to_string(),
                ));
            }
            sql.execute(
                "DELETE FROM import_candidate_chosen_catalog WHERE content_hash = ?",
                [&content_hash],
            )?;
            for (position, value) in choices.chosen_catalogs.iter().enumerate() {
                sql.execute(
                    "INSERT INTO import_candidate_chosen_catalog (content_hash, position, value) \
                     VALUES (?, ?, ?)",
                    params![content_hash, position as i64, value],
                )?;
            }
            Ok(())
        })
        .await
    }
}
