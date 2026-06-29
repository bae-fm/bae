use super::*;

impl Database {
    /// Insert a new artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                r#"
                    INSERT INTO artists (
                        id, name, sort_name, discogs_artist_id,
                        musicbrainz_artist_id,
                        _updated_at, created_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?)
                    "#,
                params![
                    artist.id,
                    artist.name,
                    artist.sort_name,
                    artist.discogs_artist_id,
                    artist.musicbrainz_artist_id,
                    reg,
                    artist.created_at.to_rfc3339(),
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }
    /// Look up a single artist by a one-parameter equality query. The four
    /// `get_artist_by_*` / `find_artist_by_id` lookups differ only in which
    /// column they match on, so they share this body.
    async fn get_artist_by_sql(
        &self,
        sql: &'static str,
        value: String,
    ) -> Result<Option<DbArtist>, DbError> {
        self.call(move |conn| {
            conn.query_row(sql, params![value], |row| Ok(Self::row_to_artist(row)))
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// Get artist by Discogs artist ID (for deduplication)
    pub async fn get_artist_by_discogs_id(
        &self,
        discogs_artist_id: &str,
    ) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql(
            "SELECT * FROM artists WHERE discogs_artist_id = ?",
            discogs_artist_id.to_string(),
        )
        .await
    }

    /// Get artist by MusicBrainz artist ID (for deduplication)
    pub async fn get_artist_by_mb_id(&self, mb_id: &str) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql(
            "SELECT * FROM artists WHERE musicbrainz_artist_id = ?",
            mb_id.to_string(),
        )
        .await
    }

    /// Get artist by name (case-insensitive, first match)
    pub async fn get_artist_by_name(&self, name: &str) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql(
            "SELECT * FROM artists WHERE name = ? COLLATE NOCASE LIMIT 1",
            name.to_string(),
        )
        .await
    }

    /// Fill in NULL external IDs on an existing artist via COALESCE (never overwrites).
    /// Also updates sort_name if currently NULL.
    pub async fn update_artist_external_ids(
        &self,
        id: &str,
        discogs_id: Option<&str>,
        mb_id: Option<&str>,
        sort_name: Option<&str>,
    ) -> Result<(), DbError> {
        let (id, discogs_id, mb_id, sort_name) = (
            id.to_string(),
            discogs_id.map(str::to_string),
            mb_id.map(str::to_string),
            sort_name.map(str::to_string),
        );
        let reg = self.register_stamp().await?;
        self.call(move |conn| {
            conn.execute(
                r#"
                    UPDATE artists SET
                        discogs_artist_id = COALESCE(discogs_artist_id, ?),
                        musicbrainz_artist_id = COALESCE(musicbrainz_artist_id, ?),
                        sort_name = COALESCE(sort_name, ?),
                        _updated_at = ?
                    WHERE id = ?
                    "#,
                params![discogs_id, mb_id, sort_name, reg, id],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_album_artist_row(conn, &album_artist, &reg))
            .await
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        let reg = self.register_stamp().await?;
        self.call(move |conn| insert_track_artist_row(conn, &track_artist, &reg))
            .await
    }
    /// Get artists for an album (ordered by position)
    pub async fn get_artists_for_album(&self, album_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let album_id = album_id.to_string();
        self.call(move |conn| {
            // Primary artist from FK (sort_key = -1 so it's first),
            // then additional artists from junction table ordered by position.
            let mut stmt = conn.prepare(
                r#"
                        SELECT a.*, -1 AS sort_key FROM artists a
                        JOIN albums alb ON alb.artist_id = a.id
                        WHERE alb.id = ?
                        UNION ALL
                        SELECT a.*, aa.position AS sort_key FROM artists a
                        JOIN album_artists aa ON a.id = aa.artist_id
                        WHERE aa.album_id = ?
                        ORDER BY sort_key
                        "#,
            )?;
            let rows = stmt.query_map(params![album_id, album_id], |row| {
                Ok(Self::row_to_artist(row))
            })?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Get artists for a track (ordered by position)
    pub async fn get_artists_for_track(&self, track_id: &str) -> Result<Vec<DbArtist>, DbError> {
        let track_id = track_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                r#"
                        SELECT a.* FROM artists a
                        JOIN track_artists ta ON a.id = ta.artist_id
                        WHERE ta.track_id = ?
                        ORDER BY ta.position
                        "#,
            )?;
            let rows = stmt.query_map(params![track_id], |row| Ok(Self::row_to_artist(row)))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Find artist by ID. Caller-provided ID — may not exist.
    pub async fn find_artist_by_id(&self, artist_id: &str) -> Result<Option<DbArtist>, DbError> {
        self.get_artist_by_sql("SELECT * FROM artists WHERE id = ?", artist_id.to_string())
            .await
    }
}
