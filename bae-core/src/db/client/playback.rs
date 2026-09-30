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
            .process(|_, display| Ok(display))
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
                    let release =
                        find_release_by_id_on(&sql, &track.release_id)?.ok_or_else(|| {
                            DbError::Message(format!(
                                "track {track_id} names release {} the library does not hold",
                                track.release_id
                            ))
                        })?;
                    infos.push(crate::playback::PlaybackTrackInfo::of(&track, &release));
                }
                Ok(infos)
            })
            .process(|_, infos| Ok(infos))
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

/// Each requested track's display, and its catalog duration the queue rows
/// show: its title, its credited artists (the album's when the track credits
/// none), its album, the release it is on, and that release's cover. The
/// cover is the track's own release's, not the album's primary release's, so
/// a track from a non-primary release shows that release's art. Its `covers`
/// row joins in here rather than in a second query, giving each track the
/// versioned reference the UI caches art under; a release with no cover row
/// yields `None`. A requested track the library no longer holds is absent.
fn track_displays_on(
    sql: &SqlReadContext<'_>,
    track_ids: &BTreeSet<String>,
) -> Result<HashMap<String, TrackQueueMeta>, DbError> {
    let track_ids: Vec<&String> = track_ids.iter().collect();
    let mut meta_by_track: HashMap<String, TrackQueueMeta> = HashMap::new();
    for chunk in track_ids.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        let query = format!(
            "SELECT \
                t.id AS track_id, t.title, t.duration_ms, a.id AS album_id, \
                a.title AS album_title, r.id AS release_id, c.blob_id AS cover_version, \
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
                    ( \
                        SELECT GROUP_CONCAT(art.name, ', ' ORDER BY credit.position) \
                        FROM ( \
                            SELECT album_credit.artist_id, MIN(album_credit.position) AS position \
                            FROM ( \
                                SELECT {primary} AS artist_id, -1 AS position \
                                UNION ALL \
                                SELECT {album_artist} AS artist_id, aa.position \
                                FROM album_artists aa \
                                WHERE aa.album_id = a.id \
                            ) album_credit \
                            GROUP BY 1 \
                        ) credit \
                        JOIN artists art ON art.id = credit.artist_id \
                    ) \
                ) AS artist_names \
             FROM tracks t \
             JOIN releases r ON r.id = t.release_id \
             JOIN albums a ON a.id = r.album_id \
             LEFT JOIN covers c ON c.id = r.id \
             WHERE t.id IN ({placeholders})",
            track_artist = shown_artist_id("ta.artist_id"),
            primary = shown_artist_id("a.artist_id"),
            album_artist = shown_artist_id("aa.artist_id"),
        );
        meta_by_track.extend(sql.query(
            &query,
            coven::rusqlite::params_from_iter(chunk.iter()),
            |row| {
                let track_id: String = row.get("track_id")?;
                let release_id: String = row.get("release_id")?;
                let cover_version: Option<String> = row.get("cover_version")?;
                // A release's cover image is keyed by the release's id.
                let cover_image = cover_version.map(|version| crate::album_detail::ImageRef {
                    id: release_id.clone(),
                    version,
                    image_type: LibraryImageType::Cover,
                });
                Ok((
                    track_id,
                    TrackQueueMeta {
                        display: crate::playback::TrackDisplay {
                            title: row.get("title")?,
                            artist_names: row.get("artist_names")?,
                            album_id: row.get("album_id")?,
                            release_id,
                            album_title: row.get("album_title")?,
                            cover_image,
                        },
                        duration_ms: row.get("duration_ms")?,
                    },
                ))
            },
        )?);
    }
    Ok(meta_by_track)
}

/// The display of `track_id`, or `None` when the library no longer holds it.
fn track_display_on(
    sql: &SqlReadContext<'_>,
    track_id: &str,
) -> Result<Option<crate::playback::TrackDisplay>, DbError> {
    Ok(
        track_displays_on(sql, &BTreeSet::from([track_id.to_string()]))?
            .remove(track_id)
            .map(|meta| meta.display),
    )
}

fn queue_catalog_on(
    sql: &SqlReadContext<'_>,
    request: &QueueCatalogRequest,
) -> Result<QueueCatalogProjection, DbError> {
    let tracks = track_displays_on(sql, &request.track_ids)?;
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
