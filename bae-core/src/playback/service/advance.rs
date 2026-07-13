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

        self.playback_queue.remove_by_ids(&ids);

        if current_deleted {
            self.stop().await;

            // Play on from what's left of the queue, or stay stopped.
            if let Some(next_id) = self.playback_queue.advance_to_front() {
                self.play_track(
                    &next_id,
                    TrackStart::Direct,
                    PlayTarget::Playing,
                    TrackTransition::AutoAdvance,
                )
                .await;
            }
        } else {
            // The current track survives, but the preload may have been deleted.
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

    /// Preload the next track for gapless playback: start its decoder now, so its
    /// samples are already buffered when the current track ends.
    pub(super) async fn preload_next_track(&mut self, track_id: &str) {
        self.clear_next_track_state();

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
                self.telemetry_playback_failed(crate::diagnostics::PlaybackOperation::Preload);
                return;
            }
        };

        // A preload is a natural transition, so no pregap is skipped: decode from
        // the track's first sample to its end.
        let decode = prepared.decode_params(0, true);

        // A preload has no stream yet. `spawn_decoder` mints the decoder's cancel
        // token, carried in `PreloadedNext` so promotion hands it to whatever
        // adopts this decoder. LogOnly: a preload decode failure is logged, never
        // surfaced as a PlaybackError — the promotion path re-decodes through
        // play_track if the preload turns out unusable.
        let DecoderStart {
            track_stream,
            handle: decoder_handle,
            cancel_token,
            // No Loading state observes a preload; the ready signal goes unused.
            ready: _,
        } = spawn_decoder(
            decode,
            prepared.sample_rate,
            prepared.channels,
            "Preload streaming decode",
            DecodeFailureReport::LogOnly,
        );

        // Same format as the live stream: stage it into the `PlaybackSource` so
        // the audio callback crosses the boundary in place (true gapless).
        // Otherwise hold it for the rebuild path — a format change, no live stream
        // yet, repeat-track (the current track replays instead), or a side pause —
        // which the completion → AutoAdvance flow handles.
        let stage_source: Option<Arc<Mutex<source::PlaybackSource>>> = match &self.slot {
            PlaybackSlot::Active(cur)
                if cur.prepared.sample_rate == prepared.sample_rate
                    && cur.prepared.channels == prepared.channels
                    && self.playback_queue.repeat_mode() != RepeatMode::Track
                    && !self.should_hold_for_side_pause(&cur.prepared, &prepared) =>
            {
                self.output.as_ref().map(|o| o.source.clone())
            }
            _ => None,
        };
        if let Some(gapless) = stage_source {
            info!(
                "Preload: staged next track into gapless chain: {}",
                track_id
            );
            gapless
                .lock()
                .unwrap()
                .stage_next(track_stream, prepared.track_fmt(std::time::Duration::ZERO));
            self.preloaded_next = Some(PreloadedNext {
                prepared,
                decoder_handle,
                cancel_token,
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
                cancel_token,
                source: PreloadedNextSource::Held(track_stream),
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

        let Some(current) = (match &self.slot {
            PlaybackSlot::Active(cur) => Some(cur.prepared.track_info.clone()),
            _ => None,
        }) else {
            error!("side-pause decision requested without current track metadata");
            self.telemetry_anomaly(AnomalyKind::SidePauseDesync);
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

    /// Re-run the staging decision for the preloaded next track after
    /// `pause_between_sides` turns on mid-track. `preload_next_track` reads the
    /// config once, at preload time, so a track already staged into the gapless
    /// chain would otherwise keep crossing its boundary without a pause. If the
    /// preload is `Staged` and the updated config says to hold it, discard it and
    /// re-preload the same track — `preload_next_track` holds it this time. A
    /// `Held` preload needs nothing (`side_pause_for_queue_front` re-reads the
    /// config at drain time), and neither does no active track / no preload. If
    /// the boundary was already crossed by the time this reaches the command loop,
    /// the current track IS the next side, so no pause is due and this does
    /// nothing — the toggle simply came too late for that boundary.
    pub(super) async fn reevaluate_side_pause_staging(&mut self) {
        let Some(preloaded) = &self.preloaded_next else {
            return;
        };
        if !matches!(preloaded.source, PreloadedNextSource::Staged) {
            return;
        }
        let should_hold = match &self.slot {
            PlaybackSlot::Active(cur) => {
                self.should_hold_for_side_pause(&cur.prepared, &preloaded.prepared)
            }
            _ => return,
        };
        if !should_hold {
            return;
        }
        let track_id = preloaded.track_id().to_string();
        info!(
            "pause_between_sides enabled mid-track: unstaging preloaded {} to hold for the side pause",
            track_id
        );
        self.clear_next_track_state();
        self.preload_next_track(&track_id).await;
    }

    pub(super) fn pause_for_side_end(&mut self, decision: SidePauseDecision) {
        // The current track drained (phase Completed): fold the side-pause into
        // the phase, carrying the track it resumes into.
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Paused(PausePhase::SideEnded(decision));
        }
        self.sync_audio_state();
        self.emit_state();
        self.emit_queue_update();
    }

    pub(super) async fn resume_from_side_pause(&mut self) {
        let Some(pending_track_id) = self.side_pause_resume_track_id() else {
            warn!("side-pause resume requested without pending side-pause state");
            self.telemetry_anomaly(AnomalyKind::SidePauseDesync);
            return;
        };

        if self.next_track_id() == Some(pending_track_id.as_str()) {
            self.advance_and_play_preloaded(&pending_track_id, true, PlayTarget::Playing)
                .await;
            return;
        }

        let Some(front) = self.playback_queue.front().map(str::to_string) else {
            error!("side-pause resume expected {pending_track_id}, but the queue is empty");
            self.telemetry_anomaly(AnomalyKind::SidePauseDesync);
            self.demote_side_pause_to_manual();
            return;
        };
        if front != pending_track_id {
            error!("side-pause resume expected {pending_track_id}, but queue front is {front}");
            self.telemetry_anomaly(AnomalyKind::SidePauseDesync);
            self.demote_side_pause_to_manual();
            return;
        }
        match self.playback_queue.next_entry() {
            NextEntry::Play(track_id) => {
                self.emit_queue_update();
                self.play_track(
                    &track_id,
                    TrackStart::Natural,
                    PlayTarget::Playing,
                    TrackTransition::AutoAdvance,
                )
                .await;
            }
            other => {
                error!("side-pause resume expected Play for {pending_track_id}, got {other:?}");
                self.telemetry_anomaly(AnomalyKind::SidePauseDesync);
                self.demote_side_pause_to_manual();
            }
        }
    }

    /// The track a side-pause resumes into, if the current phase is a side-pause.
    fn side_pause_resume_track_id(&self) -> Option<String> {
        match &self.slot {
            PlaybackSlot::Active(cur) => match &cur.phase {
                TrackPhase::Paused(PausePhase::SideEnded(decision)) => {
                    Some(decision.track_id.clone())
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Demote a side-pause to a plain manual pause without emitting. Used when a
    /// queue mutation invalidates the pending next side, or on a side-pause
    /// resume that can no longer find its target — the UI keeps showing the last
    /// emitted state (still paused) while the machine forgets the side-pause.
    pub(super) fn demote_side_pause_to_manual(&mut self) {
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            if matches!(cur.phase, TrackPhase::Paused(PausePhase::SideEnded(_))) {
                cur.phase = TrackPhase::Paused(PausePhase::Manual);
            }
        }
    }

    /// If the preloaded track is no longer the queue front (a mutation inserted
    /// ahead of it), discard it and preload the new front.
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

    /// The persistent output's playback source, present whenever a stream is
    /// attached (which is exactly whenever a track is current).
    pub(super) fn current_source(&self) -> Option<&Arc<Mutex<source::PlaybackSource>>> {
        self.output.as_ref().map(|o| &o.source)
    }

    /// Discard the preloaded decoder and hand back its prepared track without
    /// releasing its file buffers — for callers that release them later, once
    /// the retained files are known (`play_track`).
    pub(super) fn take_preloaded_prepared(&mut self) -> Option<PlaybackPreparedTrack> {
        // Clone the Arc so the borrow of the slot ends before we mutate
        // `preloaded_next` (both are fields of self).
        let current_source = self.current_source().cloned();
        discard_preloaded_next(&mut self.preloaded_next, current_source.as_ref())
    }

    pub(super) fn clear_next_track_state(&mut self) {
        let Some(prepared) = self.take_preloaded_prepared() else {
            return;
        };
        // Release the preload's buffers; files the current track still plays
        // stay cached and alive.
        let retained_file_ids = match &self.slot {
            PlaybackSlot::Active(cur) => cur.prepared.file_ids(),
            _ => HashSet::new(),
        };
        prepared.release_buffers(&retained_file_ids, &mut self.shared_file_buffers);
    }

    /// Whether a preloaded next-track source is available.
    pub(super) fn has_preloaded_next(&self) -> bool {
        let Some(preloaded) = &self.preloaded_next else {
            return false;
        };
        match &preloaded.source {
            PreloadedNextSource::Held(_) => true,
            PreloadedNextSource::Staged => self
                .current_source()
                .is_some_and(|gapless| gapless.lock().unwrap().has_next()),
        }
    }

    /// Promote the track bookkeeping after the audio callback crossed a gapless
    /// boundary. The `PlaybackSource` already advanced to the staged next track
    /// within the same stream; this reports the finishing track's decode stats,
    /// updates service state to match, and preloads the following track. No stream
    /// rebuild, no UI gap.
    ///
    /// Reads only the `TrackCrossing` payload — both track ids and the finishing
    /// stats come from the event, not from a shared cell.
    pub(super) async fn handle_track_crossed(&mut self, crossing: TrackCrossing) {
        // A gaplessly-advanced track never reaches the completion path, so this is
        // the completion log + stats for every track except an album's last.
        info!(
            "Track completed (gapless): {} ({} decode errors, {} samples)",
            crossing.finished_fmt.track_id, crossing.decode_error_count, crossing.samples_decoded
        );
        self.telemetry().event(TelemetryEvent::TrackCompleted {
            track_id: LocalId(crossing.finished_fmt.track_id.clone()),
            decode_errors: crossing.decode_error_count as u64,
        });
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
                self.telemetry_anomaly(AnomalyKind::GaplessBoundaryDesync);
                return;
            }
        };
        let PreloadedNext {
            prepared: next_prepared,
            decoder_handle,
            cancel_token,
            ..
        } = preloaded;
        let track_id = crossing.incoming_fmt.track_id.clone();
        info!("Gapless boundary: now playing {}", track_id);
        self.telemetry_track_started(&track_id, TrackTransition::Gapless);

        // Swap the finishing track's prepared + decoder for the incoming one in
        // place. The phase stays Playing — a crossing only fires while samples are
        // pulling.
        let Some(cur) = (match &mut self.slot {
            PlaybackSlot::Active(cur) => Some(cur),
            _ => None,
        }) else {
            warn!(
                track_id = %crossing.incoming_fmt.track_id,
                "Gapless boundary fired with no active track; ignoring"
            );
            self.telemetry_anomaly(AnomalyKind::GaplessBoundaryDesync);
            return;
        };
        // Release the previous track's buffers (files shared with the new one
        // stay cached and alive).
        cur.prepared
            .release_buffers(&next_prepared.file_ids(), &mut self.shared_file_buffers);
        cur.prepared = next_prepared;
        // Swap in the crossed-into decoder. The finishing track's already exited
        // (that is what fired the crossing), so dropping its handle detaches
        // nothing live.
        cur.decoder = TrackDecoder {
            handle: decoder_handle,
            cancel_token,
        };

        // The crossed-into track is playing: hand its reader fetch priority over
        // the track preloaded below.
        self.mark_current_foreground();

        self.advance_to_preloaded();

        // A natural transition starts at position 0 (pregap included).
        *self.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

        self.sync_audio_state();
        self.emit_state();

        self.preload_queue_front().await;

        // This boundary advanced the current track without play_track.
        self.persist_playback_state().await;
    }

    /// Advance the queue's current pointer past the finished track to the front,
    /// and emit the queue update. Used by `Next`, `AutoAdvance` (preloaded path),
    /// and the gapless boundary handler. The front IS the track being played: the
    /// preload refreshes whenever the queue mutates, so the front we advance to
    /// and the track these callers go on to play are the same one.
    pub(super) fn advance_to_preloaded(&mut self) {
        if self.playback_queue.advance_to_front().is_none() {
            warn!("advance_to_preloaded: queue had no front to advance to");
            self.telemetry_anomaly(AnomalyKind::PreloadStateMissing);
        }
        self.emit_queue_update();
    }

    /// Advance the queue to the preloaded next track, then start its buffered
    /// stream if it's ready, or play it fresh otherwise. `natural` (pregap
    /// included) and `target` pass through to the player. Shared by `Next` and
    /// `AutoAdvance`, which differ only in those two.
    pub(super) async fn advance_and_play_preloaded(
        &mut self,
        preloaded_track_id: &str,
        natural: bool,
        target: PlayTarget,
    ) {
        // The shared `natural` bool is exactly the manual-skip vs auto-advance
        // distinction — the same one `Next`/`AutoAdvance` pass in. Each branch's
        // sink reports the start itself (gated on a Playing target): the
        // streaming path inside `play_preloaded_track`, the rebuild path inside
        // `play_track`.
        let transition = if natural {
            TrackTransition::AutoAdvance
        } else {
            TrackTransition::Manual
        };
        if self.has_preloaded_next() {
            info!("Using preloaded track: {}", preloaded_track_id);
            self.advance_to_preloaded();
            self.play_preloaded_track(natural, target, transition).await;
        } else {
            // A preload started but has no stream yet. `play_track` discards the
            // half-ready preload itself and keeps its file buffers — it is about
            // to play the same track.
            self.advance_to_preloaded();
            self.play_track(
                preloaded_track_id,
                TrackStart::from_natural_transition(natural),
                target,
                transition,
            )
            .await;
        }
    }

    /// Play the preloaded next track, whose decoder is already running from
    /// `preload_next_track`: attach its stream to the persistent output, tear the
    /// outgoing track down, and install it as current.
    /// - `is_natural_transition`: play from INDEX 00 (pregap included) rather
    ///   than skipping the pregap.
    /// - `target`: where it lands once audio is ready (Playing, or Paused with a
    ///   reason). Computed absolutely by the caller.
    pub(super) async fn play_preloaded_track(
        &mut self,
        is_natural_transition: bool,
        target: PlayTarget,
        transition: TrackTransition,
    ) {
        let preloaded = match self.preloaded_next.take() {
            Some(preloaded) => preloaded,
            None => {
                error!("play_preloaded_track called without preloaded state");
                self.telemetry_anomaly(AnomalyKind::PreloadStateMissing);
                return;
            }
        };
        let PreloadedNext {
            prepared: next_prepared,
            decoder_handle,
            cancel_token,
            source: preloaded_source,
        } = preloaded;

        let pregap_ms = next_prepared.total_pregap_ms();
        let track_id = next_prepared.track_info.track_id.clone();

        // The preload decoded from the track's first sample (pregap included), so
        // a direct selection that skips the pregap can't use it: discard its
        // decoder and re-decode through play_track, which seeks past the pregap.
        if !is_natural_transition && pregap_ms.is_some_and(|p| p > 0) {
            info!("Pregap skip needed for preloaded track - falling back to play_track");
            let detached = detach_preloaded_source(self.current_source(), preloaded_source);
            discard_preloaded_decoder(&next_prepared, detached, &cancel_token);
            self.play_track(
                &track_id,
                TrackStart::from_natural_transition(is_natural_transition),
                target,
                transition,
            )
            .await;
            return;
        }

        // The streaming path below swaps the buffered decoder in place without
        // reaching `play_track`, so report the start here (gated on a Playing
        // target — a paused load isn't a start). The pregap-skip fallback above
        // is covered by `play_track`.
        if matches!(target, PlayTarget::Playing) {
            self.telemetry_track_started(&track_id, transition);
        }

        // Recover the incoming stream BEFORE attaching it: in the staged case it
        // lives inside the current source until this `take_next` pulls it out.
        let track_stream = detach_preloaded_source(self.current_source(), preloaded_source)
            .expect("Preloaded track has no streaming source");

        // The preload always decoded from INDEX 00, so this stream starts at 0.
        let fmt = next_prepared.track_fmt(std::time::Duration::ZERO);
        let sample_rate = next_prepared.sample_rate;
        let channels = next_prepared.channels;

        // Attach the preloaded stream to the persistent output: same format swaps
        // the callback's source in place; a format change rebuilds the stream. On
        // a rebuild-build failure the incoming decoder is left with no consumer —
        // cancel it and hard-fail, mirroring play_track when audio can't start.
        if self
            .attach_track(track_stream, fmt, sample_rate, channels)
            .await
            .is_err()
        {
            cancel_token.store(true, std::sync::atomic::Ordering::Release);
            for segment in &next_prepared.segments {
                segment.buffer.wake_readers();
            }
            drop(decoder_handle);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Couldn't start audio output for the next track.",
                    ),
                },
            );
            self.stop().await;
            return;
        }

        // The incoming source is live; tear the OUTGOING decoder down and release
        // its buffers now that the incoming track's files are known — files they
        // share stay cached and alive. The incoming decoder's own token
        // (`cancel_token`) is untouched by that teardown.
        if let Some(outgoing) = self.teardown_current_track() {
            outgoing.release_buffers(&next_prepared.file_ids(), &mut self.shared_file_buffers);
        }

        *self.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

        self.install_active_track(
            next_prepared,
            TrackDecoder {
                handle: decoder_handle,
                cancel_token,
            },
            target.into_track_phase(),
        );
        self.emit_state();

        self.preload_queue_front().await;

        // This advance bypasses play_track, so persist the now-playing track here.
        self.persist_playback_state().await;
    }
}
