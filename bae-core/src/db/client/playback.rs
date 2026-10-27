use super::*;

impl Database {
    /// Follow the display of the track `initial` names, `None` when it names
    /// none or the library no longer holds it. The playing track changes by
    /// pointing the same query at the new track through its request handle.
    pub(crate) fn subscribe_track_display(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<crate::playback::TrackDisplay>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |track_id, sql| match track_id {
                Some(track_id) => track_display_on(&sql, track_id).map_err(CovenError::from),
                None => Ok(None),
            })
            .process(Ok)
    }

    /// Follow the side facts of the tracks `initial` names, in its order,
    /// leaving out a track the library no longer holds. The playback service
    /// points it at the tracks whose crossing it has staged.
    pub(crate) fn subscribe_playback_track_infos(
        &self,
        initial: Vec<String>,
    ) -> coven::ReconfigurableLiveQuery<Vec<String>, Vec<crate::playback::PlaybackTrackInfo>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |track_ids, sql| {
                let mut infos = Vec::with_capacity(track_ids.len());
                for track_id in track_ids {
                    let track = sql
                        .query_row(
                            "SELECT * FROM tracks WHERE id = ?",
                            params![track_id],
                            row_to_track,
                        )
                        .optional()
                        .map_err(DbError::from)?;
                    let Some(track) = track else { continue };
                    let mediums = release_mediums_on(&sql, &track.release_id)?;
                    infos.push(crate::playback::PlaybackTrackInfo::of(&track, &mediums));
                }
                Ok(infos)
            })
            .process(Ok)
    }

    /// The display of `track_id` as the library holds it now, or `None` when it
    /// no longer holds the track.
    pub async fn track_display(
        &self,
        track_id: &str,
    ) -> Result<Option<crate::playback::TrackDisplay>, DbError> {
        let track_id = track_id.to_string();
        self.read(move |sql| track_display_on(&sql, &track_id))
            .await
    }

    /// Write the single device-local `playback_state` row (id = 'current'),
    /// replacing any existing one. Never synced.
    pub async fn save_playback_state(&self, state: &DbPlaybackState) -> Result<(), DbError> {
        let state = state.clone();
        self.call(move |conn| {
            let (current_entry, current_track_id) = match &state.current {
                Some(current) => (Some(current.id.to_string()), Some(current.track_id.clone())),
                None => (None, None),
            };
            conn.execute(
                "INSERT OR REPLACE INTO playback_state \
                     (id, current_entry, current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', ?, ?, ?, ?, ?)",
                params![
                    current_entry,
                    current_track_id,
                    state.position_ms.map(|position| position as i64),
                    state.volume,
                    state.is_muted,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Read the device-local `playback_state` row, `None` when none is stored.
    pub async fn load_playback_state(&self) -> Result<Option<DbPlaybackState>, DbError> {
        self.read(move |sql| {
            Ok(sql
                .query_row(
                    "SELECT current_entry, current_track_id, position_ms, volume, is_muted \
                     FROM playback_state WHERE id = 'current'",
                    [],
                    |row| {
                        let entry: Option<String> = row.get("current_entry")?;
                        let track_id: Option<String> = row.get("current_track_id")?;
                        // The schema keeps `position_ms` non-negative.
                        let position_ms: Option<i64> = row.get("position_ms")?;
                        let current = match entry.zip(track_id) {
                            Some((entry, track_id)) => Some(crate::playback::QueueEntry {
                                id: entry.parse().map_err(|e: String| {
                                    rusqlite::Error::FromSqlConversionFailure(
                                        0,
                                        rusqlite::types::Type::Text,
                                        e.into(),
                                    )
                                })?,
                                track_id,
                            }),
                            None => None,
                        };
                        Ok(DbPlaybackState {
                            current,
                            position_ms: position_ms.map(|position| position as u64),
                            volume: row.get("volume")?,
                            is_muted: row.get("is_muted")?,
                        })
                    },
                )
                .optional()?)
        })
        .await
    }

    /// Delete the device-local `playback_state` row (playback stopped).
    ///
    /// Stopping is persisted on every queue change and every stop, so most
    /// calls arrive with the row already gone. That case is read, not written:
    /// a delete over an empty table is a transaction that changes nothing, and
    /// those belong off the sync journal.
    pub async fn clear_playback_state(&self) -> Result<(), DbError> {
        let stored = self
            .read(move |sql| {
                Ok(sql
                    .query_row("SELECT 1 FROM playback_state", [], |_| Ok(()))
                    .optional()?
                    .is_some())
            })
            .await?;
        if !stored {
            return Ok(());
        }
        self.call(move |conn| {
            conn.execute("DELETE FROM playback_state", [])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }
}

/// A track's display and its catalog duration, as the queue rows show it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TrackQueueMeta {
    pub(super) display: crate::playback::TrackDisplay,
    pub(super) duration_ms: u64,
}

/// The joins from `tracks t` that [`track_display_columns`] reads: the
/// release the track is on, its album, and that release's cover.
pub(super) const TRACK_DISPLAY_JOINS: &str = "JOIN releases r ON r.id = t.release_id \
     JOIN albums a ON a.id = r.album_id \
     LEFT JOIN covers c ON c.id = r.id";

/// The columns of a track's display over `tracks t` and
/// [`TRACK_DISPLAY_JOINS`], which [`track_queue_meta_from_row`] reads: its
/// title, its credited artists (the album's when the track credits none), its
/// album, the release it is on, and that release's cover. The cover is the
/// track's own release's, not the album's primary release's, so a track from
/// a non-primary release shows that release's art; its versioned reference is
/// what the UI caches art under, and a release with no cover row has none.
pub(super) fn track_display_columns() -> String {
    format!(
        "t.id AS track_id, t.title, t.duration_ms, a.id AS album_id, \
         a.title AS album_title, r.id AS release_id, c.revision AS cover_revision, \
         c.hash AS cover_hash, {artist_names} AS artist_names",
        artist_names = track_credits_json_sql("t"),
    )
}

pub(super) fn track_queue_meta_from_row(row: &Row<'_>) -> rusqlite::Result<TrackQueueMeta> {
    let release_id: String = row.get("release_id")?;
    // A release's cover image is keyed by the release's id.
    let cover_image = match row.get::<_, Option<String>>("cover_revision")? {
        Some(revision) => Some(crate::album_detail::ImageRef {
            id: release_id.clone(),
            version: super::files::image_version(
                &revision,
                row.get::<_, Option<Vec<u8>>>("cover_hash")?.as_deref(),
            ),
            image_type: LibraryImageType::Cover,
        }),
        None => None,
    };
    Ok(TrackQueueMeta {
        display: crate::playback::TrackDisplay {
            title: row.get("title")?,
            artist_names: credit_line_column(row, "artist_names")?,
            album_id: row.get("album_id")?,
            release_id,
            album_title: row.get("album_title")?,
            cover_image,
        },
        duration_ms: row.get("duration_ms")?,
    })
}

/// The display of `track_id`, or `None` when the library no longer holds it.
fn track_display_on(
    sql: &SqlReadContext<'_>,
    track_id: &str,
) -> Result<Option<crate::playback::TrackDisplay>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "SELECT {columns} FROM tracks t {joins} WHERE t.id = ?",
                columns = track_display_columns(),
                joins = TRACK_DISPLAY_JOINS,
            ),
            [track_id],
            track_queue_meta_from_row,
        )
        .optional()?
        .map(|meta| meta.display))
}
