//! Which library artists each artist credit of a candidate's draft names, as
//! the records' rule decides it (`bae_records::credit_artist_clauses`), stored
//! per distinct credit (its folded name and its two catalog ids, all the rule
//! reads of it).
//!
//! The SQL file beside this one holds the tables and the triggers that mark a
//! credit due when a write changes the credit or an artist the rule can name
//! for it, in this device's writes and in the ones coven applies from another
//! device alike. [`answer_due_credits`], the part's before-commit function,
//! runs last in every such transaction: it answers each due credit with the
//! rule, deletes the answer of a credit no draft holds any more, and empties
//! the list. The import pane's reads join the stored answers
//! (`artist_standings`). A release editor's form is in no table, since what
//! the person types is view state, so its draft's read applies the same
//! rule's SQL to the form's credits.

use rusqlite::{params, OptionalExtension};

/// The credit resolutions' part of the library's schema. It runs after the
/// part creating the draft's credit tables, which its triggers fire on.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("credit_resolutions.sql")])
        .before_commit(answer_due_credits);

/// Whether a draft holds a credit of the relation row `credit` by its folded
/// name and catalog ids, as an SQL condition.
fn held_sql(credit: &str) -> String {
    let holding = |table: &str| {
        format!(
            "EXISTS (SELECT 1 FROM {table} AS held \
                     WHERE held.assignment_kind = 'credit' \
                       AND bae_artist_name_key(held.name) = {credit}.name_key \
                       AND held.musicbrainz_artist_id IS {credit}.musicbrainz_artist_id \
                       AND held.discogs_artist_id IS {credit}.discogs_artist_id)"
        )
    };
    format!(
        "({} OR {})",
        holding("import_candidate_album_artist_assignment"),
        holding("import_candidate_track_artist_assignment"),
    )
}

/// An answer's artists as the JSON array that stored answers and the rule's
/// are compared, stored and read as: the rows of `artists` that `of` picks,
/// in `position` order, whose catalog ids are in the columns `musicbrainz`
/// and `discogs`.
fn answer_json_sql(artists: &str, musicbrainz: &str, discogs: &str, of: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
                     'artist_id', a.artist_id, 'name', a.name, 'sort_name', a.sort_name, \
                     'musicbrainz_artist_id', a.{musicbrainz}, \
                     'discogs_artist_id', a.{discogs}) ORDER BY a.position) \
          FROM {artists} AS a WHERE {of})"
    )
}

/// The artists of the stored answer whose id the SQL expression `resolution`
/// holds, as [`answer_json_sql`] gives them.
pub(super) fn stored_answer_json_sql(resolution: &str) -> String {
    answer_json_sql(
        "import_credit_resolution_artist",
        "musicbrainz_artist_id",
        "discogs_artist_id",
        &format!("a.resolution = {resolution}"),
    )
}

/// The artists the rule names for the credit of the row `credit`, which has
/// the columns `name_key`, `musicbrainz_artist_id` and `discogs_artist_id`,
/// as [`answer_json_sql`] gives them, in a statement that puts
/// `bae_records::credit_artist_clauses` after its WITH.
pub(super) fn rule_answer_json_sql(credit: &str) -> String {
    answer_json_sql(
        "credit_artist",
        "artist_musicbrainz_id",
        "artist_discogs_id",
        &format!(
            "a.name_key = {credit}.name_key \
             AND a.musicbrainz_artist_id IS {credit}.musicbrainz_artist_id \
             AND a.discogs_artist_id IS {credit}.discogs_artist_id"
        ),
    )
}

/// Answer every credit the transaction's writes marked due and empty the
/// list: each one a draft still holds gets the rule's answer, written where
/// it differs from the stored one, and each one none holds loses its answer.
fn answer_due_credits(sql: &coven::CommitSql<'_>) -> Result<(), coven::DbError> {
    let any_due: bool =
        sql.query_row("SELECT EXISTS (SELECT 1 FROM import_credit_resolution_due)", [], |row| {
            row.get(0)
        })?;
    if !any_due {
        return Ok(());
    }
    sql.execute(
        &format!(
            "DELETE FROM import_credit_resolution AS r \
             WHERE EXISTS (SELECT 1 FROM import_credit_resolution_due AS d \
                           WHERE d.name_key = r.name_key \
                             AND d.musicbrainz_artist_id IS r.musicbrainz_artist_id \
                             AND d.discogs_artist_id IS r.discogs_artist_id) \
               AND NOT {}",
            held_sql("r")
        ),
        [],
    )?;
    let changed = sql.query(
        &format!(
            "WITH due_credit AS ( \
                 SELECT DISTINCT name_key, musicbrainz_artist_id, discogs_artist_id \
                 FROM import_credit_resolution_due AS due WHERE {held}), \
             {rule}, \
             fresh AS ( \
                 SELECT c.name_key, c.musicbrainz_artist_id, c.discogs_artist_id, \
                        {fresh} AS artists \
                 FROM due_credit AS c) \
             SELECT f.name_key, f.musicbrainz_artist_id, f.discogs_artist_id, f.artists \
             FROM fresh AS f \
             LEFT JOIN import_credit_resolution AS r \
                 ON r.name_key = f.name_key \
                AND r.musicbrainz_artist_id IS f.musicbrainz_artist_id \
                AND r.discogs_artist_id IS f.discogs_artist_id \
             WHERE r.id IS NULL OR {stored} IS NOT f.artists",
            held = held_sql("due"),
            rule = bae_records::credit_artist_clauses("due_credit"),
            fresh = rule_answer_json_sql("c"),
            stored = stored_answer_json_sql("r.id"),
        ),
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
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
                sql.execute(
                    "DELETE FROM import_credit_resolution_artist WHERE resolution = ?1",
                    [resolution],
                )?;
                resolution
            }
            None => {
                sql.execute(
                    "INSERT INTO import_credit_resolution \
                         (name_key, musicbrainz_artist_id, discogs_artist_id) \
                     VALUES (?1, ?2, ?3)",
                    key,
                )?;
                resolution_of()?.expect("the answer this function just inserted")
            }
        };
        sql.execute(
            "INSERT INTO import_credit_resolution_artist \
                 (resolution, position, artist_id, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id) \
             SELECT ?1, j.key, j.value ->> 'artist_id', j.value ->> 'name', \
                    j.value ->> 'sort_name', j.value ->> 'musicbrainz_artist_id', \
                    j.value ->> 'discogs_artist_id' \
             FROM json_each(?2) AS j",
            params![resolution, artists],
        )?;
    }
    sql.execute("DELETE FROM import_credit_resolution_due", [])?;
    Ok(())
}

#[cfg(test)]
#[path = "credit_resolutions_tests.rs"]
mod tests;
