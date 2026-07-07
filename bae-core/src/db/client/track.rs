use super::*;

impl Database {
    /// Insert a new track
    pub async fn insert_track(&self, track: &DbTrack) -> Result<(), DbError> {
        let track = track.clone();
        self.call_sql(move |sql| {
            let reg = sql.stamp();
            insert_track_row(sql.tx(), &track, &reg)
        })
        .await
    }

    /// Find track by ID. Caller-provided ID — may not exist.
    pub async fn find_track_by_id(&self, track_id: &str) -> Result<Option<DbTrack>, DbError> {
        let track_id = track_id.to_string();
        self.call(move |conn| {
            conn.query_row(
                "SELECT * FROM tracks WHERE id = ?",
                params![track_id],
                row_to_track,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
    /// Get ordered track IDs for a release. Cheaper than `get_tracks_for_release`
    /// when callers only need IDs (queue building, play context).
    pub async fn get_track_ids_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<String>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM tracks WHERE release_id = ? ORDER BY side, track_number, id",
            )?;
            let rows = stmt.query_map(params![release_id], |row| row.get::<_, String>("id"))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Every track id in the library, in a deterministic base order (the same
    /// order across calls so a shuffle seed permutes a stable list). Ordered to
    /// match the per-release order — by release, then side, track number, id — so
    /// the library and a single release agree on what "source order" means.
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, DbError> {
        self.call(move |conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM tracks ORDER BY release_id, side, track_number, id")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>("id"))?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// Return the subset of `track_ids` that exist in the tracks table.
    /// Ordering of returned IDs is unspecified; callers that need a
    /// specific order must re-derive it.
    pub async fn filter_existing_track_ids(
        &self,
        track_ids: &[String],
    ) -> Result<Vec<String>, DbError> {
        if track_ids.is_empty() {
            return Ok(Vec::new());
        }
        let track_ids = track_ids.to_vec();
        self.call(move |conn| {
            let mut existing = Vec::new();
            for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
                let placeholders = in_clause_placeholders(chunk.len());
                let query = format!("SELECT id FROM tracks WHERE id IN ({placeholders})");
                let mut stmt = conn.prepare(&query)?;
                let rows = stmt
                    .query_map(coven::rusqlite::params_from_iter(chunk.iter()), |row| {
                        row.get::<_, String>("id")
                    })?;
                existing.extend(rows.collect::<coven::rusqlite::Result<Vec<_>>>()?);
            }
            Ok(existing)
        })
        .await
    }

    /// Get tracks for a release
    pub async fn get_tracks_for_release(&self, release_id: &str) -> Result<Vec<DbTrack>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks WHERE release_id = ? ORDER BY side, track_number, id",
            )?;
            let rows = stmt.query_map(params![release_id], row_to_track)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
    /// Get every audio-format row for a release, joined through its tracks.
    /// One row per track; a single-file CUE rip yields many rows sharing one
    /// `file_id`. The resolver groups them by `file_id` to describe each audio
    /// file's format.
    pub async fn get_audio_formats_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbAudioFormat>, DbError> {
        let release_id = release_id.to_string();
        self.call(move |conn| get_audio_formats_for_release_on(conn, &release_id))
            .await
    }
    /// Find audio format by track ID. Caller-provided ID — may not exist.
    pub async fn find_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, DbError> {
        let track_id = track_id.to_string();
        self.call(move |conn| {
            conn.query_row(
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
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM audio_format_segments \
                 WHERE audio_format_id = ? \
                 ORDER BY segment_index",
            )?;
            let rows = stmt.query_map(params![audio_format_id], row_to_audio_segment)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }
}
