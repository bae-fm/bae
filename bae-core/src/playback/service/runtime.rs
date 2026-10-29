use super::*;

use bae_util::worker_thread::WorkerThread;

impl PlaybackService {
    /// Hand the current track's reader fetch priority over a preload's. Called
    /// wherever a track becomes the current one.
    pub(super) fn mark_current_foreground(&self) {
        if let PlaybackSlot::Active(cur) = &self.slot {
            self.file_buffers.mark_foreground(&cur.prepared);
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
        self.file_buffers.retire(prepared);
        true
    }

    pub(super) fn retire_current_track(&mut self) -> bool {
        if let PlaybackSlot::Active(current) =
            std::mem::replace(&mut self.slot, PlaybackSlot::Stopped)
        {
            self.audible_completion = None;
            self.sync_audio_state();
            self.discard_buffered_listening();
            self.renderer.flush_airplay();
            self.capture_listening_position();
            current
                .decoder
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Release);
            self.file_buffers.retire(current.prepared);
            true
        } else {
            false
        }
    }

    pub(super) fn discard_current_track(&mut self) {
        self.retire_current_track();
        self.file_buffers.release_retired(&HashSet::new());
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
        let starts_at = fmt.starts_at;
        let track_id = prepared.track_id.clone();
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;

        let (track_stream, handle, cancel_token, ready) = spawn_decoder(
            DecoderSetup::new(
                decode,
                sample_rate,
                channels,
                "Streaming decode",
                DecodeFailureReport::EmitPlaybackError {
                    progress_tx: self.progress_tx.clone(),
                },
            ),
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
            starts_at,
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
    pub(super) async fn handle_read_failed(&mut self, buffer_id: u64, error: Arc<PlaybackError>) {
        if let PlaybackSlot::Active(cur) = &self.slot {
            if cur.prepared.reads_buffer(buffer_id) {
                error!(
                    track_id = %cur.prepared.track_id,
                    "read failed for the playing track; halting playback"
                );
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: error.ui_reason(),
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

    /// Restore playback from the device-local resume row at startup: a stored
    /// row resumes its track over the queue the tables hold; an absent row
    /// resumes nothing, and the library opened with the queue emptied
    /// (`settle_playback_queue`).
    pub(super) async fn restore_from_cache(&mut self) {
        match self.library_manager.load_playback_state().await {
            Ok(Some(state)) => self.restore(state).await,
            Ok(None) => {}
            Err(e) => warn!("couldn't load the saved playback state: {e}; resuming nothing"),
        }
    }

    /// A queue table write or read failed: the queue is as it was before the
    /// command, and the command does nothing more.
    pub(super) fn queue_failed(&self, doing: &str, error: crate::db::DbError) {
        error!("couldn't {doing}: {error}");
        self.telemetry_playback_failed(PlaybackOperation::QueueChange);
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
    /// from the chosen track. A track the library does not hold, or a failed
    /// fill, fails loud rather than falling back to a single-track queue.
    pub(super) async fn handle_play(&mut self, track_id: String) {
        self.stop_preview_for_main_playback();
        let failure = match self
            .playback_queue
            .play(ContextFill::ReleaseOfTrack(track_id.clone()))
            .await
        {
            Ok(ContextFilled::Playing { track_count, .. }) => {
                self.telemetry_playback_started(PlaybackStartSource::Release, track_count);
                None
            }
            Ok(ContextFilled::NoTracks | ContextFilled::StartMissing) => Some(
                crate::library::LibraryError::TrackMapping(format!("Track not found: {track_id}")),
            ),
            Err(e) => Some(crate::library::LibraryError::from(e)),
        };
        if let Some(e) = failure {
            error!("Failed to play {track_id} from its release: {e}");
            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: PlaybackError::database(e).ui_reason(),
                },
            );
            return;
        }
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
            match self.next_entry().await {
                NextEntry::Play(next_track) => {
                    info!("No preloaded track, playing from queue: {}", next_track);
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
        let (Some(current_track_id), Some(track_time)) = (
            self.current_track_id().map(str::to_string),
            self.current_track_time(),
        ) else {
            debug!("Previous command received with no current track; ignoring");
            return;
        };
        let target = self.current_play_target();

        let action = match self.playback_queue.previous_action(track_time).await {
            Ok(action) => action,
            Err(e) => {
                self.queue_failed("step the queue back", e);
                return;
            }
        };
        match action {
            PreviousAction::PlayPrevious(previous_track_id) => {
                info!("Going to previous track: {}", previous_track_id);
                // previous_action already stepped the context cursor back and made
                // this track current; just play it.
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
                if cur.prepared.track_id == track_id
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

        match self.next_entry().await {
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
                self.stop().await;
            }
        }
    }

    pub(super) async fn handle_position_event(
        &mut self,
        fmt: Arc<TrackFmt>,
        mut position: StreamPosition,
    ) {
        // A dead AirPlay receiver surfaces here — the audio thread keeps ticking
        // while its sends fail, so this regular path catches the failure and ends
        // AirPlay, resuming local paused (the shape a Cast/DLNA end has).
        if let Some(cause) = self.renderer.airplay_failure() {
            warn!("airplay receiver unreachable ({cause}); ending AirPlay and resuming local");
            self.end_airplay_and_resume_local(RemoteEnd::Lost(cause))
                .await;
            return;
        }

        // Samples are flowing, so any starvation episode is over.
        self.reset_starvation_episode();
        // On AirPlay the drain has pulled (and the stream sent) audio the receiver
        // has not yet played — the session's receiver latency ahead. Offset the
        // position back by it so the bar matches what is audible, not what has
        // been transmitted.
        if let Some(latency) = self.renderer.airplay_latency() {
            position = position.saturating_sub(latency);
        }
        self.observe_listening_position(fmt.clone(), position);
        self.checkpoint_listening().await;
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            if cur.prepared.track_id == fmt.track_id {
                cur.position = position;
            }
        }
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PositionUpdate {
                position_ms: fmt.timeline.track_time(position).as_millis(),
                duration_ms: fmt.timeline.duration_ms(),
                track_id: fmt.track_id.clone(),
                progress: fmt.timeline.progress(position),
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
            AudioEvent::Position((fmt, position)) => {
                self.handle_position_event(fmt, position).await
            }
            AudioEvent::Completion((fmt, error_count, samples_decoded)) => {
                self.handle_completion_event(fmt, error_count, samples_decoded)
                    .await;
            }
            AudioEvent::TrackCrossing(crossing) => self.handle_track_crossed(crossing).await,
            AudioEvent::Starved {
                fmt,
                starved_ms,
                position,
                producer_finished,
                samples_decoded,
                ..
            } => {
                // `producer_finished` means a drained track awaiting AutoAdvance,
                // not the stall this watchdog targets.
                if !producer_finished {
                    self.handle_starvation(&fmt, position, samples_decoded, starved_ms)
                        .await;
                }
            }
            AudioEvent::StarvationEnded { .. } => self.reset_starvation_episode(),
            AudioEvent::OutputFailed { detail } => self.output_stopped_sending(detail).await,
            AudioEvent::SourceLockMissed { .. } | AudioEvent::SourceLockReacquired { .. } => {}
        }
    }

    pub(super) async fn drain_current_audio_events(&mut self) {
        while let Some(event) = self.output.as_mut().and_then(|o| o.audio_events.pop()) {
            self.handle_audio_event(event).await;
        }

        // Asked of whichever stream the events above left in place: one they
        // replaced took its device's reports with it.
        if let Some(detail) = self
            .output
            .as_ref()
            .and_then(|out| out.stream.take_invalidation())
        {
            let out = self
                .output
                .take()
                .expect("the output whose stream reported it can't go on");
            self.follow_output_device(out, &detail).await;
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
    /// `restore_playback` controls whether startup resumes the saved track and
    /// position over the queue the tables hold; opening the library settled
    /// the tables for it first (`settle_playback_queue`). The crash-safe row
    /// stays current either way: track loads, stops, and queue edits write it
    /// immediately, and active playback writes it at most once a second.
    /// `clock` is the service's time source for side-pause countdowns;
    /// production passes one over the library's wall clock. `audio_focus` is
    /// how the platform shares the speaker, asked before every start.
    pub(crate) fn start(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        clock: crate::playback::PlaybackClockRef,
        audio_focus: crate::playback::AudioFocus,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            None,
            clock,
            audio_focus,
        )
    }

    /// Start over a caller-supplied audio device, for tests that capture samples
    /// — both the main player's output and any preview's come from it, so a test
    /// touches no audio hardware.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn start_with_audio_device(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_device: Box<dyn AudioOutputDevice>,
        clock: crate::playback::PlaybackClockRef,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            Some(audio_device),
            clock,
            // These tests play on the desktop, where nothing shares the speaker.
            crate::playback::AudioFocus::Unshared,
        )
    }

    pub(super) fn start_inner(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        custom_device: Option<Box<dyn AudioOutputDevice>>,
        clock: crate::playback::PlaybackClockRef,
        audio_focus: crate::playback::AudioFocus,
    ) -> PlaybackHandle {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let progress_handle =
            PlaybackProgressHandle::new(progress_rx, runtime_handle.clone(), clock.clone());
        let playback_queue = PlaybackQueue::new(library_manager.playback_queue_tables());
        // Taken before the service runs, so no deletion lands unheard between.
        let track_deletions = library_manager.subscribe_track_deletions();
        let progress_handle_for_completion = progress_handle.clone();
        let worker =
            WorkerThread::spawn("playback service thread", move |command_tx, command_rx| {
                let command_tx_for_completion = command_tx.clone();
                runtime_handle.spawn(async move {
                    let mut progress_rx = progress_handle_for_completion.subscribe_all();
                    while let Some(progress) = progress_rx.recv().await {
                        match progress {
                            PlaybackProgress::TrackCompleted { track_id } => {
                                info!(
                                    "Auto-advance: Track completed, sending AutoAdvance \
                                     command: {track_id}"
                                );
                                dispatch_command(
                                    &command_tx_for_completion,
                                    PlaybackCommand::AutoAdvance { track_id },
                                );
                            }
                            // A mid-flight read failure fails the buffer and the decoder
                            // exits without a TrackCompleted, so without this the UI would
                            // sit in Playing forever.
                            PlaybackProgress::PlaybackError { .. } => {
                                dispatch_command(
                                    &command_tx_for_completion,
                                    PlaybackCommand::HaltOnError,
                                );
                            }
                            _ => {}
                        }
                    }
                });
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("Failed to create runtime");
                    rt.block_on(async move {
                        let Some((audio_device, audio_output)) =
                            open_audio_device_and_output(custom_device)
                        else {
                            return;
                        };
                        let preview = PreviewPlayer::new(
                            progress_tx.clone(),
                            command_tx.clone(),
                            position_update_interval_ms,
                        );
                        let staged_sides = advance::staged_sides_follow(&library_manager);
                        let mut service = PlaybackService {
                            library_manager,
                            command_tx: command_tx.clone(),
                            command_rx,
                            progress_tx,
                            playback_queue,
                            audio_focus,
                            audio_device,
                            audio_output,
                            output: None,
                            slot: PlaybackSlot::Stopped,
                            listening: None,
                            buffered_listening: Vec::new(),
                            audible_completion: None,
                            load_generation_counter: 0,
                            preloaded_next: None,
                            volume: OutputVolume::new(),
                            preview,
                            position_update_interval_ms,
                            file_buffers: FileBuffers::new(),
                            starvation_episode: None,
                            last_position_persist: None,
                            first_audio_pending: None,
                            renderer: Renderer::Local,
                            clock,
                            staged_sides,
                        };
                        // "Restore on launch" off starts with nothing in playback;
                        // opening the library emptied the queue for it.
                        if restore_playback {
                            service.restore_from_cache().await;
                        } else {
                            debug!("restore on launch is off; starting with nothing in playback");
                        }
                        service.run(track_deletions).await;
                    });
                })
            });
        PlaybackHandle::new(worker, progress_handle)
    }

    pub(super) async fn run(
        &mut self,
        mut track_deletions: tokio_mpsc::UnboundedReceiver<Vec<String>>,
    ) {
        info!("PlaybackService started");
        let mut audio_event_tick = tokio::time::interval(std::time::Duration::from_millis(10));
        audio_event_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut countdown = side_countdown::SideCountdownWait::new();
        loop {
            countdown.follow(self.side_pause_countdown_deadline(), self.clock.as_ref());
            self.follow_staged_sides();
            tokio::select! {
                (_, sides) = self.staged_sides.next() => {
                    self.handle_staged_crossing_sides(sides).await;
                }
                _ = audio_event_tick.tick(), if self.output.is_some() || !self.buffered_listening.is_empty() || self.audible_completion.is_some() => {
                    self.drain_current_audio_events().await;
                    self.checkpoint_listening().await;
                    self.publish_audible_completion();
                }
                deadline = countdown.elapsed(), if countdown.is_armed() => {
                    self.side_pause_countdown_elapsed(deadline).await;
                }
                Some(command) = self.command_rx.recv() => {
            // Apply a boundary the callback already crossed before a command can
            // replace its source and discard the finishing track's last samples.
            self.drain_current_audio_events().await;
            // A deletion sent before this command applies before it, so the
            // command reads and edits the queue the library left. The
            // deletions arrive on a channel of their own, which the select
            // below may poll after this one.
            while let Ok(track_ids) = track_deletions.try_recv() {
                self.handle_tracks_deleted(track_ids).await;
            }
            // One event per user-intent command, at the point the loop picks it
            // up. Internal/system commands, queries, and continuous inputs map to
            // `None` and ship nothing.
            if let Some(kind) = playback_command_kind(&command) {
                self.record_telemetry(TelemetryEvent::PlaybackCommand { command: kind });
            }
            if cancels_side_pause_countdown(&command) {
                self.cancel_side_pause_countdown();
            }
            match command {
                PlaybackCommand::Play(track_id) => {
                    self.handle_play(track_id).await;
                }
                PlaybackCommand::PlayRelease { release_id, start_track_id, shuffle } => {
                    // A shuffled play starts wherever the shuffle puts first.
                    let start = match (shuffle, start_track_id) {
                        (true, _) => ReleaseStart::Shuffled,
                        (false, None) => ReleaseStart::First,
                        (false, Some(track_id)) => ReleaseStart::Track(track_id),
                    };
                    let fill = ContextFill::Release { release_id: release_id.clone(), start };
                    match self.playback_queue.play(fill).await {
                        Ok(ContextFilled::Playing { current, track_count }) => {
                            self.start_filled_context(
                                current.track_id,
                                PlaybackStartSource::Release,
                                track_count,
                            )
                            .await;
                        }
                        Ok(ContextFilled::NoTracks) => {
                            warn!("PlayRelease: release {release_id} has no tracks");
                        }
                        // A named track the release no longer has — an edit removed
                        // it since the caller read it — plays nothing rather than
                        // some other track.
                        Ok(ContextFilled::StartMissing) => {
                            error!("PlayRelease: the track to start from is not on release {release_id}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                        }
                        Err(e) => {
                            error!("Failed to play release {release_id}: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                        }
                    }
                }
                PlaybackCommand::PlayReleases(release_ids) => {
                    // The source names exactly the releases that have tracks.
                    match self.playback_queue.play(ContextFill::Releases(release_ids)).await {
                        Ok(ContextFilled::Playing { current, track_count }) => {
                            self.start_filled_context(
                                current.track_id,
                                PlaybackStartSource::Releases,
                                track_count,
                            )
                            .await;
                        }
                        Ok(ContextFilled::NoTracks | ContextFilled::StartMissing) => {
                            warn!("PlayReleases: no playable releases; nothing to play");
                        }
                        Err(e) => {
                            error!("PlayReleases: couldn't fill the queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                        }
                    }
                }
                PlaybackCommand::PlayLibraryShuffled => {
                    match self.playback_queue.play(ContextFill::LibraryShuffled).await {
                        Ok(ContextFilled::Playing { current, track_count }) => {
                            self.start_filled_context(
                                current.track_id,
                                PlaybackStartSource::LibraryShuffled,
                                track_count,
                            )
                            .await;
                        }
                        Ok(ContextFilled::NoTracks | ContextFilled::StartMissing) => {
                            warn!("PlayLibraryShuffled: the library has no tracks; nothing to play");
                        }
                        Err(e) => {
                            error!("PlayLibraryShuffled: couldn't fill the queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                        }
                    }
                }
                PlaybackCommand::Pause => {
                    self.pause();
                    self.capture_listening_position();
                    self.persist_listening().await;
                }
                PlaybackCommand::Resume => {
                    self.resume().await;
                }
                PlaybackCommand::CancelSidePauseCountdown => {
                    self.cancel_side_pause_countdown();
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
                    self.resolve_track_ready(track_id, generation).await;
                }
                PlaybackCommand::HaltOnError => {
                    self.halt_on_error().await;
                }
                PlaybackCommand::ReadFailed { buffer_id, error } => {
                    self.handle_read_failed(buffer_id, error).await;
                }
                PlaybackCommand::Previous => {
                    self.handle_previous().await;
                }
                PlaybackCommand::Seek(time) => self.seek_to_track_time(time).await,
                PlaybackCommand::SeekByRatio(ratio) => self.seek_by_ratio(ratio).await,
                PlaybackCommand::SetVolume(volume) => {
                    self.set_volume(volume);
                }
                PlaybackCommand::SetMuted(muted) => {
                    // Already in that state means nothing changes and nothing
                    // emits. Otherwise the new audible level comes back — zero on
                    // mute, the level from before the mute on unmute. Receiver
                    // mute is flaky across devices, so the remote device follows
                    // that level rather than a mute command of its own.
                    if let Some(level) = self.volume.set_muted(muted, self.audio_output.as_ref()) {
                        self.renderer.set_remote_volume(level);
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: level },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: muted },
                        );
                    }
                }
                PlaybackCommand::AddToQueue(track_ids) => {
                    let added = self.playback_queue.add_to_queue(track_ids).await;
                    self.queue_added(added).await;
                }
                PlaybackCommand::AddNext(track_ids) => {
                    let added = self.playback_queue.add_next(track_ids).await;
                    self.queue_added(added).await;
                }
                PlaybackCommand::AddReleaseToQueue(release_id) => {
                    match self.library_manager.get_track_ids(&release_id).await {
                        Ok(track_ids) if !track_ids.is_empty() => {
                            let added = self.playback_queue.add_to_queue(track_ids).await;
                            self.queue_added(added).await;
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
                            let added = self.playback_queue.add_next(track_ids).await;
                            self.queue_added(added).await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to add release {release_id} next in queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
                        }
                    }
                }
                PlaybackCommand::InsertInQueue(track_ids, index) => {
                    let added = self.playback_queue.insert_at(index, track_ids).await;
                    self.queue_added(added).await;
                }
                PlaybackCommand::RemoveFromQueue(entry_id) => {
                    match self.playback_queue.remove(entry_id).await {
                        Ok(Some(removed)) => {
                            if self
                                .current_track_id()
                                .map(|id| id == removed.track_id)
                                .unwrap_or(false)
                            {
                                self.stop().await;
                            } else {
                                self.on_queue_mutated().await;
                            }
                        }
                        // The id named no queued entry — a stale UI action against a
                        // queue that already moved on.
                        Ok(None) => self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown),
                        Err(e) => self.queue_failed("remove a queue entry", e),
                    }
                }
                PlaybackCommand::ReorderQueue { entry_id, before } => {
                    match self.playback_queue.reorder(entry_id, before).await {
                        Ok(true) => self.on_queue_mutated().await,
                        Ok(false) => self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown),
                        Err(e) => self.queue_failed("reorder the queue", e),
                    }
                }
                PlaybackCommand::ClearUpNext => {
                    match self.playback_queue.clear_up_next().await {
                        Ok(()) => self.on_queue_mutated().await,
                        Err(e) => self.queue_failed("clear Up Next", e),
                    }
                }
                PlaybackCommand::ClearPlayingFrom => {
                    match self.playback_queue.clear_playing_from().await {
                        Ok(()) => self.on_queue_mutated().await,
                        Err(e) => self.queue_failed("clear the context lane", e),
                    }
                }
                PlaybackCommand::SetRepeatMode(mode) => {
                    self.apply_repeat_mode(mode).await;
                }
                PlaybackCommand::SetShuffle(on) => {
                    // Shuffling changes what plays next, so the preload and the
                    // staged side-pause reconcile like any queue mutation.
                    match self.playback_queue.set_shuffle(on).await {
                        Ok(()) => self.on_queue_mutated().await,
                        Err(e) => self.queue_failed("shuffle the queue", e),
                    }
                }
                PlaybackCommand::ReevaluateSidePauseStaging => {
                    self.reevaluate_side_pause_staging().await;
                }
                PlaybackCommand::SkipTo(entry_id) => {
                    match self.playback_queue.skip_to(entry_id).await {
                        Ok(Some(entry)) => {
                            info!(
                                "SkipTo: jumping to queue entry {}, track {}",
                                entry.id, entry.track_id
                            );
                            self.play_track(
                                &entry.track_id,
                                TrackStart::Direct,
                                PlayTarget::Playing,
                                TrackTransition::Manual,
                            )
                            .await;
                        }
                        // The id named no queued entry — a stale UI action.
                        Ok(None) => self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown),
                        Err(e) => self.queue_failed("skip to a queue entry", e),
                    }
                }
                PlaybackCommand::PreviewPlay(target) => {
                    self.preview_play(target).await;
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
                    if reply.send(self.audio_output.get_volume()).is_err() {
                        debug!("volume asked for by a caller that stopped waiting");
                    }
                }
                #[cfg(any(test, feature = "test-utils"))]
                PlaybackCommand::GetQueue(reply) => {
                    let value = match self.library_manager.queue_value().await {
                        Ok(value) => self
                            .library_manager
                            .queue_upcoming()
                            .await
                            .map(|upcoming| (value, upcoming)),
                        Err(e) => Err(e),
                    }
                    .map_err(|e| e.to_string());
                    if reply.send(value).is_err() {
                        debug!("queue asked for by a caller that stopped waiting");
                    }
                }
                PlaybackCommand::Shutdown(reply) => {
                    if matches!(&self.slot, PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Playing)) {
                        self.pause();
                    }
                    self.capture_listening_position();
                    self.finish_listening().await;
                    self.persist_playback_state().await;
                    // Closing the library ends its playback: give the speaker
                    // back so another app can resume.
                    self.audio_focus.release();
                    if reply.send(()).is_err() {
                        debug!("shutdown asked for by a caller that stopped waiting");
                    }
                    break;
                }
                PlaybackCommand::SaveState(reply) => {
                    self.persist_playback_state().await;
                    if reply.send(()).is_err() {
                        debug!("state save asked for by a caller that stopped waiting");
                    }
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
                PlaybackCommand::RemoteEvent(event) => {
                    self.handle_remote_event(event).await;
                }
            }
                }
                Some(track_ids) = track_deletions.recv() => {
                    self.handle_tracks_deleted(track_ids).await;
                }
                else => break,
            }
        }
        info!("PlaybackService stopped");
    }
}
