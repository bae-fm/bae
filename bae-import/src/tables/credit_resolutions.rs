//! Which library artists each artist credit of a candidate's draft or of a
//! release editor's session names, as the records' rule
//! (`bae_records::resolve_credit_on`) decides it, stored per distinct credit
//! (its folded name and its two catalog ids, all the rule reads of it).
//!
//! The SQL file beside this one holds the tables and the triggers that mark a
//! credit due when a write changes the credit or an artist the rule can name
//! for it, in this device's writes and in the ones coven applies from another
//! device alike. [`answer_due_credits`], the part's before-commit function,
//! runs last in every such transaction: it answers each due credit with the
//! rule, deletes the answer of a credit no draft or session holds any more,
//! and empties the list. The import pane's and the release editor's reads
//! join the stored answers (`artist_standings`).

use bae_catalogs::credits::ArtistCredit;
use bae_records::{CreditResolution, ExistingArtist};
use rusqlite::{params, OptionalExtension};

/// The credit resolutions' part of the library's schema. It runs after the
/// parts creating the draft's and the session's credit tables, which its
/// triggers fire on.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("credit_resolutions.sql")])
        .before_commit(answer_due_credits);

/// A credit as the rule reads it: its folded name and its catalog ids.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CreditKey {
    name_key: String,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
}

/// Answer every credit the transaction's writes marked due and empty the
/// list: each one a draft or a session still holds gets the rule's answer,
/// written where it differs from the stored one, and each one none holds
/// loses its answer.
fn answer_due_credits(sql: &coven::CommitSql<'_>) -> Result<(), coven::DbError> {
    let due = sql.query(
        "SELECT DISTINCT name_key, musicbrainz_artist_id, discogs_artist_id \
         FROM import_credit_resolution_due",
        [],
        |row| {
            Ok(CreditKey {
                name_key: row.get(0)?,
                musicbrainz_artist_id: row.get(1)?,
                discogs_artist_id: row.get(2)?,
            })
        },
    )?;
    if due.is_empty() {
        return Ok(());
    }
    for key in &due {
        match held_name_on(sql, key)? {
            Some(name) => {
                let credit = ArtistCredit {
                    credited_as: None,
                    name,
                    sort_name: None,
                    musicbrainz_artist_id: key.musicbrainz_artist_id.clone(),
                    discogs_artist_id: key.discogs_artist_id.clone(),
                };
                let answer = match bae_records::resolve_credit_on(sql, &credit)? {
                    CreditResolution::New => Vec::new(),
                    CreditResolution::Library { artist } => vec![artist],
                    CreditResolution::Ambiguous { artists } => artists,
                };
                if stored_answer_on(sql, key)?.as_ref() != Some(&answer) {
                    store_answer_on(sql, key, &answer)?;
                }
            }
            None => {
                sql.execute(
                    "DELETE FROM import_credit_resolution \
                     WHERE name_key = ?1 AND musicbrainz_artist_id IS ?2 \
                       AND discogs_artist_id IS ?3",
                    params![key.name_key, key.musicbrainz_artist_id, key.discogs_artist_id],
                )?;
            }
        }
    }
    sql.execute("DELETE FROM import_credit_resolution_due", [])?;
    Ok(())
}

/// The name of one credit a draft or a session holds as `key`, any one of
/// them: the rule reads only its folded name, which they share. `None` when
/// none holds it.
fn held_name_on(sql: &coven::CommitSql<'_>, key: &CreditKey) -> rusqlite::Result<Option<String>> {
    let holding = |table: &str| {
        format!(
            "SELECT name FROM {table} WHERE assignment_kind = 'credit' \
               AND bae_artist_name_key(name) = ?1 AND musicbrainz_artist_id IS ?2 \
               AND discogs_artist_id IS ?3"
        )
    };
    sql.query_row(
        &format!(
            "{} UNION ALL {} UNION ALL {} UNION ALL {} LIMIT 1",
            holding("import_candidate_album_artist_assignment"),
            holding("import_candidate_track_artist_assignment"),
            holding("release_edit_album_artist"),
            holding("release_edit_track_artist"),
        ),
        params![key.name_key, key.musicbrainz_artist_id, key.discogs_artist_id],
        |row| row.get(0),
    )
    .optional()
}

/// The artists the stored answer for `key` names, in order; `None` when no
/// answer is stored.
fn stored_answer_on(
    sql: &coven::CommitSql<'_>,
    key: &CreditKey,
) -> rusqlite::Result<Option<Vec<ExistingArtist>>> {
    let Some(resolution) = resolution_id_on(sql, key)? else {
        return Ok(None);
    };
    sql.query(
        "SELECT artist_id, name, sort_name, musicbrainz_artist_id, discogs_artist_id \
         FROM import_credit_resolution_artist WHERE resolution = ?1 ORDER BY position",
        [resolution],
        |row| {
            Ok(ExistingArtist {
                artist_id: row.get(0)?,
                name: row.get(1)?,
                sort_name: row.get(2)?,
                musicbrainz_artist_id: row.get(3)?,
                discogs_artist_id: row.get(4)?,
            })
        },
    )
    .map(Some)
}

/// The id of the stored answer for `key`.
fn resolution_id_on(sql: &coven::CommitSql<'_>, key: &CreditKey) -> rusqlite::Result<Option<i64>> {
    sql.query_row(
        "SELECT id FROM import_credit_resolution \
         WHERE name_key = ?1 AND musicbrainz_artist_id IS ?2 AND discogs_artist_id IS ?3",
        params![key.name_key, key.musicbrainz_artist_id, key.discogs_artist_id],
        |row| row.get(0),
    )
    .optional()
}

/// Store `answer` as the answer for `key`, over any stored one.
fn store_answer_on(
    sql: &coven::CommitSql<'_>,
    key: &CreditKey,
    answer: &[ExistingArtist],
) -> rusqlite::Result<()> {
    let resolution = match resolution_id_on(sql, key)? {
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
                params![key.name_key, key.musicbrainz_artist_id, key.discogs_artist_id],
            )?;
            resolution_id_on(sql, key)?.expect("the answer this function just inserted")
        }
    };
    for (position, artist) in answer.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_credit_resolution_artist \
                 (resolution, position, artist_id, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                resolution,
                position as i64,
                artist.artist_id,
                artist.name,
                artist.sort_name,
                artist.musicbrainz_artist_id,
                artist.discogs_artist_id,
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "credit_resolutions_tests.rs"]
mod tests;
