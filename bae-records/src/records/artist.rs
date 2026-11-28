use super::*;

// The credits the track `track` shows
// ([`TrackCredits`](super::credit_fragments::TrackCredits)); no row when the
// library holds no such track.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer ShownCreditsOfTrack(track: String),
    r#"
    SELECT @crate::records::credit_fragments::TrackCredits(parent: track) AS shown
    FROM tracks AS track
    WHERE track.id = :track
"#
);

// The artist `artist`'s row; no row when the library holds no such artist.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer OneArtist(artist: String),
    r#"
    SELECT artist.*
    FROM @crate::records::browse_fragments::ArtistRecord AS artist
    WHERE artist.artist_id = :artist
"#
);

impl Records {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        self.store.call_sql(move |sql| insert_artist_row(&sql, &artist)).await
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        self.store.call_sql(move |sql| insert_album_artist_row(&sql, &album_artist)).await
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        self.store.call_sql(move |sql| insert_track_artist_row(&sql, &track_artist)).await
    }
    /// The album's credits in list order, each the artist it shows after
    /// merges; a list naming one artist twice gives it twice.
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let album_id = album_id.to_string();
        self.store.read(move |sql| get_artists_for_album_on(&sql, &album_id)).await
    }
    /// The credits the track shows, in list order: its own, else its
    /// album's, each the artist it shows after merges. The rule is
    /// [`super::query::track_credits_json_sql`]'s, which every read of a
    /// track's shown credits goes through. None for an unknown track.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_shown_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let track_id = track_id.to_string();
        let statement = ShownCreditsOfTrackParams { track: track_id };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        match row {
            Some(row) => {
                let shown = row.shown.ok_or_else(|| {
                    DbError::unreadable("a track's shown credits", None::<String>)
                })?;
                super::read::credits_from_json(&shown.credits)
            }
            None => Ok(Vec::new()),
        }
    }
    /// Find artist by ID. Caller-provided ID — may not exist.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_artist_by_id(&self, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
        let artist_id = artist_id.to_string();
        let statement = OneArtistParams { artist: artist_id };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        row.map(|row| row.artist.into_artist()).transpose()
    }
}
