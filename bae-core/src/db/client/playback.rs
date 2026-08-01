use super::*;

impl Database {
    /// One `QueueItem` per entry, in order, each carrying the entry's per-instance
    /// id and its track's album/artist display metadata. A track queued twice
    /// resolves twice — the metadata is fetched once and joined onto every entry of
    /// that track. Entries whose track is not found are skipped.
    ///
    /// The cover is the track's own release's, not the album's primary release's,
    /// so a queued track from a non-primary release shows that release's art — the
    /// same rule `playback_info_from_track_release` applies to the playing track.
    /// Its `covers` row joins in here rather than in a second query, giving each
    /// entry the versioned reference the UI caches art under; a release with no
    /// cover row yields `None`.
    pub async fn get_queue_items(&self, entries: &[QueueEntry]) -> Result<Vec<QueueItem>, DbError> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let entries = entries.to_vec();
        self
            .read(move |sql| {
                let track_ids: Vec<String> =
                    entries.iter().map(|e| e.track_id.clone()).collect();
                let mut meta_by_track: HashMap<String, TrackQueueMeta> = HashMap::new();
                for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
                    let placeholders = in_clause_placeholders(chunk.len());
                    let query = format!(
                        "SELECT \
                            t.id AS track_id, \
                            t.title, \
                            t.duration_ms, \
                            a.title AS album_title, \
                            r.id AS cover_image_id, \
                            c._updated_at AS cover_version, \
                            COALESCE( \
                                NULLIF(( \
                                    SELECT GROUP_CONCAT(art.name, ', ' ORDER BY ta.position) \
                                    FROM track_artists ta \
                                    JOIN artists art ON art.id = ta.artist_id \
                                    WHERE ta.track_id = t.id \
                                ), ''), \
                                (SELECT art_primary.name FROM artists art_primary WHERE art_primary.id = a.artist_id) \
                            ) AS artist_names \
                        FROM tracks t \
                        JOIN releases r ON r.id = t.release_id \
                        JOIN albums a ON a.id = r.album_id \
                        LEFT JOIN covers c ON c.id = r.id \
                        WHERE t.id IN ({placeholders})"
                    );

                    meta_by_track.extend(sql.query(
                        &query,
                        coven::rusqlite::params_from_iter(chunk.iter()),
                        |row| {
                            let track_id: String = row.get("track_id")?;
                            let cover_image_id: String = row.get("cover_image_id")?;
                            let cover_version: Option<String> = row.get("cover_version")?;
                            Ok((
                                track_id,
                                TrackQueueMeta {
                                    title: row.get("title")?,
                                    artist_names: row.get("artist_names")?,
                                    duration_ms: row.get("duration_ms")?,
                                    album_title: row.get("album_title")?,
                                    cover_image: cover_version.map(|version| {
                                        crate::album_detail::ImageRef {
                                            id: cover_image_id,
                                            version,
                                            image_type: LibraryImageType::Cover,
                                        }
                                    }),
                                },
                            ))
                        },
                    )?);
                }

                Ok(resolve_queue_entries(&meta_by_track, &entries))
            })
            .await
    }

    /// Write the single device-local `playback_state` row (id = 'current'),
    /// replacing any existing one. Never synced.
    pub async fn save_playback_state(&self, state: &DbPlaybackState) -> Result<(), DbError> {
        let state = state.clone();
        self.call(move |conn| {
            // Flatten the context substruct back to the table's nullable
            // columns: all NULL when no context is playing.
            let (source, shuffled) = match &state.context {
                Some(ctx) => (Some(&ctx.source), Some(ctx.shuffled)),
                None => (None, None),
            };
            conn.execute(
                "INSERT OR REPLACE INTO playback_state \
                     (id, source, shuffled, manual, repeat, \
                      current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    source,
                    shuffled,
                    state.manual,
                    state.repeat,
                    state.current_track_id,
                    state.position_ms,
                    state.volume,
                    state.is_muted,
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
    }

    /// Read the device-local `playback_state` row: [`LoadedPlaybackState::Present`]
    /// with the row, [`LoadedPlaybackState::Absent`] when none is stored, or
    /// [`LoadedPlaybackState::Corrupt`] for a structurally-impossible row. The
    /// three are distinct so the caller counts and clears a corrupt cache rather
    /// than silently starting fresh over a masked failure.
    pub async fn load_playback_state(&self) -> Result<LoadedPlaybackState, DbError> {
        self.read(move |sql| {
            // The closure's `None` is a corrupt row; the outer `.optional()`'s
            // `None` is no row at all — the two stay distinct below.
            let loaded = sql
                .query_row(
                    "SELECT source, shuffled, manual, repeat, \
                     current_track_id, position_ms, volume, is_muted \
                     FROM playback_state WHERE id = 'current'",
                    [],
                    |row| {
                        // `source` and `shuffled` are written together: both
                        // present is a context, both absent is no context,
                        // exactly one present is a corrupt row.
                        let source: Option<String> = row.get("source")?;
                        let shuffled: Option<bool> = row.get("shuffled")?;
                        let context = match (source, shuffled) {
                            (Some(source), Some(shuffled)) => {
                                Some(DbPlaybackContext { source, shuffled })
                            }
                            (None, None) => None,
                            (Some(source), None) => {
                                warn!(
                                    "discarding the playback resume cache: source {source:?} \
                                     present but shuffled is NULL"
                                );
                                return Ok(None);
                            }
                            (None, Some(shuffled)) => {
                                warn!(
                                    "discarding the playback resume cache: shuffled {shuffled} \
                                     present but source is NULL"
                                );
                                return Ok(None);
                            }
                        };
                        Ok(Some(DbPlaybackState {
                            context,
                            manual: row.get("manual")?,
                            repeat: row.get("repeat")?,
                            current_track_id: row.get("current_track_id")?,
                            position_ms: row.get("position_ms")?,
                            volume: row.get("volume")?,
                            is_muted: row.get("is_muted")?,
                        }))
                    },
                )
                .optional()
                .map_err(DbError::from)?;
            Ok(match loaded {
                None => LoadedPlaybackState::Absent,
                Some(None) => LoadedPlaybackState::Corrupt,
                Some(Some(row)) => LoadedPlaybackState::Present(row),
            })
        })
        .await
    }

    /// Delete the device-local `playback_state` row (playback stopped).
    pub async fn clear_playback_state(&self) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute("DELETE FROM playback_state", [])
                .map(|_| ())
                .map_err(DbError::from)
        })
        .await
    }
}
