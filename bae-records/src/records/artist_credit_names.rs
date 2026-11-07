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

/// Why an artist's other printed name could not be added or renamed.
#[derive(Debug, thiserror::Error)]
pub enum CreditNameError {
    /// The artist is not in the library: it was deleted, or merged into
    /// another, after the person opened it. Nothing was written.
    #[error("artist {0} is not in the library")]
    ArtistMissing(String),
    #[error(transparent)]
    Database(#[from] DbError),
}

/// A bug the store's write carried back, which the store reported as it
/// returned; a refusal holds none.
impl bae_util::task_failure::ReportedBug for CreditNameError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::ArtistMissing(_) => None,
        }
    }
}

/// The credit name a link to `artist_id` stores for `credit`: none where it
/// folds to the artist's own name, else its key, the name added to the
/// artist's `artist_credits` if it is new there. A name already there keeps
/// its spelling. Every link a write makes names an artist the write holds,
/// so a missing one is a bug, which panics in the store's work.
pub(super) fn settle_credit_name(
    conn: &CovenWrite<'_>,
    artist_id: &str,
    credit: Option<&CreditName>,
    now: &str,
) -> Result<Option<String>, DbError> {
    let Some(credit) = credit else {
        return Ok(None);
    };
    let own = own_name_key_on(conn, artist_id)?.unwrap_or_else(|| {
        panic!("a link names artist {artist_id}, which the write does not hold")
    });
    settle_credit_name_beside(conn, artist_id, &own, credit, now)
}

/// [`settle_credit_name`] for an artist whose own name's key is `own`.
fn settle_credit_name_beside(
    conn: &CovenWrite<'_>,
    artist_id: &str,
    own: &str,
    credit: &CreditName,
    now: &str,
) -> Result<Option<String>, DbError> {
    if own == credit.key {
        return Ok(None);
    }
    conn.execute(
        "INSERT INTO artist_credits (artist_id, name_key, name, created_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT (artist_id, name_key) DO NOTHING",
        params![artist_id, credit.key, credit.name, now],
    )?;
    Ok(Some(credit.key.clone()))
}

/// The key of `artist_id`'s own name; `None` when the library holds no such
/// artist.
fn own_name_key_on(conn: &CovenWrite<'_>, artist_id: &str) -> Result<Option<String>, DbError> {
    Ok(conn
        .query_row("SELECT name_key FROM artists WHERE id = ?", params![artist_id], |row| {
            row.get(0)
        })
        .optional()?)
}

/// Point every link naming `artist_id`'s credit name `from` at `to` (`None`:
/// the artist's own name).
fn relink_credit_name(
    conn: &CovenWrite<'_>,
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

/// The other printed names of the artist `artist_id` shows as (a SQL
/// expression), in name order, as a JSON array: its own rows and those of the
/// artists merged into it, read by the artists' shown-as index.
pub fn credit_names_json_sql(artist_id: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object('artist_id', credit_name.artist_id, \
                                              'key', credit_name.name_key, \
                                              'name', credit_name.name) \
                                  ORDER BY credit_name.name COLLATE NOCASE, credit_name.name_key) \
          FROM artist_shown_as shown \
          JOIN artist_credits credit_name ON credit_name.artist_id = shown.artist_id \
          WHERE shown.shown_artist_id = {artist_id})"
    )
}

/// The other printed names [`credit_names_json_sql`] reads into `column`.
pub fn credit_names_column(row: &Row<'_>, column: &str) -> rusqlite::Result<Vec<ArtistCreditName>> {
    #[derive(serde::Deserialize)]
    struct Stored {
        artist_id: String,
        key: String,
        name: String,
    }
    let stored: Vec<Stored> = bae_store::read::json_column(row, column)?;
    Ok(stored
        .into_iter()
        .map(|stored| ArtistCreditName {
            artist_id: stored.artist_id,
            credit: CreditName { key: stored.key, name: stored.name },
        })
        .collect())
}

/// The other printed names of each of `artist_ids` (artists as they show,
/// after merges), each artist's in name order: its own rows and those of the
/// artists merged into it.
pub fn credit_names_of_on(
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
                        credit: CreditName { key: row.get(2)?, name: row.get(3)? },
                    },
                ))
            })?
        {
            names.entry(shown_id).or_default().push(name);
        }
    }
    Ok(names)
}

impl Records {
    /// The other printed names of each of `artist_ids` (artists as they
    /// show, after merges), each artist's in name order.
    pub async fn get_artist_credit_names(
        &self,
        artist_ids: &[String],
    ) -> Result<HashMap<String, Vec<ArtistCreditName>>, DbError> {
        let artist_ids = artist_ids.to_vec();
        self.store.read(move |sql| credit_names_of_on(&sql, &artist_ids)).await
    }

    /// Give `artist_id` the other printed name `name`. A name that folds to
    /// the artist's own, or to one it already goes by, adds nothing: the
    /// artist already goes by it. An artist the library no longer holds is
    /// refused.
    pub async fn add_artist_credit_name(
        &self,
        artist_id: &str,
        name: &bae_store::EnteredName,
    ) -> Result<(), CreditNameError> {
        let (artist, name) = (artist_id.to_string(), name.as_str().to_string());
        let now = self.store.now_utc().to_rfc3339();
        let held = self
            .store
            .call_sql(move |sql| {
                let Some(own) = own_name_key_on(&sql, &artist)? else {
                    return Ok(false);
                };
                settle_credit_name_beside(&sql, &artist, &own, &CreditName::of(&name), &now)?;
                Ok(true)
            })
            .await?;
        match held {
            true => Ok(()),
            false => Err(CreditNameError::ArtistMissing(artist_id.to_string())),
        }
    }

    /// Rename `artist_id`'s credit name `key` to `name`, in one write: its
    /// links follow it. A new spelling of the same folded name rewrites the
    /// name; a name another of the artist's names already is joins that one;
    /// the artist's own name leaves the links printing it, and the credit name
    /// goes. An artist the library no longer holds is refused.
    pub async fn rename_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
        name: &bae_store::EnteredName,
    ) -> Result<(), CreditNameError> {
        let (artist, key, name) =
            (artist_id.to_string(), key.to_string(), name.as_str().to_string());
        let now = self.store.now_utc().to_rfc3339();
        let held = self
            .store
            .call_sql(move |sql| {
                let Some(own) = own_name_key_on(&sql, &artist)? else {
                    return Ok(false);
                };
                let renamed = CreditName::of(&name);
                if renamed.key == key {
                    sql.execute(
                        "UPDATE artist_credits SET name = ? WHERE artist_id = ? AND name_key = ?",
                        params![renamed.name, artist, key],
                    )?;
                    return Ok(true);
                }
                let to = settle_credit_name_beside(&sql, &artist, &own, &renamed, &now)?;
                relink_credit_name(&sql, &artist, &key, to.as_deref())?;
                sql.execute(
                    "DELETE FROM artist_credits WHERE artist_id = ? AND name_key = ?",
                    params![artist, key],
                )?;
                Ok(true)
            })
            .await?;
        match held {
            true => Ok(()),
            false => Err(CreditNameError::ArtistMissing(artist_id.to_string())),
        }
    }

    /// Remove `artist_id`'s credit name `key`, in one write: its links print
    /// the artist's own name from then on.
    pub async fn remove_artist_credit_name(
        &self,
        artist_id: &str,
        key: &str,
    ) -> Result<(), DbError> {
        let (artist_id, key) = (artist_id.to_string(), key.to_string());
        self.store
            .call_sql(move |sql| {
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

#[cfg(test)]
#[path = "artist_credit_names_tests.rs"]
mod tests;
