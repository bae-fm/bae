use super::*;

impl Database {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        self.inner.store.call_sql(move |sql| insert_artist_row(&sql, &artist)).await
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        self.inner.store.call_sql(move |sql| insert_album_artist_row(&sql, &album_artist)).await
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        self.inner.store.call_sql(move |sql| insert_track_artist_row(&sql, &track_artist)).await
    }
    /// The album's credits in list order, each the artist it shows after
    /// merges; a list naming one artist twice gives it twice.
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let album_id = album_id.to_string();
        self.inner.store.read(move |sql| get_artists_for_album_on(&sql, &album_id)).await
    }
    /// The credits the track shows, in list order: its own, else its
    /// album's, each the artist it shows after merges. The rule is
    /// [`super::query::track_credits_json_sql`]'s, which every read of a
    /// track's shown credits goes through. None for an unknown track.
    pub async fn get_shown_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let track_id = track_id.to_string();
        self.inner
            .store
            .read(move |sql| {
                let credits: Option<String> = sql
                    .query_row(
                        &format!(
                            "SELECT {} FROM tracks t \
                         JOIN releases r ON r.id = t.release_id \
                         JOIN albums a ON a.id = r.album_id \
                         WHERE t.id = ?",
                            super::query::track_credits_json_sql("t")
                        ),
                        params![track_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                credits.map_or(Ok(Vec::new()), |json| super::read::credits_from_json(&json))
            })
            .await
    }
    /// Find artist by ID. Caller-provided ID — may not exist.
    pub async fn find_artist_by_id(&self, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
        let artist_id = artist_id.to_string();
        self.inner
            .store
            .read(move |sql| {
                sql.query_row(
                    "SELECT * FROM artists WHERE id = ?",
                    params![artist_id],
                    row_to_artist,
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }
}
