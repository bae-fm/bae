use super::*;

impl Database {
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), DbError> {
        let artist = artist.clone();
        self.call_sql(move |sql| insert_artist_row(&sql, &artist)).await
    }
    pub async fn insert_album_artist(&self, album_artist: &DbAlbumArtist) -> Result<(), DbError> {
        let album_artist = album_artist.clone();
        self.call_sql(move |sql| insert_album_artist_row(&sql, &album_artist)).await
    }
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_track_artist(&self, track_artist: &DbTrackArtist) -> Result<(), DbError> {
        let track_artist = track_artist.clone();
        self.call_sql(move |sql| insert_track_artist_row(&sql, &track_artist)).await
    }
    /// The album's credits in list order, each the artist it shows after
    /// merges; a list naming one artist twice gives it twice.
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<LibraryCredit>, DbError> {
        let album_id = album_id.to_string();
        self.read(move |sql| get_artists_for_album_on(&sql, &album_id)).await
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
        self.read(move |sql| {
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
        self.read(move |sql| {
            sql.query_row("SELECT * FROM artists WHERE id = ?", params![artist_id], row_to_artist)
                .optional()
                .map_err(DbError::from)
        })
        .await
    }

    /// The `works` row id minted for a MusicBrainz work, if this library already
    /// imported it. Works are deduped on their MBID, never on their row id.
    pub async fn find_work_id_by_musicbrainz_id(
        &self,
        musicbrainz_work_id: &str,
    ) -> Result<Option<String>, DbError> {
        let musicbrainz_work_id = musicbrainz_work_id.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT id FROM works WHERE id = ?1 OR musicbrainz_work_id = ?2 \
                 ORDER BY id = ?1 DESC LIMIT 1",
                params![crate::identity::work_id(&musicbrainz_work_id), musicbrainz_work_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Each of `works` resolved to the `works` row already holding its
    /// MusicBrainz id, or to a row for an import's commit to insert under the
    /// id that MusicBrainz id names.
    #[cfg(desktop)]
    pub async fn resolve_works(&self, works: &[crate::DbWork]) -> Result<ResolvedWorks, DbError> {
        let mut ids = Vec::with_capacity(works.len());
        let mut inserts = Vec::new();
        for work in works {
            match self.find_work_id_by_musicbrainz_id(&work.musicbrainz_work_id).await? {
                Some(existing_id) => ids.push(existing_id),
                None => {
                    // The work's id is its MusicBrainz id's, so every device
                    // that meets this work writes one row.
                    let work = crate::DbWork {
                        id: crate::identity::work_id(&work.musicbrainz_work_id),
                        ..work.clone()
                    };
                    ids.push(work.id.clone());
                    inserts.push(work);
                }
            }
        }
        Ok(ResolvedWorks { ids, inserts })
    }
}

/// Parsed works resolved against the library's `works` rows.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub struct ResolvedWorks {
    /// One id per input work, in input order, so a caller can zip it with the
    /// works to remap the parsed ids its work links carry.
    pub ids: Vec<String>,
    /// The works this library has not seen before, for the commit to insert.
    pub inserts: Vec<crate::DbWork>,
}
