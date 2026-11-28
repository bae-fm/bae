//! Which library artists each artist credit of a candidate's draft names, as
//! the records' rule decides it (`bae_records::records::credit_rule::CreditAnswers`), stored
//! per distinct credit (its folded name and its two catalog ids, all the rule
//! reads of it).
//!
//! The SQL file beside this one holds the tables and the triggers that mark a
//! credit due when a write changes the credit or an artist the rule can name
//! for it, in this device's writes and in the ones coven applies from another
//! device alike. `answer_due_credits`, the part's before-commit function,
//! runs last in every such transaction: it answers each due credit with the
//! rule, deletes the answer of a credit no draft holds any more, and empties
//! the list. The import pane's reads join the stored answers
//! (`artist_standings`). A release editor's form is in no table, since what
//! the person types is view state, so its draft's read applies the same
//! rule to the form's credits.

use bae_store::{statement_rows, WriteStatements};
use rusqlite::{params, OptionalExtension};

pub mod row_fragments;

coven::read!(
    writer DueCredit { held: bool },
    r#"
    SELECT DISTINCT due.name_key, due.musicbrainz_artist_id, due.discogs_artist_id,
           (EXISTS (SELECT 1
                   FROM @crate::tables::import_state::row_fragments::AlbumCreditNames AS held
                   WHERE held.name_key = due.name_key
                     AND held.musicbrainz_artist_id IS due.musicbrainz_artist_id
                     AND held.discogs_artist_id IS due.discogs_artist_id)
            OR EXISTS (SELECT 1
                   FROM @crate::tables::import_state::row_fragments::TrackCreditNames AS held
                   WHERE held.name_key = due.name_key
                     AND held.musicbrainz_artist_id IS due.musicbrainz_artist_id
                     AND held.discogs_artist_id IS due.discogs_artist_id)) AS held
    FROM import_credit_resolution_due AS due
"#
);

coven::read!(
    writer ChangedCreditAnswer(credits: String),
    r#"
    SELECT fresh.*
    FROM @bae_records::records::credit_rule::CreditAnswers(credits: (
        SELECT j.value ->> 'name_key' AS name_key,
               j.value ->> 'musicbrainz_artist_id' AS musicbrainz_artist_id,
               j.value ->> 'discogs_artist_id' AS discogs_artist_id
        FROM json_each(:credits) AS j)) AS fresh
    LEFT JOIN import_credit_resolution AS r
        ON r.name_key = fresh.name_key
       AND r.musicbrainz_artist_id IS fresh.musicbrainz_artist_id
       AND r.discogs_artist_id IS fresh.discogs_artist_id
    WHERE r.id IS NULL
       OR (SELECT json_group_array(json_object(
                      'artist_id', a.artist_id, 'name', a.name, 'sort_name', a.sort_name,
                      'musicbrainz_artist_id', a.musicbrainz_artist_id,
                      'discogs_artist_id', a.discogs_artist_id) ORDER BY a.position)
           FROM import_credit_resolution_artist AS a
           WHERE a.resolution = r.id) IS NOT fresh.artists
"#
);

coven::write!(
    DeleteCreditAnswer(name_key: &'p str, musicbrainz: Option<&'p str>, discogs: Option<&'p str>),
    r#"
    DELETE FROM import_credit_resolution
    WHERE name_key = :name_key AND musicbrainz_artist_id IS :musicbrainz
      AND discogs_artist_id IS :discogs
"#
);

coven::write!(
    InsertCreditAnswer(name_key: &'p str, musicbrainz: Option<&'p str>, discogs: Option<&'p str>),
    r#"
    INSERT INTO import_credit_resolution (name_key, musicbrainz_artist_id, discogs_artist_id)
    VALUES (:name_key, :musicbrainz, :discogs)
"#
);

coven::write!(
    DeleteCreditAnswerArtists(resolution: i64),
    r#"DELETE FROM import_credit_resolution_artist WHERE resolution = :resolution"#
);

// The artists of the JSON array `:artists`, as the answer `:resolution`'s.
coven::write!(
    InsertCreditAnswerArtists(resolution: i64, artists: &'p str),
    r#"
    INSERT INTO import_credit_resolution_artist (
        resolution, position, artist_id, name, sort_name, musicbrainz_artist_id,
        discogs_artist_id)
    SELECT :resolution, j.key, j.value ->> 'artist_id', j.value ->> 'name',
        j.value ->> 'sort_name', j.value ->> 'musicbrainz_artist_id',
        j.value ->> 'discogs_artist_id'
    FROM json_each(:artists) AS j
"#
);

coven::write!(DeleteDueCredits, r#"DELETE FROM import_credit_resolution_due"#);

/// Answer every credit the transaction's writes marked due and empty the
/// list: each one a draft still holds gets the rule's answer, written where
/// it differs from the stored one, and each one none holds loses its answer.
pub(crate) fn answer_due_credits(sql: &coven::CommitSql<'_>) -> Result<(), coven::DbError> {
    let due = statement_rows(sql, &DueCreditParams {})?;
    if due.is_empty() {
        return Ok(());
    }
    let mut held = Vec::new();
    for credit in due {
        if credit.held {
            held.push(serde_json::json!({
                "name_key": credit.name_key,
                "musicbrainz_artist_id": credit.musicbrainz_artist_id,
                "discogs_artist_id": credit.discogs_artist_id,
            }));
        } else {
            sql.run(&DeleteCreditAnswerParams {
                name_key: &credit.name_key,
                musicbrainz: credit.musicbrainz_artist_id.as_deref(),
                discogs: credit.discogs_artist_id.as_deref(),
            })?;
        }
    }
    let request = ChangedCreditAnswerParams { credits: serde_json::Value::Array(held).to_string() };
    let changed: Vec<_> =
        statement_rows(sql, &request)?.into_iter().map(|row| row.fresh.into_columns()).collect();
    for (name_key, musicbrainz, discogs, artists) in &changed {
        let key = params![name_key, musicbrainz, discogs];
        let resolution_of = || {
            sql.query_row(
                "SELECT id FROM import_credit_resolution \
                 WHERE name_key = ?1 AND musicbrainz_artist_id IS ?2 AND discogs_artist_id IS ?3",
                key,
                |row| row.get::<_, i64>(0),
            )
            .optional()
        };
        let resolution = match resolution_of()? {
            Some(resolution) => {
                sql.run(&DeleteCreditAnswerArtistsParams { resolution })?;
                resolution
            }
            None => {
                sql.run(&InsertCreditAnswerParams {
                    name_key: name_key
                        .as_deref()
                        .expect("the rule answers a due credit, whose name key is never NULL"),
                    musicbrainz: musicbrainz.as_deref(),
                    discogs: discogs.as_deref(),
                })?;
                resolution_of()?.expect("the answer this function just inserted")
            }
        };
        sql.run(&InsertCreditAnswerArtistsParams { resolution, artists })?;
    }
    sql.run(&DeleteDueCreditsParams {})?;
    Ok(())
}

#[cfg(test)]
#[path = "credit_resolutions_tests.rs"]
mod tests;
