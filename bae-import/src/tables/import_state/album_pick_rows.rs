//! The library album a person picked for a candidate to join
//! (`import_candidate_album_pick`), over the one its records name.
//! Its table is `album_pick_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("album_pick_rows.sql");

use super::*;

/// The statement that reads what the person picked for the album of the
/// candidate whose content hash the SQL expression `content_hash` gives. It
/// returns one row. Its `pick` is 'records' when the person picked nothing,
/// 'new' when they picked a new album, and 'album' when they picked the
/// library album `album_id`.
pub(crate) fn album_pick_sql(content_hash: &str) -> String {
    format!(
        "SELECT CASE WHEN pick.content_hash IS NULL THEN 'records' \
                     WHEN pick.album_id IS NULL THEN 'new' \
                     ELSE 'album' END AS pick, \
                pick.album_id AS album_id \
         FROM (SELECT 1) \
         LEFT JOIN import_candidate_album_pick AS pick ON pick.content_hash = {content_hash}"
    )
}

/// The album the person picked for the candidate `content_hash`: a library
/// album, a new album, or, when they picked none, the album its records name.
pub(crate) fn load_album_pick_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<crate::AlbumPick, DbError> {
    let (pick, album_id): (String, Option<String>) =
        sql.query_row(&album_pick_sql("?"), params![content_hash], |row| {
            Ok((row.get("pick")?, row.get("album_id")?))
        })?;
    Ok(match (pick.as_str(), album_id) {
        ("records", None) => crate::AlbumPick::Records,
        ("new", None) => crate::AlbumPick::New,
        ("album", Some(album_id)) => crate::AlbumPick::Album { album_id },
        (pick, album_id) => {
            return Err(DbError::unreadable(
                "a candidate's album pick",
                (pick.to_string(), album_id),
            ))
        }
    })
}

/// Leave the candidate `content_hash` to the album its records name.
pub(super) fn delete_album_pick(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_album_pick WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

impl ImportTables {
    /// The album the person picked for the candidate `content_hash`: a
    /// library album, a new album, or, when they picked none, the album its
    /// records name.
    pub(crate) async fn candidate_album_pick(
        &self,
        content_hash: &str,
    ) -> Result<crate::AlbumPick, DbError> {
        let content_hash = content_hash.to_string();
        self.store.read(move |sql| load_album_pick_on(&sql, &content_hash)).await
    }

    /// Record `pick` for the candidate `content_hash`. The table's foreign key
    /// refuses an album the library no longer holds.
    pub(crate) async fn set_candidate_album_pick(
        &self,
        content_hash: &str,
        pick: crate::AlbumPick,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .call_sql(move |sql| {
                let album_id = match pick {
                    crate::AlbumPick::Records => return delete_album_pick(&sql, &content_hash),
                    crate::AlbumPick::New => None,
                    crate::AlbumPick::Album { album_id } => Some(album_id),
                };
                sql.execute(
                    "INSERT INTO import_candidate_album_pick (content_hash, album_id) \
                     VALUES (?, ?) \
                     ON CONFLICT (content_hash) DO UPDATE SET album_id = excluded.album_id",
                    params![content_hash, album_id],
                )?;
                Ok(())
            })
            .await
    }
}
