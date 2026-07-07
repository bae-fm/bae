use super::*;

impl PlaybackService {
    pub(super) fn next_track_id(&self) -> Option<&str> {
        self.preloaded_next.as_ref().map(PreloadedNext::track_id)
    }

    pub(super) async fn handle_tracks_deleted(&mut self, track_ids: Vec<String>) {
        let ids: HashSet<String> = track_ids.into_iter().collect();

        let current_deleted = self
            .playback_queue
            .current_track_id()
            .map(|s| ids.contains(s))
            .unwrap_or(false);

        // Purge deleted tracks from the queue first
        self.playback_queue.remove_by_ids(&ids);

        if current_deleted {
            // Stop current playback (tears down streams, decoder, next track state)
            self.stop().await;

            // Advance to next track if queue has one, otherwise stay stopped
            if let Some(next_id) = self.playback_queue.advance_to_front() {
                self.play_track(&next_id, TrackStart::Direct, false).await;
            }
        } else {
            // Current track is fine, but check preloaded next track
            if let Some(preloaded) = &self.preloaded_next {
                if ids.contains(preloaded.track_id()) {
                    self.clear_next_track_state();
                }
            }
        }

        self.emit_queue_update();
    }

    pub(super) async fn preload_queue_front(&mut self) {
        if let Some(next_id) = self.playback_queue.front().map(str::to_string) {
            self.preload_next_track(&next_id).await;
        }
    }

    /// Preload the next track for gapless playback.
    /// This eagerly starts the decoder so samples are ready when we switch tracks.
    pub(super) async fn preload_next_track(&mut self, track_id: &str) {
        self.clear_next_track_state();

        // Prepare track: fetch metadata, create buffer, start reading
        let prepared = prepare_track_for_playback(
            &self.library_manager,
            track_id,
            &mut self.shared_file_buffers,
            self.progress_tx.clone(),
            self.fetch_arbiter.clone(),
        )
        .await;
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to preload track {}: {}", track_id, e);
                return;
            }
        };

        // Create decoder sink/source and start decoder eagerly for gapless playback
        let decoder_buffer = prepared
            .segments
            .first()
            .expect("prepared track has at least one segment")
            .buffer
            .clone();
        let cancel_token = prepared.cancel_token.clone();

        // Preload params (natural transition: no pregap skip): seek to the
        // track's first sample, trim there, stop at its end.
        let decode = prepared.decode_params(0, true);
        let (mut sink, source, _ready) =
            create_track_stream_pair(prepared.sample_rate, prepared.channels);

        let decoder_handle = std::thread::spawn(move || {
            if let Err(e) = decode.run_decoder(decoder_buffer, &mut sink, cancel_token) {
                let _ = log_streaming_decode_failure("Preload streaming decode", e);
            }
        });

        // Store preloaded state. If the next track shares the live stream's
        // format, stage it into the PlaybackSource so the audio callback can
        // cross the boundary without rebuilding the stream (true gapless).
        // Otherwise hold it for the rebuild path (format change, no live
        // stream yet, or repeat-track mode where the current track replays
        // instead) which the completion → AutoAdvance flow handles.
        let stage_target = match (&self.current_prepared, &self.current_playback_source) {
            (Some(current), Some(gapless))
                if current.sample_rate == prepared.sample_rate
                    && current.channels == prepared.channels
                    && self.playback_queue.repeat_mode() != RepeatMode::Track
                    && !self.should_hold_for_side_pause(current, &prepared) =>
            {
                Some(gapless)
            }
            _ => None,
        };
        if let Some(gapless) = stage_target {
            info!(
                "Preload: staged next track into gapless chain: {}",
                track_id
            );
            gapless
                .lock()
                .unwrap()
                .stage_next(source, prepared.track_fmt(std::time::Duration::ZERO));
            self.preloaded_next = Some(PreloadedNext {
                prepared,
                decoder_handle,
                source: PreloadedNextSource::Staged,
            });
        } else {
            info!(
                "Preload: holding next track for stream-rebuild path: {}",
                track_id
            );
            self.preloaded_next = Some(PreloadedNext {
                prepared,
                decoder_handle,
                source: PreloadedNextSource::Held(source),
            });
        }

        info!("Preloaded next track (streaming): {}", track_id);
    }

    pub(super) async fn side_pause_for_queue_front(
        &mut self,
    ) -> Result<Option<SidePauseDecision>, ()> {
        if !self.side_pause_enabled() {
            return Ok(None);
        }

        let Some(current) = self
            .current_prepared
            .as_ref()
            .map(|prepared| prepared.track_info.clone())
        else {
            error!("side-pause decision requested without current track metadata");
            return Err(());
        };

        let Some(next_track_id) = self
            .playback_queue
            .next_sequential_context_track()
            .map(str::to_string)
        else {
            return Ok(None);
        };

        let next_info = match self.preloaded_next.as_ref() {
            Some(preloaded) if preloaded.track_id() == next_track_id => {
                preloaded.prepared.track_info.clone()
            }
            Some(preloaded) => {
                debug!(
                    preloaded_track_id = %preloaded.track_id(),
                    queue_next_track_id = %next_track_id,
                    "side-pause decision ignoring stale preloaded next track"
                );
                self.playback_info_for_side_pause(&next_track_id).await?
            }
            None => self.playback_info_for_side_pause(&next_track_id).await?,
        };

        Ok(self
            .side_pause_prompt_for_infos(&current, &next_info)
            .map(|prompt| SidePauseDecision {
                track_id: next_track_id,
                prompt,
            }))
    }

    pub(super) async fn playback_info_for_side_pause(
        &self,
        track_id: &str,
    ) -> Result<PlaybackTrackInfo, ()> {
        self.library_manager
            .get_playback_track_info(track_id)
            .await
            .map_err(|error| {
                error!(
                    "failed to resolve playback metadata for side-pause decision on {track_id}: {error}"
                );
            })
    }

    pub(super) fn should_hold_for_side_pause(
        &self,
        current: &PlaybackPreparedTrack,
        next: &PlaybackPreparedTrack,
    ) -> bool {
        self.side_pause_prompt_for_infos(&current.track_info, &next.track_info)
            .is_some()
    }

    pub(super) fn side_pause_prompt_for_infos(
        &self,
        current: &PlaybackTrackInfo,
        next: &PlaybackTrackInfo,
    ) -> Option<PlaybackSidePausePrompt> {
        if !self.side_pause_enabled() {
            return None;
        }
        side_pause_prompt_between(current, next)
    }

    pub(super) fn side_pause_enabled(&self) -> bool {
        self.library_manager.get_config().pause_between_sides
            && self.playback_queue.repeat_mode() != RepeatMode::Track
    }

    pub(super) fn pause_for_side_end(&mut self, decision: SidePauseDecision) {
        self.pending_side_pause = Some(decision.clone());
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Paused);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: self.make_paused_state(PlaybackPauseReason::SideEnded(decision.prompt)),
            },
        );
        self.emit_queue_update();
    }

    pub(super) async fn resume_from_side_pause(&mut self) {
        let Some(pending) = self.pending_side_pause.clone() else {
            warn!("side-pause resume requested without pending side-pause state");
            return;
        };
        let pending_track_id = pending.track_id;

        if self.next_track_id() == Some(pending_track_id.as_str()) {
            self.advance_and_play_preloaded(&pending_track_id, true, false)
                .await;
            self.pending_side_pause = None;
            return;
        }

        let Some(front) = self.playback_queue.front().map(str::to_string) else {
            error!("side-pause resume expected {pending_track_id}, but the queue is empty");
            self.pending_side_pause = None;
            return;
        };
        if front != pending_track_id {
            error!("side-pause resume expected {pending_track_id}, but queue front is {front}");
            self.pending_side_pause = None;
            return;
        }
        match self.playback_queue.next_entry() {
            NextEntry::Play(track_id) => {
                self.pending_side_pause = None;
                self.emit_queue_update();
                self.play_track(&track_id, TrackStart::Natural, false).await;
            }
            other => {
                error!("side-pause resume expected Play for {pending_track_id}, got {other:?}");
                self.pending_side_pause = None;
            }
        }
    }

    /// If a track is preloaded but no longer matches the queue front, discard it and
    /// preload the new front instead.  Called after queue mutations that insert at position 0.
    pub(super) async fn refresh_preload_for_queue_front(&mut self) {
        let preloaded_id = match self.next_track_id() {
            Some(id) => id.to_string(),
            None => return,
        };
        let queue_front = match self.playback_queue.front() {
            Some(id) => id.to_string(),
            None => {
                self.clear_next_track_state();
                return;
            }
        };
        if preloaded_id != queue_front {
            self.clear_next_track_state();
            self.preload_next_track(&queue_front).await;
        }
    }

    pub(super) fn clear_next_track_state(&mut self) {
        clear_preloaded_next(
            &mut self.preloaded_next,
            self.current_playback_source.as_ref(),
        );
    }

    /// Whether a preloaded next-track source is available.
    pub(super) fn has_preloaded_next(&self) -> bool {
        let Some(preloaded) = &self.preloaded_next else {
            return false;
        };
        match &preloaded.source {
            PreloadedNextSource::Held(_) => true,
            PreloadedNextSource::Staged => self
                .current_playback_source
                .as_ref()
                .is_some_and(|gapless| gapless.lock().unwrap().has_next()),
        }
    }

    /// Promote track bookkeeping after the audio callback crossed a gapless
    /// track boundary. The `PlaybackSource` has already advanced to the staged
    /// next track within the same stream; here we report the finishing track's
    /// decode stats, update service state to match, and preload the following
    /// track. No stream rebuild, no UI gap.
    ///
    /// Pure in the `TrackCrossing` payload: both track ids and the finishing
    /// stats come from the event, not from a shared cell.
    pub(super) async fn handle_track_crossed(&mut self, crossing: TrackCrossing) {
        // Report the finishing track's decode stats here: a gaplessly-advanced
        // track never reaches the completion path, so this is the per-track
        // completion log + stats for every track except the album's last.
        info!(
            "Track completed (gapless): {} ({} decode errors, {} samples)",
            crossing.finished_fmt.track_id, crossing.decode_error_count, crossing.samples_decoded
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::DecodeStats {
                track_id: crossing.finished_fmt.track_id.clone(),
                error_count: crossing.decode_error_count,
                samples_decoded: crossing.samples_decoded,
            },
        );

        let preloaded = match self.preloaded_next.take() {
            Some(preloaded) => preloaded,
            None => {
                warn!(
                    track_id = %crossing.incoming_fmt.track_id,
                    "Gapless boundary fired with no preloaded track; ignoring"
                );
                return;
            }
        };
        let PreloadedNext {
            prepared: next_prepared,
            decoder_handle,
            ..
        } = preloaded;
        let track_id = crossing.incoming_fmt.track_id.clone();
        info!("Gapless boundary: now playing {}", track_id);

        // The previous track's decoder has finished; the next track's decoder
        // becomes the current one.
        self.current_decoder_handle = Some(decoder_handle);

        // Release the previous track's buffer (unless shared with the new one).
        if let Some(prev) = self.current_prepared.take() {
            prev.release_buffers_not_used_by(&next_prepared, &mut self.shared_file_buffers);
        }

        self.current_prepared = Some(next_prepared);
        // The crossed-into track is now playing: hand its reader fetch priority
        // over the track preloaded below.
        self.mark_current_foreground();

        // Advance the queue position to the now-playing track.
        self.advance_to_preloaded();

        // Natural transition starts at position 0 (pregap included).
        *self.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

        // Tell the UI which track is now playing (StateChanged covers the transition).
        self.emit_current_state();

        self.preload_queue_front().await;

        // The gapless boundary advanced the current track without play_track.
        self.persist_playback_state().await;
    }

    /// Advance the queue's current pointer past the finished track to the front
    /// and emit the queue update. Used by `Next`, `AutoAdvance` (preloaded path),
    /// and the gapless boundary handler. The front is the track being played: the preload
    /// refreshes whenever the queue mutates, so the advanced front and the track
    /// these callers go on to play are the same one.
    pub(super) fn advance_to_preloaded(&mut self) {
        if self.playback_queue.advance_to_front().is_none() {
            warn!("advance_to_preloaded: queue had no front to advance to");
        }
        self.emit_queue_update();
    }

    /// Play the preloaded next track: advance the queue to it, then start its
    /// buffered stream if ready, or a fresh play of it otherwise. `natural`
    /// (pregap included) and `preserve_paused` pass through to the player. Shared
    /// by `Next` and `AutoAdvance`, which differ only in those two booleans.
    pub(super) async fn advance_and_play_preloaded(
        &mut self,
        preloaded_track_id: &str,
        natural: bool,
        preserve_paused: bool,
    ) {
        if self.has_preloaded_next() {
            info!("Using preloaded track: {}", preloaded_track_id);
            self.advance_to_preloaded();
            self.play_preloaded_track(natural, preserve_paused).await;
        } else {
            // Preload started but the streaming source isn't ready yet.
            self.advance_to_preloaded();
            self.clear_next_track_state();
            self.play_track(
                preloaded_track_id,
                TrackStart::from_natural_transition(natural),
                preserve_paused,
            )
            .await;
        }
    }

    /// Play a preloaded track by swapping next state to current and starting the audio stream.
    /// Play a preloaded track. The decoder is already running from preload_next_track.
    /// Play the preloaded next track.
    /// - `is_natural_transition`: if true, plays from INDEX 00 (pregap included)
    /// - `preserve_paused`: if true, inherits current paused state; if false, always starts playing
    pub(super) async fn play_preloaded_track(
        &mut self,
        is_natural_transition: bool,
        preserve_paused: bool,
    ) {
        let preloaded = match self.preloaded_next.take() {
            Some(preloaded) => preloaded,
            None => {
                error!("play_preloaded_track called without preloaded state");
                return;
            }
        };
        let PreloadedNext {
            prepared: next_prepared,
            decoder_handle,
            source: preloaded_source,
        } = preloaded;

        let pregap_ms = next_prepared.total_pregap_ms();
        let track_id = next_prepared.track_info.track_id.clone();

        // If we need to skip pregap (direct selection), the preloaded state won't work
        // because it was set up for auto-advance (starting at byte 0).
        // Fall back to play_track which handles pregap at decoder start.
        if !is_natural_transition && pregap_ms.is_some_and(|p| p > 0) {
            info!("Pregap skip needed for preloaded track - falling back to play_track");
            next_prepared.cancel_unshared_buffers();
            if let Some(source) =
                detach_preloaded_source(self.current_playback_source.as_ref(), preloaded_source)
            {
                source.cancel();
            }
            self.play_track(
                &track_id,
                TrackStart::from_natural_transition(is_natural_transition),
                preserve_paused,
            )
            .await;
            return;
        }

        // Recover the preloaded next source (staged in the gapless chain or held
        // for the rebuild path) BEFORE tearing down the current stream.
        let source =
            detach_preloaded_source(self.current_playback_source.as_ref(), preloaded_source)
                .expect("Preloaded track has no streaming source");

        // Cancel current streaming state
        if let Some(gapless) = self.current_playback_source.take() {
            if let Ok(guard) = gapless.lock() {
                guard.cancel();
            }
        }
        if let Some(prepared) = &self.current_prepared {
            prepared.release_buffers_not_used_by(&next_prepared, &mut self.shared_file_buffers);
        }

        // Swap next to current. The preloaded track's decoder becomes the current
        // one; the previous track's decoder was cancelled above via the source.
        self.current_decoder_handle = Some(decoder_handle);
        self.current_prepared = Some(next_prepared);
        // The preloaded track is now the playing one: hand its reader fetch
        // priority over whatever gets preloaded next.
        self.mark_current_foreground();

        // Natural transition: start at position 0 (INDEX 00, pregap plays).
        let fmt = self
            .current_prepared
            .as_ref()
            .expect("current_prepared just set above")
            .track_fmt(std::time::Duration::ZERO);

        // Initialize streaming with the preloaded source
        if !self.init_streaming(source, fmt).await {
            self.stop().await;
            return;
        }

        // Set audio state: always Playing unless preserving paused state
        if !preserve_paused {
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Playing);
        }

        // Send state notification
        self.emit_current_state();

        self.preload_queue_front().await;

        // The preloaded advance doesn't go through play_track, so persist the
        // now-playing track here.
        self.persist_playback_state().await;
    }
}
