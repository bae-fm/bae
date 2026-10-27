//! An artist's other printed names (`artist_credits`): what links settle on
//! when a credit prints one, what the artist page lists and edits, and what
//! the artist picker offers under each artist.
//!
//! A credit name is keyed by its artist and its folded name, so two devices
//! meeting one printed name write one row. A link names it by that key; a
//! name the person removes or renames is first unlinked or relinked in the
//! same write, so no link names a key its artist no longer holds.

use super::*;
use crate::credits::{ArtistCreditName, CreditName};

/// The credit name a link to `artist_id` stores for `credit`: none where it
/// folds to the artist's own name, else its key, the name added to the
/// artist's `artist_credits` if it is new there. A name already there keeps
/// its spelling.
pub(super) fn settle_credit_name(
    conn: &SqlContext<'_, '_>,
    artist_id: &str,
    credit: Option<&CreditName>,
    now: &str,
) -> Result<Option<String>, DbError> {
    let Some(credit) = credit else {
        return Ok(None);
    };
    if own_name_key_on(conn, artist_id)? == credit.key {
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO artist_credits (artist_id, name_key, name, created_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT (artist_id, name_key) DO NOTHING",
        params![artist_id, credit.key, credit.name, now],
    )?;
    Ok(Some(credit.key.clone()))
}

fn own_name_key_on(conn: &SqlContext<'_, '_>, artist_id: &str) -> Result<String, DbError> {
    conn.query_row(
        "SELECT name_key FROM artists WHERE id = ?",
        params![artist_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| DbError::Message(format!("artist {artist_id} is not in the library")))
}

/// Point every link naming `artist_id`'s credit name `from` at `to` (`None`:
/// the artist's own name).
fn relink_credit_name(
    conn: &SqlContext<'_, '_>,
    artist_id: &str,
    from: &str,
    to: Option<&str>,
) -> Result<(), DbError> {
    for table in ["album_artists", "track_artists"] {
        conn.execute(
            &format!("UPDATE {table} SET credit_key = ? WHERE artist_id = ? AND credit_key = ?"),
            params![to, artist_id, from],
        )?;
    }
    Ok(())
}

/// The other printed names of each of `artist_ids` (artists as they show,
/// after merges), each artist's in name order: its own rows and those of the
/// artists merged into it.
pub(super) fn credit_names_of_on(
    sql: &SqlReadContext<'_>,
    artist_ids: &[String],
) -> Result<HashMap<String, Vec<ArtistCreditName>>, DbError> {
    let mut names: HashMap<String, Vec<ArtistCreditName>> = HashMap::new();
    for chunk in artist_ids.chunks(SQL_MAX_IN_VARS) {
        let query = format!(
            "SELECT shown.shown_artist_id AS shown_id, credit_name.artist_id, \
                    credit_name.name_key, credit_name.name \
             FROM artist_shown_as shown \
             JOIN artist_credits credit_name ON credit_name.artist_id = shown.artist_id \
             WHERE shown.shown_artist_id IN ({placeholders}) \
             ORDER BY shown_id, credit_name.name COLLATE NOCASE, credit_name.name_key",
            placeholders = in_clause_placeholders(chunk.len()),
        );
        for (shown_id, name) in
            sql.query(&query, rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ArtistCreditName {
                        artist_id: row.get(1)?,
                        credit: CreditName {
                            key: row.get(2)?,
                            name: row.get(3)?,
                        },
                    },
                ))
            })?
        {
            names.entry(shown_id).or_default().push(name);
        }
    }
    Ok(names)
}

impl Database {
    /// The other printed names of each of `artist_ids` (artists as they
    /// show, after merges), each artist's in name order.
    pub async fn get_artist_credit_names(
        &self,
        artist_ids: &[String],
    ) -> Result<HashMap<String, Vec<ArtistCreditName>>, DbError> {
        let artist_ids = artist_ids.to_vec();
        self.read(move |sql| credit_names_of_on(&sql, &artist_ids))
            .await
    }

    /// Give `artist_id` the other printed name `name`. A name that folds to
    /// the artist's own, or to one it already goes by, adds nothing: the
    /// artist already goes by it.
    pub async fn add_artist_credit_name(&self, artist_id: &str, name: &str) -> Result<(), DbError> {
        let (artist_id, name) = (artist_id.to_string(), name.trim().to_string());
        let now = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now()).to_rfc3339();
        self.call_sql(move |sql| {
            settle_credit_name(&sql, &artist_id, Some(&CreditName::of(&name)), &now)?;
            Ok(())
        })
        .await
    }

    /// Rename `artist_id`'s credit name `key` to `name`, in one write: its
    /// links follow it. A new spelling of the same folded name rewrites the
    /// name; a name another of the artist's names already is joins that one;
    /// the artist's own name leaves the links printing it, and the credit name
    /// goes.
    pub async fn rename_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
        name: &str,
    ) -> Result<(), DbError> {
        let (artist_id, key, name) = (
            artist_id.to_string(),
            key.to_string(),
            name.trim().to_string(),
        );
        let now = chrono::DateTime::<chrono::Utc>::from(self.inner.clock.now()).to_rfc3339();
        self.call_sql(move |sql| {
            let renamed = CreditName::of(&name);
            if renamed.key == key {
                sql.execute(
                    "UPDATE artist_credits SET name = ? WHERE artist_id = ? AND name_key = ?",
                    params![renamed.name, artist_id, key],
                )?;
                return Ok(());
            }
            let to = settle_credit_name(&sql, &artist_id, Some(&renamed), &now)?;
            relink_credit_name(&sql, &artist_id, &key, to.as_deref())?;
            sql.execute(
                "DELETE FROM artist_credits WHERE artist_id = ? AND name_key = ?",
                params![artist_id, key],
            )?;
            Ok(())
        })
        .await
    }

    /// Remove `artist_id`'s credit name `key`, in one write: its links print
    /// the artist's own name from then on.
    pub async fn remove_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
    ) -> Result<(), DbError> {
        let (artist_id, key) = (artist_id.to_string(), key.to_string());
        self.call_sql(move |sql| {
            relink_credit_name(&sql, &artist_id, &key, None)?;
            sql.execute(
                "DELETE FROM artist_credits WHERE artist_id = ? AND name_key = ?",
                params![artist_id, key],
            )?;
            Ok(())
        })
        .await
    }
}
