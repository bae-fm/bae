//! An artist's other printed names (`artist_credits`): what links settle on
//! when a credit prints one, what the artist page lists and edits, and what
//! the artist picker offers under each artist.
//!
//! A credit name is keyed by its artist and its folded name: one printed name
//! of an artist is one row. A link names it by that key; a
//! name the person removes or renames is first unlinked or relinked in the
//! same write, so no link names a key its artist no longer holds.

use super::*;
use crate::credits::{ArtistCreditName, CreditName};
use bae_store::WriteStatements as _;
use bae_util::timestamp::StoredTimestamp as _;

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

sqlive_compose::write!(
    KeepCreditName(artist_id: &'p str, name_key: &'p str, name: &'p str, created_at: &'p str),
    r#"
    INSERT INTO artist_credits (artist_id, name_key, name, created_at)
    VALUES (:artist_id, :name_key, :name, :created_at)
    ON CONFLICT (artist_id, name_key) DO NOTHING
"#
);

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
    conn.run(&KeepCreditNameParams {
        artist_id,
        name_key: &credit.key,
        name: &credit.name,
        created_at: now,
    })?;
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

sqlive_compose::write!(
    RelinkAlbumArtistsCreditName(artist_id: &'p str, from: &'p str, to: Option<&'p str>),
    "UPDATE album_artists SET credit_key = :to WHERE artist_id = :artist_id AND credit_key = :from"
);

sqlive_compose::write!(
    RelinkTrackArtistsCreditName(artist_id: &'p str, from: &'p str, to: Option<&'p str>),
    "UPDATE track_artists SET credit_key = :to WHERE artist_id = :artist_id AND credit_key = :from"
);

sqlive_compose::write!(
    RelinkReleaseArtistRolesCreditName(artist_id: &'p str, from: &'p str, to: Option<&'p str>),
    "UPDATE release_artist_roles SET credit_key = :to WHERE artist_id = :artist_id AND credit_key = :from"
);

sqlive_compose::write!(
    RelinkTrackArtistRolesCreditName(artist_id: &'p str, from: &'p str, to: Option<&'p str>),
    "UPDATE track_artist_roles SET credit_key = :to WHERE artist_id = :artist_id AND credit_key = :from"
);

sqlive_compose::write!(
    RelinkWorkArtistsCreditName(artist_id: &'p str, from: &'p str, to: Option<&'p str>),
    "UPDATE work_artists SET credit_key = :to WHERE artist_id = :artist_id AND credit_key = :from"
);

/// Point every link naming `artist_id`'s credit name `from` at `to` (`None`:
/// the artist's own name).
fn relink_credit_name(
    conn: &CovenWrite<'_>,
    artist_id: &str,
    from: &str,
    to: Option<&str>,
) -> Result<(), DbError> {
    conn.run(&RelinkAlbumArtistsCreditNameParams { artist_id, from, to })?;
    conn.run(&RelinkTrackArtistsCreditNameParams { artist_id, from, to })?;
    conn.run(&RelinkReleaseArtistRolesCreditNameParams { artist_id, from, to })?;
    conn.run(&RelinkTrackArtistRolesCreditNameParams { artist_id, from, to })?;
    conn.run(&RelinkWorkArtistsCreditNameParams { artist_id, from, to })?;
    Ok(())
}

// The other printed names of the artist the parent row shows as, those of the
// artists merged into it among them, in name order in the device's language,
// ties going by key, as a JSON array.
sqlive_compose::fragment!(
    OtherNames on artists {
        struct OtherName { artist_id: String, key: String, name: String }
        names: json<Vec<OtherName>>,
    },
    r#"
    SELECT json_group_array(json_object('artist_id', credit_name.artist_id,
                                        'key', credit_name.name_key,
                                        'name', credit_name.name)
                            ORDER BY bae_name_key(credit_name.name, (
                                SELECT sort.language
                                FROM @crate::records::name_sort::NameSortLanguage AS sort)),
                                     credit_name.name_key) AS names
    FROM artist_shown_as AS shown
    JOIN artist_credits AS credit_name ON credit_name.artist_id = shown.artist_id
    WHERE shown.shown_artist_id = @parent.id
"#
);

// Each artist's other printed names ([`OtherNames`]) and how many they are, as
// the artist's page lists them.
sqlive_compose::fragment!(
    ArtistOtherNames { name_count: u64 },
    r#"
    SELECT artist.id AS names_artist_id,
           @crate::records::artist_credit_names::OtherNames(parent: artist) AS other_names,
           json_array_length(
               @crate::records::artist_credit_names::OtherNames(parent: artist)) AS name_count
    FROM artists AS artist
"#
);

impl ArtistOtherNames {
    /// The names, and how many they are.
    pub fn into_names(self) -> (Vec<ArtistCreditName>, u64) {
        let names = self
            .other_names
            .names
            .into_iter()
            .map(|name| ArtistCreditName {
                artist_id: name.artist_id,
                credit: CreditName { key: name.key, name: name.name },
            })
            .collect();
        (names, self.name_count)
    }
}

// [`ArtistOtherNames`] of the artist `artist`, which a test reads.
#[cfg(test)]
sqlive_compose::read!(
    writer OtherNamesOfArtist(artist: String),
    r#"
    SELECT listed.*
    FROM @crate::records::artist_credit_names::ArtistOtherNames AS listed
    WHERE listed.names_artist_id = :artist
"#
);

sqlive_compose::write!(
    SpellCreditName(artist_id: &'p str, name_key: &'p str, name: &'p str),
    "UPDATE artist_credits SET name = :name WHERE artist_id = :artist_id AND name_key = :name_key"
);

sqlive_compose::write!(
    DeleteCreditName(artist_id: &'p str, name_key: &'p str),
    "DELETE FROM artist_credits WHERE artist_id = :artist_id AND name_key = :name_key"
);

impl Records {
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
                    sql.run(&SpellCreditNameParams {
                        artist_id: &artist,
                        name_key: &key,
                        name: &renamed.name,
                    })?;
                    return Ok(true);
                }
                let to = settle_credit_name_beside(&sql, &artist, &own, &renamed, &now)?;
                relink_credit_name(&sql, &artist, &key, to.as_deref())?;
                sql.run(&DeleteCreditNameParams { artist_id: &artist, name_key: &key })?;
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
                sql.run(&DeleteCreditNameParams { artist_id: &artist_id, name_key: &key })?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "artist_credit_names_tests.rs"]
mod tests;
