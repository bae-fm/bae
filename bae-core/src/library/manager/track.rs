//! Track domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Ordered track IDs for a release, without pulling full `DbTrack` rows. For
    /// callers that only need IDs (queue building, repeat-album rebuild).
    pub async fn get_track_ids(&self, release_id: &str) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_track_ids_for_release(release_id).await?)
    }

    /// Every track id in the library, in a deterministic base order. Used to
    /// materialize a `ContextSource::Library` context (shuffle library, and the
    /// `Context`-repeat re-derive of a library context).
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_all_track_ids().await?)
    }

    /// A track's play context: its release id, that release's full track order, and
    /// the track's index within it. The playback service builds the queue around a
    /// freshly selected track from this, without chaining library calls.
    pub async fn get_play_context(&self, track_id: &str) -> Result<PlayContext, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let release_id = track.release_id;
        let track_ids = self.database.get_track_ids_for_release(&release_id).await?;
        let index = track_ids
            .iter()
            .position(|id| id == track_id)
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!(
                    "Track {} not present in its release {}",
                    track_id, release_id
                ))
            })?;
        Ok(PlayContext {
            release_id,
            track_ids,
            index,
        })
    }

    /// The subset of `ids` that still exist in the tracks table. Playback restore
    /// validates a persisted queue with this in one query, not one per track.
    pub async fn filter_existing_track_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.filter_existing_track_ids(ids).await?)
    }

    /// Resolve a mix of album and track IDs into track IDs. An album expands to the
    /// tracks of its primary release — the user's chosen release when set, otherwise
    /// the earliest-imported one, which is the fallback `primary_release_id` already
    /// encodes.
    pub async fn resolve_to_track_ids(&self, ids: &[String]) -> Result<Vec<String>, LibraryError> {
        let mut track_ids = Vec::new();
        for id in ids {
            if let Some(album_track_ids) = self
                .database
                .get_primary_release_track_ids_for_album(id)
                .await?
            {
                track_ids.extend(album_track_ids);
            } else if self.database.find_track_by_id(id).await?.is_some() {
                track_ids.push(id.clone());
            } else {
                return Err(LibraryError::TrackMapping(format!(
                    "ID not found as album or track: {id}"
                )));
            }
        }
        Ok(track_ids)
    }

    pub async fn get_queue_items(
        &self,
        entries: &[QueueEntry],
    ) -> Result<Vec<QueueItem>, LibraryError> {
        let items = self.database.get_queue_items(entries).await?;
        // Each entry resolves to at most one item; a shortfall is entries whose
        // track has no metadata (deleted from the library but still queued — a
        // deletion-consistency gap). The pure DB layer logs each; the manager,
        // which holds the diagnostics sink, counts them.
        let dropped = entries.len().saturating_sub(items.len());
        for _ in 0..dropped {
            self.diagnostics().event(TelemetryEvent::Anomaly {
                kind: crate::diagnostics::AnomalyKind::QueueTrackNoMetadata,
            });
        }
        Ok(items)
    }

    /// Resolve the manual lane in full, plus only the first `QUEUE_UPCOMING_WINDOW`
    /// entries of the context's upcoming tail; the rest pages in on demand through
    /// `AppServices::get_queue_upcoming_page`. That tail is library-scaled — a
    /// `Library` source's tail is every remaining track — so the slice happens
    /// *before* the resolve, not after. That is what keeps this bounded regardless
    /// of library size.
    pub async fn resolve_queue_projection(
        &self,
        projection: crate::playback::PlaybackQueueProjection,
    ) -> Result<crate::queue::ResolvedQueueSnapshot, LibraryError> {
        let upcoming_total = projection
            .context
            .as_ref()
            .map(|c| c.upcoming.len() as u64)
            .unwrap_or(0);
        let context_window: Vec<_> = projection
            .context
            .as_ref()
            .map(|c| {
                c.upcoming
                    .iter()
                    .take(crate::queue::QUEUE_UPCOMING_WINDOW)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let combined: Vec<_> = projection
            .manual
            .iter()
            .chain(context_window.iter())
            .cloned()
            .collect();
        let items = self.get_queue_items(&combined).await?;
        let context_ids: std::collections::HashSet<&str> =
            context_window.iter().map(|e| e.id.0.as_str()).collect();
        let (context_items, manual_items): (Vec<_>, Vec<_>) = items
            .into_iter()
            .partition(|i| context_ids.contains(i.entry_id.as_str()));
        let context = match projection.context {
            None => None,
            Some(c) => {
                let source_title = self.context_source_title(&c.source).await?;
                Some(crate::queue::ResolvedContext {
                    source: c.source,
                    source_title,
                    shuffled: c.shuffled,
                    upcoming: context_items,
                    upcoming_total,
                })
            }
        };
        Ok(crate::queue::ResolvedQueueSnapshot {
            manual: manual_items,
            context,
            has_next: projection.has_next,
            has_previous: projection.has_previous,
            revision: projection.revision,
        })
    }

    /// The display title for a context source: the album title of a single
    /// release. Multi-release and library sources return `None` — they label
    /// themselves by kind alone.
    async fn context_source_title(
        &self,
        source: &crate::playback::ContextSource,
    ) -> Result<Option<String>, LibraryError> {
        let crate::playback::ContextSource::Release(release_id) = source else {
            return Ok(None);
        };
        let Some(release) = self.database.find_release_by_id(release_id).await? else {
            // A context can outlive its release (deleted while its tail still
            // plays); the section header then labels by kind alone.
            tracing::warn!("no release {release_id} for the playing context; omitting its title");
            return Ok(None);
        };
        let Some(album) = self.database.find_album_by_id(&release.album_id).await? else {
            tracing::warn!(
                "no album {} for the playing context's release {release_id}; omitting its title",
                release.album_id
            );
            return Ok(None);
        };
        Ok(Some(album.title))
    }

    /// The file record for a blob id — streaming looks the id up on the track's
    /// audio segments, then fetches the row here.
    pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, LibraryError> {
        Ok(self.database.find_file_by_id(file_id).await?)
    }

    /// Test-only. Production reads audio format as part of the resolved track-audio
    /// / playback-info aggregates below, never standalone.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, LibraryError> {
        Ok(self
            .database
            .find_audio_format_by_track_id(track_id)
            .await?)
    }

    /// Resolve a track's audio into a `ResolvedTrackAudio` with its sample window
    /// resolved and all raw `Db*` fields hidden.
    pub async fn resolve_track_audio(
        &self,
        track_id: &str,
    ) -> Result<ResolvedTrackAudio, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        Ok(ResolvedTrackAudio::from_meta(&meta))
    }

    /// A track's display metadata (artist names, album, cover) at playback-prep
    /// time. Resolved here so `PlaybackService` never sees a `DbTrack`.
    pub async fn get_playback_track_info(
        &self,
        track_id: &str,
    ) -> Result<crate::playback::PlaybackTrackInfo, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let release = self.database.get_release_for_track(&track).await?;
        playback_info_from_track_release(&self.database, &track, &release).await
    }

    /// Both the audio aggregate and the display metadata in one pass, sparing
    /// playback prep the `DbTrack`/`DbRelease` double-fetch that calling
    /// `resolve_track_audio` and `get_playback_track_info` separately would cost.
    pub(crate) async fn resolve_track_audio_and_info(
        &self,
        track_id: &str,
    ) -> Result<(ResolvedTrackAudio, crate::playback::PlaybackTrackInfo), LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let audio = ResolvedTrackAudio::from_meta(&meta);
        let info =
            playback_info_from_track_release(&self.database, &meta.track, &meta.release).await?;
        Ok((audio, info))
    }
}

/// `PlaybackTrackInfo` from an already-loaded track and release: queries only the
/// album title and artists, reusing what it is passed.
pub(crate) async fn playback_info_from_track_release(
    database: &Database,
    track: &DbTrack,
    release: &DbRelease,
) -> Result<crate::playback::PlaybackTrackInfo, LibraryError> {
    // Cover comes from the track's own release so playing a non-primary
    // release shows that release's art, not the album-level primary.
    let cover_image_id = Some(track.release_id.clone());
    let album_id = release.album_id.clone();
    let album_title = match database.find_album_by_id(&album_id).await? {
        Some(album) => album.title,
        None => {
            return Err(LibraryError::TrackMapping(format!(
                "album not found for track {} album {}",
                track.id, album_id
            )));
        }
    };

    let track_artists = database.get_artists_for_track(&track.id).await?;
    let (artist_id, artist_names) = if !track_artists.is_empty() {
        let id = track_artists[0].id.clone();
        let names = join_artist_names(&track_artists);
        (id, names)
    } else {
        let album_artists = database.get_artists_for_album(&album_id).await?;
        if album_artists.is_empty() {
            return Err(LibraryError::TrackMapping(format!(
                "no artist found for track {} album {}",
                track.id, album_id
            )));
        }
        let id = album_artists[0].id.clone();
        let names = join_artist_names(&album_artists);
        (id, names)
    };

    let side = crate::util::format::physical_side_medium(release.pressing.format.as_deref()).map(
        |medium| crate::playback::PlaybackTrackSide {
            medium,
            side_letter: crate::util::format::side_letter(track.side),
        },
    );

    Ok(crate::playback::PlaybackTrackInfo {
        track_id: track.id.clone(),
        track_title: track.title.clone(),
        artist_names,
        artist_id,
        album_id,
        album_title,
        cover_image_id,
        release_id: release.id.clone(),
        side,
    })
}
