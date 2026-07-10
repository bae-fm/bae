use super::*;

impl Database {
    /// Enrich a list of queue entries with album/artist metadata for display.
    /// Returns one `QueueItem` per entry, in the same order, each carrying the
    /// entry's per-instance id. The same track queued twice resolves twice (the
    /// metadata is fetched once and joined onto every entry of that track).
    /// Entries whose track is not found are skipped.
    pub async fn get_queue_items(&self, entries: &[QueueEntry]) -> Result<Vec<QueueItem>, DbError> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let entries = entries.to_vec();
        self
            .read(move |conn| {
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
                            a.primary_release_id, \
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
                        WHERE t.id IN ({placeholders})"
                    );

                    let mut stmt = conn.prepare(&query)?;
                    let mut rows = stmt.query(coven::rusqlite::params_from_iter(chunk.iter()))?;
                    while let Some(row) = rows.next()? {
                        let track_id: String = row.get("track_id")?;
                        meta_by_track.insert(
                            track_id,
                            TrackQueueMeta {
                                title: row.get("title")?,
                                artist_names: row.get("artist_names")?,
                                duration_ms: row.get("duration_ms")?,
                                album_title: row.get("album_title")?,
                                cover_image_id: row.get("primary_release_id")?,
                            },
                        );
                    }
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
            // Flatten the context substruct back to the table's three
            // nullable columns: all three NULL when no context is playing.
            let (source, shuffle_seed, cursor) = match &state.context {
                Some(ctx) => (Some(&ctx.source), ctx.shuffle_seed, Some(ctx.cursor)),
                None => (None, None, None),
            };
            conn.execute(
                "INSERT OR REPLACE INTO playback_state \
                     (id, source, shuffle_seed, cursor, manual, repeat, \
                      current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    source,
                    shuffle_seed,
                    cursor,
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

    /// Read the device-local `playback_state` row, or `None` if none is stored
    /// (or if the row is corrupt — the resume cache is discarded at this
    /// boundary so no caller downstream sees a malformed context).
    pub async fn load_playback_state(&self) -> Result<Option<DbPlaybackState>, DbError> {
        self.read(move |conn| {
            // The closure yields `Option<DbPlaybackState>`: `None` is a corrupt
            // row that discards the whole cache, distinct from the outer `None`
            // for no row at all. The outer `.optional()` then flattens both to
            // a single "no resume state" answer.
            conn.query_row(
                "SELECT source, shuffle_seed, cursor, manual, repeat, \
                     current_track_id, position_ms, volume, is_muted \
                     FROM playback_state WHERE id = 'current'",
                [],
                |row| {
                    // `source` and `cursor` are written together (with
                    // `shuffle_seed`, NULL = sequential): both present is a
                    // context, both absent is none, exactly one present is a
                    // corrupt row.
                    let source: Option<String> = row.get("source")?;
                    let shuffle_seed: Option<i64> = row.get("shuffle_seed")?;
                    let cursor: Option<i64> = row.get("cursor")?;
                    let context = match (source, cursor) {
                        (Some(source), Some(cursor)) => Some(DbPlaybackContext {
                            source,
                            shuffle_seed,
                            cursor,
                        }),
                        (None, None) => None,
                        (Some(source), None) => {
                            warn!(
                                "discarding the playback resume cache: source {source:?} \
                                     present but cursor is NULL"
                            );
                            return Ok(None);
                        }
                        (None, Some(cursor)) => {
                            warn!(
                                "discarding the playback resume cache: cursor {cursor} \
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
            .map(Option::flatten)
            .map_err(DbError::from)
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
