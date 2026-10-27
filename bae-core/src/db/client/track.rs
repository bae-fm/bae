use super::*;

impl Database {
    /// Write one medium row of a release, for a test that builds a release row
    /// by row.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release_medium(&self, medium: &DbReleaseMedium) -> Result<(), DbError> {
        let medium = medium.clone();
        self.call_sql(move |sql| insert_release_medium_row(&sql, &medium))
            .await
    }

    pub async fn insert_track(&self, track: &DbTrack) -> Result<(), DbError> {
        let track = track.clone();
        self.call_sql(move |sql| insert_track_row(&sql, &track))
            .await
    }

    /// A release's mediums, in order.
    pub async fn get_release_mediums(&self, release_id: &str) -> Result<DbReleaseMediums, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| release_mediums_on(&sql, &release_id))
            .await
    }

    /// Find track by ID. Caller-provided ID — may not exist.
    pub async fn find_track_by_id(&self, track_id: &str) -> Result<Option<DbTrack>, DbError> {
        let track_id = track_id.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT * FROM tracks WHERE id = ?",
                params![track_id],
                row_to_track,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
    /// Ordered track IDs for a release — cheaper than `get_tracks_for_release`
    /// when the caller only needs IDs (queue building, play context).
    pub async fn get_track_ids_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<String>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query(
                "SELECT id FROM tracks WHERE release_id = ? ORDER BY position, id",
                params![release_id],
                |row| row.get::<_, String>("id"),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Every track id in the library, in a deterministic base order (the same
    /// order across calls, so a listing pages through a stable list): by
    /// release, then stored position, id — the order the playback queue's
    /// library play unshuffles into.
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, DbError> {
        self.read(move |sql| {
            sql.query(
                "SELECT id FROM tracks ORDER BY release_id, position, id",
                [],
                |row| row.get::<_, String>("id"),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// The subset of `track_ids` that exist in the tracks table. The returned
    /// order is unspecified; a caller that needs one must re-derive it.
    pub async fn filter_existing_track_ids(
        &self,
        track_ids: &[String],
    ) -> Result<Vec<String>, DbError> {
        if track_ids.is_empty() {
            return Ok(Vec::new());
        }
        let track_ids = track_ids.to_vec();
        self.read(move |sql| {
            let mut existing = Vec::new();
            for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let query = format!("SELECT id FROM tracks WHERE id IN ({placeholders})");
                existing.extend(sql.query(
                    &query,
                    rusqlite::params_from_iter(chunk.iter()),
                    |row| row.get::<_, String>("id"),
                )?);
            }
            Ok(existing)
        })
        .await
    }

    /// Ordered by the release’s stored track positions.
    pub async fn get_tracks_for_release(&self, release_id: &str) -> Result<Vec<DbTrack>, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query(
                "SELECT * FROM tracks WHERE release_id = ? ORDER BY position, id",
                params![release_id],
                row_to_track,
            )
            .map_err(DbError::from)
        })
        .await
    }
    /// Find audio format by track ID. Caller-provided ID — may not exist.
    pub async fn find_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, DbError> {
        let track_id = track_id.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT * FROM audio_formats WHERE track_id = ?",
                params![track_id],
                row_to_audio_format,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    pub async fn get_audio_segments_for_format(
        &self,
        audio_format_id: &str,
    ) -> Result<Vec<DbAudioSegment>, DbError> {
        let audio_format_id = audio_format_id.to_string();
        self.read(move |sql| {
            sql.query(
                "SELECT * FROM audio_format_segments \
                 WHERE audio_format_id = ? \
                 ORDER BY segment_index",
                params![audio_format_id],
                row_to_audio_segment,
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// The scrub points of every segment of an audio format that has any,
    /// in segment order, each segment's in sample order. A stored blob that
    /// doesn't decode to points is a damaged database.
    pub async fn get_scrub_points_for_format(
        &self,
        audio_format_id: &str,
    ) -> Result<Vec<DbScrubPoints>, DbError> {
        let audio_format_id = audio_format_id.to_string();
        self.read(move |sql| {
            let rows: Vec<(String, Vec<u8>)> = sql
                .query(
                    "SELECT p.audio_format_segment_id, p.points \
                     FROM audio_segment_scrub_points p \
                     JOIN audio_format_segments s ON s.id = p.audio_format_segment_id \
                     WHERE s.audio_format_id = ? \
                     ORDER BY s.segment_index",
                    params![audio_format_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(DbError::from)?;
            rows.into_iter()
                .map(|(audio_segment_id, blob)| {
                    Ok(DbScrubPoints {
                        audio_segment_id,
                        points: super::scrub_points_column::decode(&blob)?,
                    })
                })
                .collect()
        })
        .await
    }
}
