use super::*;

impl PlaybackService {
    /// Designate the current track's buffer as the foreground for fetch
    /// priority, so its reader fetches immediately and a next-track preload's
    /// reader yields to it. Called wherever a track becomes the current one.
    pub(super) fn mark_current_foreground(&self) {
        if let PlaybackSlot::Active(cur) = &self.slot {
            if let Some(segment) = cur.prepared.segments.first() {
                self.fetch_arbiter.set_foreground(segment.buffer.id());
            }
        }
    }

    /// Mint a fresh load generation. Each fresh play or seek gets its own so a
    /// `TrackReady` from an abandoned load can be told from the live one.
    pub(super) fn next_load_generation(&mut self) -> LoadGeneration {
        let generation = LoadGeneration(self.load_generation_counter);
        self.load_generation_counter += 1;
        generation
    }

    pub(super) fn discard_preloaded_source(&mut self, source: PreloadedNextSource) {
        match source {
            PreloadedNextSource::Held(source) => source.cancel(),
            PreloadedNextSource::Staged => {
                if let Some(output) = &self.output {
                    output.source.lock().unwrap().cancel_staged_next();
                }
            }
        }
    }

    pub(super) fn retire_preloaded_track(&mut self) -> bool {
        let Some(preloaded) = self.preloaded_next.take() else {
            return false;
        };
        let PreloadedNext {
            prepared,
            source,
            cancel_token,
            ..
        } = preloaded;
        self.discard_preloaded_source(source);
        discard_preloaded_decoder(&prepared, &cancel_token);
        self.retired_tracks.push(prepared);
        true
    }

    pub(super) fn retire_current_track(&mut self) -> bool {
        if let PlaybackSlot::Active(current) =
            std::mem::replace(&mut self.slot, PlaybackSlot::Stopped)
        {
            current
                .decoder
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Release);
            self.retired_tracks.push(current.prepared);
            true
        } else {
            false
        }
    }

    pub(super) fn release_retired_tracks(&mut self, retained_file_ids: HashSet<String>) {
        let retained_file_ids = retained_file_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        for prepared in self.retired_tracks.drain(..) {
            prepared.release_buffers(&retained_file_ids, &mut self.shared_file_buffers);
        }
    }

    pub(super) fn discard_current_track(&mut self) {
        self.retire_current_track();
        self.release_retired_tracks(HashSet::new());
    }

    pub(super) async fn start_decoder_and_install(
        &mut self,
        prepared: PlaybackPreparedTrack,
        decode: StreamDecodeParams,
        fmt: TrackFmt,
        generation: LoadGeneration,
        staged_next: StagedNextOnReplace,
        phase: TrackPhase,
        outgoing_decoder: Option<(TrackDecoder, Vec<SharedSparseBuffer>)>,
    ) -> Result<(), PlaybackError> {
        let position_offset = fmt.position_offset;
        let track_id = prepared.track_info.track_id.clone();
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;

        let (track_stream, handle, cancel_token, ready) = spawn_decoder(
            decode,
            sample_rate,
            channels,
            "Streaming decode",
            DecodeFailureReport::EmitPlaybackError {
                progress_tx: self.progress_tx.clone(),
            },
            |track_stream, handle, cancel_token, ready| (track_stream, handle, cancel_token, ready),
        );

        if let Err(error) = self
            .attach_track(track_stream, fmt, sample_rate, channels, staged_next)
            .await
        {
            cancel_token.store(true, std::sync::atomic::Ordering::Release);
            for segment in &prepared.segments {
                segment.buffer.wake_readers();
            }
            drop(handle);
            if let Some((decoder, buffers)) = outgoing_decoder {
                cancel_and_join_decoder(&decoder.cancel_token, &buffers, decoder.handle).await;
            }
            return Err(error);
        }

        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        let command_tx = self.command_tx.clone();
        tokio::spawn(async move {
            match ready.await {
                Ok(()) => dispatch_command(
                    &command_tx,
                    PlaybackCommand::TrackReady {
                        track_id,
                        generation,
                    },
                ),
                Err(_) => {
                    debug!("ready watcher dropped before signal for track {track_id}")
                }
            }
        });

        if let Some((decoder, buffers)) = outgoing_decoder {
            cancel_and_join_decoder(&decoder.cancel_token, &buffers, decoder.handle).await;
        }

        self.install_active_track(
            prepared,
            TrackDecoder {
                handle,
                cancel_token,
            },
            phase,
        );
        Ok(())
    }

    /// Drive playback down to Stopped after a mid-flight read/decode failure
    /// surfaced a `PlaybackError`. The self-handled failure paths in `play_track`
    /// emit `PlaybackError` AND call `stop()` synchronously before returning to
    /// the loop, so by the time this dequeues the slot is already Stopped and a
    /// second `stop()` would emit a duplicate Stopped — no-op there. A genuine
    /// mid-flight failure leaves the slot Active, so the teardown fires. The
    /// serial loop can't interleave a new load between `stop()` finishing and
    /// this running, so the guard is race-free. `stop()` emits no `PlaybackError`,
    /// so this can't feed back into the self-subscription that dispatched it.
    pub(super) async fn halt_on_error(&mut self) {
        if matches!(self.slot, PlaybackSlot::Stopped) {
            debug!("HaltOnError: playback already stopped, nothing to halt");
        } else {
            // A slot still Active here is a genuine mid-flight read/decode failure
            // (the inline failure paths stop() before their HaltOnError dequeues,
            // leaving the slot Stopped). That is the read-failure operation.
            self.telemetry_playback_failed(PlaybackOperation::Read);
            self.stop().await;
        }
    }

    /// A track buffer's byte fill failed. What that breaks depends on which track
    /// the buffer feeds, which is why the fill reports here instead of straight to
    /// the UI:
    /// - the current track's bytes: nothing can play, so surface the error, which
    ///   dispatches `HaltOnError` and tears playback down;
    /// - a preloaded next track's bytes: the current track is unaffected and keeps
    ///   playing. Discard the preload rather than leave a staged decoder whose
    ///   bytes are gone — its buffer is cancelled by now, so the boundary would
    ///   otherwise cross into a truncated track. The queue reaches that track
    ///   through `play_track` instead, which prepares it afresh and reports the
    ///   failure then if it persists;
    /// - neither: the buffer left the pipeline (released or stopped) before this
    ///   failure surfaced, so there is nothing left to halt.
    pub(super) async fn handle_read_failed(&mut self, buffer_id: u64, error: PlaybackError) {
        if let PlaybackSlot::Active(cur) = &self.slot {
            if cur.prepared.reads_buffer(buffer_id) {
                error!(
                    track_id = %cur.prepared.track_info.track_id,
                    "read failed for the playing track; halting playback"
                );
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: error.into_ui_reason(),
                    },
                );
                return;
            }
        }

        if let Some(preloaded) = &self.preloaded_next {
            if preloaded.prepared.reads_buffer(buffer_id) {
                warn!(
                    track_id = %preloaded.track_id(),
                    "read failed for the preloaded next track; discarding the preload and playing on"
                );
                self.telemetry_playback_failed(PlaybackOperation::Preload);
                self.clear_next_track_state();
                return;
            }
        }

        debug!("ignoring read failure on a buffer that left the pipeline: {error}");
    }

    pub(super) fn record_telemetry(&self, event: TelemetryEvent) {
        self.library_manager.record_telemetry(event);
    }

    /// Restore playback from the device-local resume cache at startup, unless
    /// "Restore on launch" is off (the row is kept either way — it stays the
    /// crash-safe resume point). A present-and-valid row replays; a corrupt row
    /// (DB-structural or a per-lane out-of-domain value) is counted and cleared;
    /// an absent row or a read failure starts fresh.
    pub(super) async fn restore_from_cache(&mut self, restore_playback: bool) {
        if !restore_playback {
            debug!("restore on launch is off; starting with nothing in playback");
            return;
        }
        use crate::db::LoadedPlaybackState;
        match self.library_manager.load_playback_state().await {
            Ok(LoadedPlaybackState::Present(state)) => match PersistedPlayback::from_row(state) {
                Some(parsed) => self.restore(parsed).await,
                // The row parsed as corrupt (a per-lane out-of-domain value; logged
                // in `from_row`). Count it and delete it so the bad row doesn't
                // linger durably across restarts.
                None => self.discard_corrupt_resume_cache().await,
            },
            // A structurally-impossible row (source XOR cursor); the DB client
            // logged the specific mismatch.
            Ok(LoadedPlaybackState::Corrupt) => self.discard_corrupt_resume_cache().await,
            Ok(LoadedPlaybackState::Absent) => {}
            Err(e) => warn!("couldn't load the saved playback state: {e}; starting fresh"),
        }
    }

    /// The resume cache was corrupt (a DB-structural mismatch or a per-lane
    /// out-of-domain value): count the anomaly and delete the row so the bad
    /// state doesn't linger durably across restarts. The pure parse/DB layers
    /// only detect and log it; the service, which owns the diagnostics sink,
    /// emits and clears.
    pub(super) async fn discard_corrupt_resume_cache(&self) {
        self.telemetry_anomaly(AnomalyKind::ResumeCacheCorrupt);
        if let Err(e) = self.library_manager.clear_playback_state().await {
            warn!("couldn't clear the corrupt playback resume cache: {e}");
        }
    }

    /// A play command established a new playing context.
    pub(super) fn telemetry_playback_started(
        &self,
        source: PlaybackStartSource,
        track_count: usize,
    ) {
        self.record_telemetry(TelemetryEvent::PlaybackStarted {
            source,
            track_count: track_count as u32,
        });
    }

    /// A track start was initiated (context established, playback requested).
    pub(super) fn telemetry_track_started(&self, track_id: &str, transition: TrackTransition) {
        self.record_telemetry(TelemetryEvent::TrackStarted {
            track_id: LocalId(track_id.to_string()),
            transition,
        });
    }

    /// A playback operation failed. The free-text cause stays in the local
    /// `error!` next to the call; only the coarse operation ships.
    pub(super) fn telemetry_playback_failed(&self, operation: PlaybackOperation) {
        self.record_telemetry(TelemetryEvent::PlaybackFailed { operation });
    }

    /// An impossible-state site fired. The free-text detail stays in the local
    /// `warn!`/`error!` next to the call; only the kind (a count) ships.
    pub(super) fn telemetry_anomaly(&self, kind: AnomalyKind) {
        self.record_telemetry(TelemetryEvent::Anomaly { kind });
    }

    /// Direct selection of `track_id`: its release becomes the playing context,
    /// with the cursor at the chosen track. `get_play_context`'s `Err` covers
    /// only DB failures and data-inconsistency (`release_id` is a required
    /// column, so there is no legitimate "track with no context" case) — fail
    /// loud rather than silently falling back to a single-track queue.
    pub(super) async fn handle_play(&mut self, track_id: String) {
        self.stop_preview_for_main_playback();
        let context = match self.library_manager.get_play_context(&track_id).await {
            Ok(context) => context,
            Err(e) => {
                error!("Failed to load play context for {track_id}: {e}");
                self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: PlaybackError::database(e).into_ui_reason(),
                    },
                );
                return;
            }
        };
        let track_count = context.track_ids.len();
        self.playback_queue.play_release(
            ContextSource::Release(context.release_id),
            context.track_ids,
            ContextStart::Index(context.index),
        );
        self.emit_queue_update();
        self.telemetry_playback_started(PlaybackStartSource::Release, track_count);
        self.play_track(
            &track_id,
            TrackStart::Direct,
            PlayTarget::Playing,
            TrackTransition::Manual,
        )
        .await;
    }

    /// Manual skip to the next track (skip pregap). A side-pause forces the next
    /// side to start playing; any other state carries the outgoing track's
    /// play/pause intent forward (a naturally-completed track carries Playing, so
    /// the skip resumes audibly rather than inheriting the Stopped atomic).
    pub(super) async fn handle_next(&mut self) {
        let target = if self.is_side_paused() {
            PlayTarget::Playing
        } else {
            self.current_play_target()
        };
        if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
            self.advance_and_play_preloaded(&preloaded_track_id, false, target)
                .await;
        } else {
            // Nothing preloaded: let the queue decide what plays next.
            match self.playback_queue.next_entry() {
                NextEntry::Play(next_track) => {
                    info!("No preloaded track, playing from queue: {}", next_track);
                    self.emit_queue_update();
                    self.play_track(
                        &next_track,
                        TrackStart::Direct,
                        target,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                _ => {
                    info!("No next track available, stopping");
                    self.emit_queue_update();
                    self.stop().await;
                }
            }
        }
    }

    /// Manual step to the previous track, or restart the current one when it's
    /// played past the restart threshold. Carries the outgoing track's play/pause
    /// intent to the track we land on (a side-pause collapses to a plain manual
    /// pause across the track change).
    pub(super) async fn handle_previous(&mut self) {
        let Some(current_track_id) = self.current_track_id().map(|s| s.to_string()) else {
            debug!("Previous command received with no current track; ignoring");
            return;
        };
        let target = self.current_play_target();
        let current_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(std::time::Duration::ZERO);
        let position_ms = current_position.as_millis() as u64;

        match self.playback_queue.previous_action(position_ms) {
            PreviousAction::PlayPrevious(previous_track_id) => {
                info!("Going to previous track: {}", previous_track_id);
                // previous_action already stepped the context cursor back and made
                // this track current; just play it.
                self.emit_queue_update();
                self.play_track(
                    &previous_track_id,
                    TrackStart::Direct,
                    target,
                    TrackTransition::Manual,
                )
                .await;
            }
            PreviousAction::RestartCurrent => {
                info!("Restarting current track from beginning");
                self.play_track(
                    &current_track_id,
                    TrackStart::Direct,
                    target,
                    TrackTransition::Manual,
                )
                .await;
            }
        }
    }

    /// Advance to the next track after `track_id` completed naturally (pregap
    /// played). Dispatched from the `TrackCompleted` progress event.
    ///
    /// Only advances if `track_id` is still the current track AND still in the
    /// `Completed` phase. `AutoAdvance` rides the command queue behind whatever
    /// the user did, so a `Next` (a different track is now current) or a `Seek`
    /// (the same track, but the seek reset its phase off `Completed`) that reached
    /// the loop first already moved on — advancing again would skip the track
    /// `Next` landed on, or abandon the `Seek`. Either mismatch drops the advance.
    pub(super) async fn handle_auto_advance(&mut self, track_id: String) {
        let still_completed = matches!(
            &self.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_info.track_id == track_id
                    && matches!(cur.phase, TrackPhase::Completed)
        );
        if !still_completed {
            debug!(
                track_id,
                "ignoring stale AutoAdvance: the completed track is no longer current-and-Completed"
            );
            return;
        }
        match self.side_pause_for_queue_front().await {
            Ok(Some(decision)) => {
                self.pause_for_side_end(decision);
                return;
            }
            Ok(None) => {}
            Err(()) => {
                self.stop().await;
                return;
            }
        }

        // Repeat-track replays the current track, so its preload (the queue's
        // next) is not what plays.
        if self.playback_queue.repeat_mode() != RepeatMode::Track {
            if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
                self.advance_and_play_preloaded(&preloaded_track_id, true, PlayTarget::Playing)
                    .await;
                return;
            }
        }

        match self.playback_queue.next_entry() {
            NextEntry::RepeatCurrent(next_track) => {
                info!("Repeat mode: track, replaying {}", next_track);
                self.play_track(
                    &next_track,
                    TrackStart::Natural,
                    PlayTarget::Playing,
                    TrackTransition::Repeat,
                )
                .await;
            }
            NextEntry::Play(next_track) => {
                info!("Playing from queue: {}", next_track);
                self.emit_queue_update();
                self.play_track(
                    &next_track,
                    TrackStart::Natural,
                    PlayTarget::Playing,
                    TrackTransition::AutoAdvance,
                )
                .await;
            }
            NextEntry::Stop => {
                info!("No next track available, stopping");
                self.emit_queue_update();
                self.stop().await;
            }
        }
    }

    pub(super) async fn handle_position_event(
        &mut self,
        fmt: Arc<TrackFmt>,
        pos: std::time::Duration,
    ) {
        // A dead AirPlay receiver surfaces here — the audio thread keeps ticking
        // while its sends fail, so this regular path catches the failure and ends
        // AirPlay, resuming local paused (the shape a Cast/DLNA `ended` status has).
        if self.renderer.airplay_failed() {
            info!("airplay receiver unreachable; ending AirPlay and resuming local");
            self.end_airplay_and_resume_local().await;
            return;
        }

        // Samples are flowing, so any starvation episode is over.
        self.reset_starvation_episode();
        let mut actual_pos = fmt.position_offset + pos;
        // On AirPlay the drain has pulled (and the stream sent) audio the receiver
        // has not yet played — the ~2 s buffer ahead. Offset the position back by
        // the receiver latency so the bar matches what is audible, not what has
        // been transmitted.
        if let Some(latency) = self.renderer.airplay_latency() {
            actual_pos = actual_pos.saturating_sub(latency);
        }
        *self.current_position_shared.lock().unwrap() = Some(actual_pos);
        let raw_pos_ms = actual_pos.as_millis() as u64;
        let progress =
            crate::playback::format::compute_progress(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
        let (adjusted_pos_ms, adjusted_dur_ms) =
            crate::playback::format::adjust_for_pregap(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PositionUpdate {
                position_ms: adjusted_pos_ms,
                duration_ms: adjusted_dur_ms,
                track_id: fmt.track_id.clone(),
                progress,
            },
        );

        // Persist the resume point at most once a second while the track is
        // actually advancing (Playing) — that is what keeps `playback_state`
        // crash-safe between the discrete events (load/pause/seek/stop) that
        // persist on their own. A Loading or Paused tick skips it: position isn't
        // moving, and pause/seek already wrote the row.
        let playing = matches!(
            &self.slot,
            PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Playing)
        );
        if playing {
            let due = match self.last_position_persist {
                Some(last) => last.elapsed() >= std::time::Duration::from_secs(1),
                None => true,
            };
            if due {
                self.persist_playback_state().await;
            }
        }
    }

    pub(super) fn handle_completion_event(
        &mut self,
        fmt: Arc<TrackFmt>,
        error_count: u32,
        samples_decoded: u64,
    ) {
        info!(
            "Track completed: {} ({} decode errors, {} samples)",
            fmt.track_id, error_count, samples_decoded
        );
        self.record_telemetry(TelemetryEvent::TrackCompleted {
            track_id: LocalId(fmt.track_id.clone()),
            decode_errors: error_count as u64,
        });
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::TrackCompleted {
                track_id: fmt.track_id.clone(),
            },
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::DecodeStats {
                track_id: fmt.track_id.clone(),
                error_count,
                samples_decoded,
            },
        );
        // The track drained: mark the phase Completed. The persistent output stays
        // live because AutoAdvance and the side-pause decision still read the
        // source; the audio callback already flipped the atomic to Stopped (which
        // `sync_audio_state` below confirms), so it emits nothing further until
        // the source is replaced.
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Completed;
        }
        self.sync_audio_state();
    }

    pub(super) async fn handle_audio_event(&mut self, event: AudioEvent) {
        // Position/Completion/TrackCrossing carry their own logging (or none);
        // every other kind goes to the diagnostic log both players share.
        if !matches!(
            event,
            AudioEvent::Position(_) | AudioEvent::Completion(_) | AudioEvent::TrackCrossing(_)
        ) {
            log_stream_diagnostic("playback", &event);
        }
        match event {
            AudioEvent::Position((fmt, pos)) => self.handle_position_event(fmt, pos).await,
            AudioEvent::Completion((fmt, error_count, samples_decoded)) => {
                self.handle_completion_event(fmt, error_count, samples_decoded);
            }
            AudioEvent::TrackCrossing(crossing) => self.handle_track_crossed(crossing).await,
            AudioEvent::Starved {
                fmt,
                starved_ms,
                producer_finished,
                samples_decoded,
                ..
            } => {
                // `producer_finished` means a drained track awaiting AutoAdvance,
                // not the stall this watchdog targets.
                if !producer_finished {
                    self.handle_starvation(&fmt.track_id, samples_decoded, starved_ms)
                        .await;
                }
            }
            AudioEvent::StarvationEnded { .. } => {
                self.reset_starvation_episode();
            }
            AudioEvent::SourceLockMissed { .. } | AudioEvent::SourceLockReacquired { .. } => {}
        }
    }

    pub(super) async fn drain_current_audio_events(&mut self) {
        while let Some(event) = self.output.as_mut().and_then(|o| o.audio_events.pop()) {
            self.handle_audio_event(event).await;
        }

        let dropped_required = self
            .output
            .as_ref()
            .map(|o| report_dropped_audio_events(&o.audio_events, "playback"))
            .unwrap_or(false);
        if dropped_required {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Playback event queue dropped a required audio event".to_string(),
                    ),
                },
            );
            self.stop().await;
        }
    }
    /// `restore_playback` controls whether startup restores the saved queue,
    /// track, and position. The crash-safe row stays current either way: track
    /// loads, stops, and queue edits write it immediately, and active playback
    /// writes it at most once a second.
    pub(crate) fn start(
        library_manager: LibraryManager,
        queue_ids: coven::IdRef,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            queue_ids,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            None,
        )
    }

    /// Start over a caller-supplied audio device, for tests that capture samples
    /// — both the main player's output and any preview's come from it, so a test
    /// touches no audio hardware.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn start_with_audio_device(
        library_manager: LibraryManager,
        queue_ids: coven::IdRef,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_device: Box<dyn AudioOutputDevice>,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            queue_ids,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            Some(audio_device),
        )
    }

    pub(super) fn start_inner(
        library_manager: LibraryManager,
        queue_ids: coven::IdRef,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        custom_device: Option<Box<dyn AudioOutputDevice>>,
    ) -> PlaybackHandle {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let progress_handle = PlaybackProgressHandle::new(progress_rx, runtime_handle.clone());
        let playback_queue = PlaybackQueue::new(queue_ids);
        let (queue_values, queue_receiver) =
            tokio::sync::watch::channel(PlaybackQueueProjection::from_queue(&playback_queue));
        let thread_slot = Arc::new(Mutex::new(None));
        let handle = PlaybackHandle::new(
            command_tx.clone(),
            progress_handle.clone(),
            queue_receiver,
            thread_slot.clone(),
        );
        let command_tx_for_completion = command_tx.clone();
        let progress_handle_for_completion = progress_handle.clone();
        runtime_handle.spawn(async move {
            let mut progress_rx = progress_handle_for_completion.subscribe_all();
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    PlaybackProgress::TrackCompleted { track_id } => {
                        info!(
                            "Auto-advance: Track completed, sending AutoAdvance command: {}",
                            track_id
                        );
                        dispatch_command(
                            &command_tx_for_completion,
                            PlaybackCommand::AutoAdvance { track_id },
                        );
                    }
                    // A mid-flight read failure cancels the buffer and the decoder
                    // exits without a TrackCompleted, so without this the UI would
                    // sit in Playing forever.
                    PlaybackProgress::PlaybackError { .. } => {
                        dispatch_command(&command_tx_for_completion, PlaybackCommand::HaltOnError);
                    }
                    _ => {}
                }
            }
        });
        let join_handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime");
            rt.block_on(async move {
                let Some((audio_device, audio_output)) =
                    open_audio_device_and_output(custom_device, &command_tx)
                else {
                    return;
                };
                let preview = PreviewPlayer::new(
                    progress_tx.clone(),
                    command_tx.clone(),
                    position_update_interval_ms,
                );
                let mut service = PlaybackService {
                    library_manager,
                    command_tx: command_tx.clone(),
                    command_rx,
                    progress_tx,
                    queue_values,
                    playback_queue,
                    current_position_shared: Arc::new(std::sync::Mutex::new(None)),
                    audio_device,
                    audio_output,
                    output: None,
                    slot: PlaybackSlot::Stopped,
                    load_generation_counter: 0,
                    preloaded_next: None,
                    preview,
                    main_was_playing_before_preview: false,
                    is_muted: false,
                    pre_mute_volume: 1.0,
                    position_update_interval_ms,
                    shared_file_buffers: HashMap::new(),
                    retired_tracks: Vec::new(),
                    fetch_arbiter: FetchArbiter::new(),
                    starvation_episode: None,
                    last_position_persist: None,
                    first_audio_pending: None,
                    renderer: Renderer::Local,
                };
                service.restore_from_cache(restore_playback).await;
                service.run().await;
            });
        });
        *thread_slot.lock().unwrap() = Some(join_handle);
        handle
    }

    pub(super) async fn apply_repeat_mode(&mut self, mode: RepeatMode) {
        if self.playback_queue.repeat_mode() == mode {
            return;
        }

        self.playback_queue.set_repeat_mode(mode);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RepeatModeChanged { mode },
        );
        self.emit_queue_update();
        self.persist_playback_state().await;
    }

    pub(super) async fn run(&mut self) {
        info!("PlaybackService started");
        let mut library_event_rx = self.library_manager.subscribe_events();
        let mut audio_event_tick = tokio::time::interval(std::time::Duration::from_millis(10));
        audio_event_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = audio_event_tick.tick(), if self.output.is_some() => {
                    self.drain_current_audio_events().await;
                }
                Some(command) = self.command_rx.recv() => {
            // One event per user-intent command, at the point the loop picks it
            // up. Internal/system commands, queries, and continuous inputs map to
            // `None` and ship nothing.
            if let Some(kind) = playback_command_kind(&command) {
                self.record_telemetry(TelemetryEvent::PlaybackCommand { command: kind });
            }
            match command {
                PlaybackCommand::Play(track_id) => {
                    self.handle_play(track_id).await;
                }
                PlaybackCommand::PlayRelease { release_id, start_track_index, shuffle } => {
                    let track_ids = match self.library_manager.get_track_ids(&release_id).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            error!("Failed to get tracks for release {release_id}: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                            continue;
                        }
                    };

                    if track_ids.is_empty() {
                        warn!("PlayRelease: release {release_id} has no tracks");
                        continue;
                    }

                    self.stop_preview_for_main_playback();
                    let start = if shuffle {
                        // The seed is minted once, here, and carried into the
                        // context, so the order is reproducible and `Context`
                        // repeat can re-derive it.
                        ContextStart::Shuffled {
                            seed: rand::random(),
                        }
                    } else {
                        // `None` means "from the first track"; an out-of-range
                        // index is a bad caller value — clamp and log.
                        let index = match start_track_index {
                            Some(i) if i < track_ids.len() => i,
                            Some(i) => {
                                warn!(
                                    "PlayRelease: start index {i} out of range for {} tracks; starting at 0",
                                    track_ids.len()
                                );
                                0
                            }
                            None => 0,
                        };
                        ContextStart::Index(index)
                    };
                    let track_count = track_ids.len();
                    let first_track = self.playback_queue.play_release(
                        ContextSource::Release(release_id),
                        track_ids,
                        start,
                    );
                    self.emit_queue_update();
                    self.telemetry_playback_started(PlaybackStartSource::Release, track_count);
                    self.play_track(
                        &first_track,
                        TrackStart::Direct,
                        PlayTarget::Playing,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                PlaybackCommand::PlayReleases(release_ids) => {
                    // The source is exactly the releases that contributed tracks,
                    // so a later shuffle/restore re-fetch stays in lockstep.
                    let (playable_ids, track_ids) =
                        self.load_release_set_tracks(release_ids).await;
                    if track_ids.is_empty() {
                        warn!("PlayReleases: no playable releases; nothing to play");
                        continue;
                    }
                    self.stop_preview_for_main_playback();
                    let track_count = track_ids.len();
                    let first_track = self.playback_queue.play_release(
                        ContextSource::releases(playable_ids),
                        track_ids,
                        ContextStart::Index(0),
                    );
                    self.emit_queue_update();
                    self.telemetry_playback_started(PlaybackStartSource::Releases, track_count);
                    self.play_track(
                        &first_track,
                        TrackStart::Direct,
                        PlayTarget::Playing,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                PlaybackCommand::PlayLibraryShuffled => {
                    let track_ids = match self.fetch_source_tracks(&ContextSource::Library).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            error!("PlayLibraryShuffled: couldn't load library tracks: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                            continue;
                        }
                    };
                    if track_ids.is_empty() {
                        warn!("PlayLibraryShuffled: the library has no tracks; nothing to play");
                        continue;
                    }
                    self.stop_preview_for_main_playback();
                    // A fresh seed, so the order is reproducible and `Context`
                    // repeat can re-derive it.
                    let track_count = track_ids.len();
                    let first_track = self.playback_queue.play_release(
                        ContextSource::Library,
                        track_ids,
                        ContextStart::Shuffled {
                            seed: rand::random(),
                        },
                    );
                    self.emit_queue_update();
                    self.telemetry_playback_started(
                        PlaybackStartSource::LibraryShuffled,
                        track_count,
                    );
                    self.play_track(
                        &first_track,
                        TrackStart::Direct,
                        PlayTarget::Playing,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                PlaybackCommand::Pause => {
                    self.pause();
                }
                PlaybackCommand::Resume => {
                    self.resume().await;
                }
                PlaybackCommand::Stop => {
                    self.stop().await;
                }
                PlaybackCommand::Next => {
                    self.handle_next().await;
                }
                PlaybackCommand::AutoAdvance { track_id } => {
                    self.handle_auto_advance(track_id).await;
                }
                PlaybackCommand::TrackReady {
                    track_id,
                    generation,
                } => {
                    self.resolve_track_ready(track_id, generation);
                }
                PlaybackCommand::HaltOnError => {
                    self.halt_on_error().await;
                }
                PlaybackCommand::ReadFailed { buffer_id, error } => {
                    self.handle_read_failed(buffer_id, error).await;
                }
                #[cfg(target_os = "macos")]
                PlaybackCommand::OutputDeviceChanged => {
                    self.handle_output_device_changed().await;
                }
                PlaybackCommand::Previous => {
                    self.handle_previous().await;
                }
                PlaybackCommand::Seek(position) => {
                    self.seek(position).await;
                }
                PlaybackCommand::SeekByRatio(ratio) => {
                    let position_ms = if let PlaybackSlot::Active(cur) = &self.slot {
                        let prepared = &cur.prepared;
                        Some(crate::playback::format::position_for_progress(
                            ratio,
                            prepared.duration.as_millis() as u64,
                            prepared.total_pregap_ms(),
                        ))
                    } else {
                        None
                    };
                    if let Some(position_ms) = position_ms {
                        self.seek(std::time::Duration::from_millis(position_ms))
                            .await;
                    }
                }
                PlaybackCommand::SetVolume(volume) => {
                    self.set_volume(volume);
                }
                PlaybackCommand::SetMuted(muted) => {
                    if muted == self.is_muted {
                        // Already there: nothing changes, nothing emits.
                    } else if muted {
                        self.pre_mute_volume = self.audio_output.get_volume();
                        self.is_muted = true;
                        self.audio_output.set_volume(0.0);
                        // Receiver mute is flaky across devices, so muting maps
                        // to receiver volume 0; the pre-mute level is remembered
                        // above and restored on unmute.
                        self.renderer.set_remote_volume(0.0);
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: 0.0 },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: true },
                        );
                    } else {
                        self.is_muted = false;
                        let vol = self.pre_mute_volume;
                        self.audio_output.set_volume(vol);
                        self.renderer.set_remote_volume(vol);
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: vol },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: false },
                        );
                    }
                }
                PlaybackCommand::AddToQueue(track_ids) => {
                    let count = track_ids.len() as u32;
                    self.playback_queue.add_to_queue(track_ids);
                    self.emit_queue_items_added(count);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::AddNext(track_ids) => {
                    let count = track_ids.len() as u32;
                    self.playback_queue.add_next(track_ids);
                    self.emit_queue_items_added(count);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::AddReleaseToQueue(release_id) => {
                    match self.library_manager.get_track_ids(&release_id).await {
                        Ok(track_ids) if !track_ids.is_empty() => {
                            let count = track_ids.len() as u32;
                            self.playback_queue.add_to_queue(track_ids);
                            self.emit_queue_items_added(count);
                            self.on_queue_mutated().await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to add release {release_id} to queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
                        }
                    }
                }
                PlaybackCommand::AddReleaseNext(release_id) => {
                    match self.library_manager.get_track_ids(&release_id).await {
                        Ok(track_ids) if !track_ids.is_empty() => {
                            let count = track_ids.len() as u32;
                            self.playback_queue.add_next(track_ids);
                            self.emit_queue_items_added(count);
                            self.on_queue_mutated().await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to add release {release_id} next in queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
                        }
                    }
                }
                PlaybackCommand::InsertInQueue(track_ids, index) => {
                    let count = track_ids.len() as u32;
                    self.playback_queue.insert_at(index, track_ids);
                    self.emit_queue_items_added(count);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::RemoveFromQueue(entry_id) => {
                    if let Some(removed) = self.playback_queue.remove(&entry_id) {
                        if self
                            .current_track_id()
                            .map(|id| id == removed.track_id)
                            .unwrap_or(false)
                        {
                            self.stop().await;
                            self.emit_queue_update();
                        } else {
                            self.on_queue_mutated().await;
                        }
                    } else {
                        // The id named no queued entry — a stale UI action against a
                        // queue that already moved on.
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                    }
                }
                PlaybackCommand::ReorderQueue { entry_id, before } => {
                    if !self.playback_queue.reorder(&entry_id, before.as_ref()) {
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                    }
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ClearUpNext => {
                    self.playback_queue.clear_up_next();
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ClearPlayingFrom => {
                    self.playback_queue.clear_playing_from();
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::SetRepeatMode(mode) => {
                    self.apply_repeat_mode(mode).await;
                }
                PlaybackCommand::SetShuffle(on) => {
                    // The seed for the permutation `set_shuffle` performs on the
                    // rows it already holds; it uses it only when turning shuffle
                    // on. Shuffling changes what plays next, so the preload and
                    // the staged side-pause reconcile like any queue mutation.
                    let seed: u64 = rand::random();
                    self.playback_queue.set_shuffle(on, seed);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ReevaluateSidePauseStaging => {
                    self.reevaluate_side_pause_staging().await;
                }
                PlaybackCommand::SkipTo(entry_id) => {
                    if let Some(entry) = self.playback_queue.skip_to(&entry_id) {
                        info!(
                            "SkipTo: jumping to queue entry {}, track {}",
                            entry.id.0, entry.track_id
                        );

                        self.emit_queue_update();
                        self.play_track(
                            &entry.track_id,
                            TrackStart::Direct,
                            PlayTarget::Playing,
                            TrackTransition::Manual,
                        )
                        .await;
                    } else {
                        // The id named no queued entry — a stale UI action.
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                    }
                }
                PlaybackCommand::PreviewPlay(path) => {
                    self.preview_play(path).await;
                }
                PlaybackCommand::PreviewStop => {
                    self.preview_stop();
                }
                PlaybackCommand::PreviewTogglePause => {
                    self.preview_toggle_pause();
                }
                PlaybackCommand::PreviewSeekByRatio(ratio) => {
                    self.preview
                        .seek_by_ratio(ratio, self.audio_device.as_ref())
                        .await;
                }
                PlaybackCommand::PreviewCompleted => {
                    self.preview_completed();
                }
                PlaybackCommand::GetVolume(reply) => {
                    let _ = reply.send(self.audio_output.get_volume());
                }
                #[cfg(any(test, feature = "test-utils"))]
                PlaybackCommand::GetQueueProjection(reply) => {
                    let _ = reply.send(self.queue_projection());
                }
                PlaybackCommand::Shutdown(reply) => {
                    self.persist_playback_state().await;
                    let _ = reply.send(());
                    break;
                }
                PlaybackCommand::SaveState(reply) => {
                    self.persist_playback_state().await;
                    let _ = reply.send(());
                }
                PlaybackCommand::PlayOn(connect) => {
                    self.handle_play_on(*connect).await;
                }
                PlaybackCommand::PlayOnAirPlay(connect) => {
                    self.handle_play_on_airplay(*connect).await;
                }
                PlaybackCommand::StopRemote => {
                    self.handle_stop_remote().await;
                }
                PlaybackCommand::RemoteStatus(status) => {
                    self.handle_remote_status(status).await;
                }
            }
                }
                Ok(event) = library_event_rx.recv() => {
                    let LibraryEvent::TracksDeleted { track_ids } = event;
                    self.handle_tracks_deleted(track_ids).await;
                }
                else => break,
            }
        }
        info!("PlaybackService stopped");
    }
}
