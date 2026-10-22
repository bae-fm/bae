//! The library album a person picked for a candidate to join
//! (`import_candidate_album_pick`), over the one its records name.

use super::*;

/// The album picked for the candidate `content_hash`, if one is.
pub(crate) fn load_album_pick_on(
    sql: &impl super::super::query::QueryOne,
    content_hash: &str,
) -> Result<Option<String>, DbError> {
    sql.query_row(
        "SELECT album_id FROM import_candidate_album_pick WHERE content_hash = ?",
        params![content_hash],
        |row| row.get(0),
    )
    .optional()
    .map_err(DbError::from)
}

impl Database {
    /// The album picked for the candidate `content_hash`, if one is.
    pub(crate) async fn candidate_album_pick(
        &self,
        content_hash: &str,
    ) -> Result<Option<String>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_album_pick_on(&sql, &content_hash))
            .await
    }

    /// Pick `album_id` for the candidate `content_hash` to join, or (`None`)
    /// leave it to the album its records name. An album no longer in the
    /// library is refused by its foreign key.
    pub(crate) async fn set_candidate_album_pick(
        &self,
        content_hash: &str,
        album_id: Option<&str>,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let album_id = album_id.map(str::to_string);
        self.call_sql(move |sql| {
            match &album_id {
                Some(album_id) => sql.execute(
                    "INSERT INTO import_candidate_album_pick (content_hash, album_id) \
                     VALUES (?, ?) \
                     ON CONFLICT (content_hash) DO UPDATE SET album_id = excluded.album_id",
                    params![content_hash, album_id],
                )?,
                None => sql.execute(
                    "DELETE FROM import_candidate_album_pick WHERE content_hash = ?",
                    params![content_hash],
                )?,
            };
            Ok(())
        })
        .await
    }
}
