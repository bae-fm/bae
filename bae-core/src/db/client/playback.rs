use super::*;

impl Database {
    /// Follow the display rows for the tracks `initial` names. The queue
    /// changes by pointing the same query at new tracks through its request
    /// handle, not by opening another one; a queue change that plays the same
    /// tracks in another order is not a new request at all.
    pub(crate) fn subscribe_queue_catalog(
        &self,
        initial: QueueCatalogRequest,
    ) -> coven::ReconfigurableLiveQuery<QueueCatalogRequest, QueueCatalogProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |request, sql| {
                queue_catalog_on(&sql, request).map_err(CovenError::from)
            })
            .process(|_, projection| Ok(projection))
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

/// Each track's queue display metadata: its album and artist names, duration,
/// and its own release's cover. The cover is the track's own release's, not
/// the album's primary release's, so a queued track from a non-primary release
/// shows that release's art — the same rule `playback_info_from_track_release`
/// applies to the playing track. Its `covers` row joins in here rather than in
/// a second query, giving each entry the versioned reference the UI caches art
/// under; a release with no cover row yields `None`.
fn queue_metadata_on(
    sql: &SqlReadContext<'_>,
    track_ids: &BTreeSet<String>,
) -> Result<HashMap<String, TrackQueueMeta>, DbError> {
    let track_ids: Vec<&String> = track_ids.iter().collect();
    let mut meta_by_track: HashMap<String, TrackQueueMeta> = HashMap::new();
    for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        let query = format!(
            "SELECT \
                t.id AS track_id, t.title, t.duration_ms, a.title AS album_title, \
                r.id AS cover_image_id, c.blob_id AS cover_version, \
                COALESCE( \
                    NULLIF(( \
                        SELECT GROUP_CONCAT(art.name, ', ' ORDER BY credit.position) \
                        FROM ( \
                            SELECT {track_artist} AS artist_id, MIN(ta.position) AS position \
                            FROM track_artists ta \
                            WHERE ta.track_id = t.id \
                            GROUP BY 1 \
                        ) credit \
                        JOIN artists art ON art.id = credit.artist_id \
                    ), ''), \
                    (SELECT art_primary.name FROM artists art_primary WHERE art_primary.id = {primary}) \
                ) AS artist_names \
             FROM tracks t \
             JOIN releases r ON r.id = t.release_id \
             JOIN albums a ON a.id = r.album_id \
             LEFT JOIN covers c ON c.id = r.id \
             WHERE t.id IN ({placeholders})",
            track_artist = shown_artist_id("ta.artist_id"),
            primary = shown_artist_id("a.artist_id"),
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
                        cover_image: cover_version.map(|version| crate::album_detail::ImageRef {
                            id: cover_image_id,
                            version,
                            image_type: LibraryImageType::Cover,
                        }),
                    },
                ))
            },
        )?);
    }
    Ok(meta_by_track)
}

fn queue_catalog_on(
    sql: &SqlReadContext<'_>,
    request: &QueueCatalogRequest,
) -> Result<QueueCatalogProjection, DbError> {
    let tracks = queue_metadata_on(sql, &request.track_ids)?;
    let source_title = match request.context_release_id.as_deref() {
        None => None,
        Some(release_id) => {
            let album_id = find_release_by_id_on(sql, release_id)?.map(|release| release.album_id);
            match album_id {
                None => None,
                Some(album_id) => find_album_by_id_on(sql, &album_id)?.map(|album| album.title),
            }
        }
    };
    Ok(QueueCatalogProjection {
        tracks,
        source_title,
    })
}

/// What a catalog read reads: the distinct tracks the queue shows, and the
/// release a release context plays from, which names the queue's source. The
/// queue's entries — which instance of a track sits where — are not read from
/// the database, so they are not part of the request; the consumer joins them
/// onto the tracks it read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QueueCatalogRequest {
    pub track_ids: BTreeSet<String>,
    pub context_release_id: Option<String>,
}

impl QueueCatalogRequest {
    /// The request that reads every track `entries` play.
    pub(crate) fn for_entries<'a>(
        entries: impl IntoIterator<Item = &'a QueueEntry>,
        context_release_id: Option<String>,
    ) -> Self {
        Self {
            track_ids: entries
                .into_iter()
                .map(|entry| entry.track_id.clone())
                .collect(),
            context_release_id,
        }
    }
}

/// The display metadata of each requested track, and the source title.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueueCatalogProjection {
    tracks: HashMap<String, TrackQueueMeta>,
    pub source_title: Option<String>,
}

impl QueueCatalogProjection {
    /// One item per entry whose track this read found, in entry order. An
    /// entry whose track was not requested or is gone from the library is
    /// skipped.
    pub(crate) fn items(&self, entries: &[QueueEntry]) -> Vec<QueueItem> {
        resolve_queue_entries(&self.tracks, entries)
    }
}
