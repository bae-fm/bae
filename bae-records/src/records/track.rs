use super::*;

// Each track the library holds among those the JSON array `ids` names.
sqlive_compose::read!(
    HeldTrackIds(ids: String),
    r#"
    SELECT track.id
    FROM tracks AS track
    WHERE track.id IN (SELECT asked.value FROM json_each(:ids) AS asked)
"#
);

// The tracks the JSON array `ids` of album and track ids names, in the
// array's order: an album's are the tracks of its primary release, in playing
// order, and a track's is itself. A row with no `track_id` is for an album
// whose primary release has no track, `album_id` naming it, or for an id of
// no album and no track.
sqlive_compose::read!(
    NamedTrackIds(ids: String) {
        asked_id: Option<String>,
        track_id: Option<String>,
    },
    r#"
    SELECT asked.value AS asked_id, album.id AS album_id,
           COALESCE(held.id, lone.id) AS track_id
    FROM json_each(:ids) AS asked
    LEFT JOIN albums AS album ON album.id = asked.value
    LEFT JOIN tracks AS held
           ON held.release_id =
              @crate::records::fragments::AlbumPrimaryRelease(parent: album)
    LEFT JOIN tracks AS lone ON lone.id = asked.value AND album.id IS NULL
    ORDER BY asked.key, held.position, held.id
"#
);

/// An id that names no album and no track the library holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownTrackSource(pub String);

impl Records {
    /// The track ids that `ids`, of albums and tracks, name, in their order:
    /// an album's are the tracks of its primary release, none when it has no
    /// release or that release no track. The first id of no album and no
    /// track the library holds is the answer instead.
    pub async fn track_ids_of_albums_and_tracks(
        &self,
        ids: &[String],
    ) -> Result<Result<Vec<String>, UnknownTrackSource>, DbError> {
        let statement = NamedTrackIdsParams { ids: serde_json::json!(ids).to_string() };
        let rows = self.store.read_rows(statement).await?;
        let mut track_ids = Vec::with_capacity(rows.len());
        for row in rows {
            match (row.track_id, row.album_id) {
                (Some(track_id), _) => track_ids.push(track_id),
                (None, Some(_album_without_tracks)) => {}
                (None, None) => {
                    let id = row.asked_id.ok_or_else(|| {
                        DbError::unreadable("a list of ids", "an entry that is NULL")
                    })?;
                    return Ok(Err(UnknownTrackSource(id)));
                }
            }
        }
        Ok(Ok(track_ids))
    }

    /// Write one medium row of a release, for a test that builds a release row
    /// by row.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release_medium(&self, medium: &DbReleaseMedium) -> Result<(), DbError> {
        let medium = medium.clone();
        self.store.call_sql(move |sql| insert_release_medium_row(&sql, &medium)).await
    }

    pub async fn insert_track(&self, track: &DbTrack) -> Result<(), DbError> {
        let track = track.clone();
        self.store.call_sql(move |sql| insert_track_row(&sql, &track)).await
    }

    /// A release's mediums, in order, for a test that reads a release's
    /// rows back.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_release_mediums(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbReleaseMedium>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| release_mediums_on(&sql, &release_id)).await
    }

    /// Each track of `release_id` in playing order, with how a surface
    /// outside bae numbers it.
    pub async fn get_numbered_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbNumberedTrack>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                super::export_numbers::numbered_tracks_of_release_on(&sql, &release_id)
            })
            .await
    }

    /// The track `track_id`, with how a surface outside bae numbers it, or
    /// `None` when the library holds no such track.
    pub async fn get_numbered_track(
        &self,
        track_id: &str,
    ) -> Result<Option<DbNumberedTrack>, DbError> {
        let track_id = track_id.to_string();
        self.store.read(move |sql| super::export_numbers::numbered_track_on(&sql, &track_id)).await
    }

    /// Where each of `track_ids` sits on its release, in their order, leaving
    /// out a track the library does not hold.
    pub async fn track_places(&self, track_ids: &[String]) -> Result<Vec<DbTrackPlace>, DbError> {
        let track_ids = track_ids.to_vec();
        self.store.read(move |sql| super::release_tracks::track_places_on(&sql, &track_ids)).await
    }

    /// Find track by ID. Caller-provided ID — may not exist.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_track_by_id(&self, track_id: &str) -> Result<Option<DbTrack>, DbError> {
        let track_id = track_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row("SELECT * FROM tracks WHERE id = ?", params![track_id], row_to_track)
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
        self.store
            .read(move |sql| {
                sql.query(
                    "SELECT id FROM tracks WHERE release_id = ? ORDER BY position, id",
                    params![release_id],
                    |row| row.get::<_, String>("id"),
                )
                .map_err(DbError::from)
            })
            .await
    }

    /// The tracks from `anchor_id` to `target_id`, both included, in the
    /// order their release lists them: what a shift-click on `target_id`
    /// picks after a pick of `anchor_id`. The function returns only the
    /// target when the anchor is not a track of the target's release, and an
    /// empty list when the library holds no track `target_id`.
    pub async fn track_ids_between(
        &self,
        anchor_id: String,
        target_id: String,
    ) -> Result<Vec<String>, DbError> {
        self.store
            .read(move |sql| {
                sql.query(
                    "SELECT id FROM ( \
                         SELECT t.id, t.position FROM tracks AS t \
                         JOIN tracks AS target ON target.id = ?2 \
                         JOIN tracks AS anchor \
                           ON anchor.id = ?1 AND anchor.release_id = target.release_id \
                         WHERE t.release_id = target.release_id \
                           AND ((t.position, t.id) BETWEEN (anchor.position, anchor.id) \
                                                       AND (target.position, target.id) \
                             OR (t.position, t.id) BETWEEN (target.position, target.id) \
                                                       AND (anchor.position, anchor.id)) \
                         UNION ALL \
                         SELECT target.id, target.position FROM tracks AS target \
                         WHERE target.id = ?2 \
                           AND NOT EXISTS (SELECT 1 FROM tracks AS anchor \
                                           WHERE anchor.id = ?1 \
                                             AND anchor.release_id = target.release_id)) \
                     ORDER BY position, id",
                    params![anchor_id, target_id],
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
        self.store
            .read(move |sql| {
                sql.query("SELECT id FROM tracks ORDER BY release_id, position, id", [], |row| {
                    row.get::<_, String>("id")
                })
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
        let statement = HeldTrackIdsParams { ids: serde_json::json!(track_ids).to_string() };
        Ok(self.store.read_rows(statement).await?.into_iter().map(|row| row.id).collect())
    }

    /// Ordered by the release’s stored track positions.
    pub async fn get_tracks_for_release(&self, release_id: &str) -> Result<Vec<DbTrack>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
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
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, DbError> {
        let track_id = track_id.to_string();
        self.store
            .read(move |sql| {
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

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_audio_segments_for_format(
        &self,
        audio_format_id: &str,
    ) -> Result<Vec<DbAudioSegment>, DbError> {
        let audio_format_id = audio_format_id.to_string();
        self.store
            .read(move |sql| {
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
}

#[cfg(test)]
#[path = "track_tests.rs"]
mod tests;
