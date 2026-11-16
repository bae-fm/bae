//! The library album a person picked for a candidate to join
//! (`import_candidate_album_pick`), over the one its records name.
//! Its table is `album_pick_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("album_pick_rows.sql");

use super::*;

/// The album the person picked for the candidate `content_hash`: a library
/// album, a new album, or, when they picked none, the album its records name.
pub(crate) fn load_album_pick_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<crate::AlbumPick, DbError> {
    let row: Option<Option<String>> = sql
        .query_row(
            "SELECT album_id FROM import_candidate_album_pick WHERE content_hash = ?",
            params![content_hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match row {
        None => crate::AlbumPick::Records,
        Some(None) => crate::AlbumPick::New,
        Some(Some(album_id)) => crate::AlbumPick::Album { album_id },
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
