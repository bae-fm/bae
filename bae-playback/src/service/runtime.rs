use super::*;

use bae_util::task_failure::{caught, caught_sync, Bug};
use bae_util::worker_thread::WorkerThread;
use std::ops::ControlFlow;

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
        let PreloadedNext { prepared, source, cancel_token, .. } = preloaded;
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
            current.decoder.cancel_token.store(true, std::sync::atomic::Ordering::Release);
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

        let spawned = spawn_decoder(
            &self.diagnostics,
            DecoderSetup::new(
                decode,
                sample_rate,
                channels,
                "Streaming decode",
                DecodeFailureReport::EmitPlaybackError { progress_tx: self.progress_tx.clone() },
            ),
            |track_stream, handle, cancel_token, ready| (track_stream, handle, cancel_token, ready),
        );
        let (track_stream, handle, cancel_token, ready) = match spawned {
            Ok(parts) => parts,
            Err(error) => {
                if let Some((decoder, buffers)) = outgoing_decoder {
                    // The outgoing track is still the source's current one, and
                    // the callback is silenced, so its decoder may be parked on
                    // a full ring that only the sink's cancel frees. The failed
                    // load stops playback, so the staged next goes with it.
                    if let Some(output) = &self.output {
                        output.source.lock().unwrap().cancel();
                    }
                    cancel_and_join_decoder(&decoder.cancel_token, &buffers, decoder.handle).await;
                }
                return Err(error);
            }
        };

        if let Err(error) =
            self.attach_track(track_stream, fmt, sample_rate, channels, staged_next).await
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
        let progress_tx = self.progress_tx.clone();
        self.diagnostics.spawn_reported(
            &tokio::runtime::Handle::current(),
            "wait for the track's first samples",
            async move {
                match ready.await {
                    Ok(()) => dispatch_command(
                        &command_tx,
                        PlaybackCommand::TrackReady { track_id, generation },
                    ),
                    Err(_) => {
                        debug!("ready watcher dropped before signal for track {track_id}")
                    }
                }
            },
            // The load would wait for its TrackReady forever: stop it with
            // an error instead.
            move |failed| {
                emit_progress(
                    &progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: crate::PlaybackErrorReason::internal(failed),
                    },
                )
            },
        );

        if let Some((decoder, buffers)) = outgoing_decoder {
            cancel_and_join_decoder(&decoder.cancel_token, &buffers, decoder.handle).await;
        }

        self.install_active_track(
            prepared,
            TrackDecoder { handle, cancel_token },
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
    pub(super) async fn handle_read_failed(
        &mut self,
        buffer_id: u64,
        error: Arc<bae_track_audio::TrackReadError>,
    ) {
        if let PlaybackSlot::Active(cur) = &self.slot {
            if cur.prepared.reads_buffer(buffer_id) {
                error!(
                    track_id = %cur.prepared.track_id,
                    "read failed for the playing track; halting playback"
                );
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError { reason: crate::read_failure_reason(&error) },
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
        self.diagnostics.event(event);
    }

    /// Restore playback from the device-local resume row at startup: a stored
    /// row resumes its track over the queue the tables hold; an absent row
    /// resumes nothing, and the library opened with the queue emptied
    /// (`settle_playback_queue`).
    pub(super) async fn restore_from_cache(&mut self) {
        match crate::tables::load_playback_state(&self.store).await {
            Ok(Some(state)) => self.restore(state).await,
            Ok(None) => {}
            Err(e) => warn!("couldn't load the saved playback state: {e}; resuming nothing"),
        }
    }

    /// A queue table write or read failed: the queue is as it was before the
    /// command, and the command does nothing more.
    pub(super) fn queue_failed(&self, doing: &str, error: &bae_store::DbError) {
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
        let failure =
            match self.playback_queue.play(ContextFill::ReleaseOfTrack(track_id.clone())).await {
                Ok(ContextFilled::Playing { track_count, .. }) => {
                    self.telemetry_playback_started(PlaybackStartSource::Release, track_count);
                    None
                }
                Ok(ContextFilled::NoTracks | ContextFilled::StartMissing) => {
                    Some(PlaybackError::not_found("Track", track_id.clone()))
                }
                Err(e) => Some(PlaybackError::from(e)),
            };
        if let Some(e) = failure {
            error!("Failed to play {track_id} from its release: {e}");
            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError { reason: e.ui_reason() },
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
        let target =
            if self.is_side_paused() { PlayTarget::Playing } else { self.current_play_target() };
        if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
            self.advance_and_play_preloaded(&preloaded_track_id, false, target).await;
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
        let (Some(current_track_id), Some(track_time)) =
            (self.current_track_id().map(str::to_string), self.current_track_time())
        else {
            debug!("Previous command received with no current track; ignoring");
            return;
        };
        let target = self.current_play_target();

        let action = match self.playback_queue.previous_action(track_time).await {
            Ok(action) => action,
            Err(e) => {
                self.queue_failed("step the queue back", &e);
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
            PlaybackProgress::position_update(fmt.timeline, fmt.track_id.clone(), position),
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
                self.handle_completion_event(fmt, error_count, samples_decoded).await;
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
                    self.handle_starvation(&fmt, position, samples_decoded, starved_ms).await;
                }
            }
            AudioEvent::StarvationEnded { .. } => self.reset_starvation_episode(),
            AudioEvent::OutputFailed { detail } => self.output_stopped_sending(detail).await,
            AudioEvent::SourceLockMissed { .. } | AudioEvent::SourceLockReacquired { .. } => {}
        }
    }

    /// A stream to the AirPlay receiver reported it failed for good: a
    /// receiver that stopped answering, or a sender thread that panicked and
    /// sends no more. End AirPlay and resume local paused (the shape a
    /// Cast/DLNA end has). The live session's failure is the one that counts:
    /// a report from a session since replaced finds none.
    pub(super) async fn airplay_stream_failed(&mut self) {
        if let Some(cause) = self.renderer.airplay_failure() {
            warn!("airplay stream failed ({cause}); ending AirPlay and resuming local");
            self.end_airplay_and_resume_local(RemoteEnd::Lost(cause)).await;
        }
    }

    pub(super) async fn drain_current_audio_events(&mut self) {
        while let Some(event) = self.output.as_mut().and_then(|o| o.audio_events.pop()) {
            self.handle_audio_event(event).await;
        }

        // Asked of whichever stream the events above left in place: one they
        // replaced took its device's reports with it.
        if let Some(detail) = self.output.as_ref().and_then(|out| out.stream.take_invalidation()) {
            let out = self.output.take().expect("the output whose stream reported it can't go on");
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
                    reason: crate::PlaybackErrorReason::internal(
                        "Playback event queue dropped a required audio event".to_string(),
                    ),
                },
            );
            self.stop().await;
        }
    }
    /// Playback reads the library's tracks and preferences and streams their
    /// files from `store`, and its tracks' audio from `records`, and writes
    /// its resume row there and its listening history through `people`, each play under an id `ids` mints.
    /// `restore_playback` controls whether startup resumes the saved track and
    /// position over the queue the tables hold; opening the library settled
    /// the tables for it first (`settle_playback_queue`). The crash-safe row
    /// stays current either way: track loads, stops, and queue edits write it
    /// immediately, and active playback writes it at most once a second.
    /// `clock` is the service's time source for side-pause countdowns;
    /// production passes one over the library's wall clock. `audio_focus` is
    /// how the platform shares the speaker, asked before every start. Playback
    /// changes the queue through `queue_tables`, writes what it is doing
    /// and where it is into `now_row` and `timeline_row`, and lets go of a
    /// track it holds once the library no longer holds it.
    pub fn start(
        store: bae_store::Store,
        records: bae_records::Records,
        people: bae_people::People,
        ids: coven::IdSourceRef,
        queue_tables: crate::tables::PlaybackQueueTables,
        now_row: crate::tables::PlaybackNowRow,
        timeline_row: crate::tables::PlaybackTimelineRow,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        clock: crate::PlaybackClockRef,
        audio_focus: crate::AudioFocus,
    ) -> PlaybackHandle {
        Self::start_inner(
            store,
            records,
            people,
            ids,
            queue_tables,
            now_row,
            timeline_row,
            diagnostics,
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
    pub fn start_with_audio_device(
        store: bae_store::Store,
        records: bae_records::Records,
        people: bae_people::People,
        ids: coven::IdSourceRef,
        queue_tables: crate::tables::PlaybackQueueTables,
        now_row: crate::tables::PlaybackNowRow,
        timeline_row: crate::tables::PlaybackTimelineRow,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_device: Box<dyn AudioOutputDevice>,
        clock: crate::PlaybackClockRef,
    ) -> PlaybackHandle {
        Self::start_inner(
            store,
            records,
            people,
            ids,
            queue_tables,
            now_row,
            timeline_row,
            diagnostics,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            Some(audio_device),
            clock,
            // These tests play on the desktop, where nothing shares the speaker.
            crate::AudioFocus::Unshared,
        )
    }

    pub(super) fn start_inner(
        store: bae_store::Store,
        records: bae_records::Records,
        people: bae_people::People,
        ids: coven::IdSourceRef,
        queue_tables: crate::tables::PlaybackQueueTables,
        now_row: crate::tables::PlaybackNowRow,
        timeline_row: crate::tables::PlaybackTimelineRow,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        custom_device: Option<Box<dyn AudioOutputDevice>>,
        clock: crate::PlaybackClockRef,
        audio_focus: crate::AudioFocus,
    ) -> PlaybackHandle {
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let arbiter = FetchArbiter::new();
        let (playing_bytes, playing_bytes_rx) = tokio::sync::watch::channel(None);
        let progress_handle = PlaybackProgressHandle::new(
            progress_rx,
            runtime_handle.clone(),
            clock.clone(),
            now_row,
            timeline_row,
            diagnostics.clone(),
        );
        let playback_queue = PlaybackQueue::new(queue_tables);
        let progress_handle_for_completion = progress_handle.clone();
        let service_arbiter = arbiter.clone();
        let worker =
            WorkerThread::spawn("playback service thread", move |command_tx, command_rx| {
                let command_tx_for_completion = command_tx.clone();
                let follower_diagnostics = diagnostics.clone();
                runtime_handle.spawn(async move {
                    let mut progress_rx = progress_handle_for_completion.subscribe_all();
                    while let Some(progress) = progress_rx.recv().await {
                        // One event at a time: a panic turning one into a
                        // command is reported, and the next is served.
                        let followed = caught_sync("follow a playback event", || {
                            follow_progress(&command_tx_for_completion, progress)
                        });
                        if let Err(bug) = followed {
                            follower_diagnostics.panicked(bug);
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
                            diagnostics.clone(),
                        );
                        let staged_sides = advance::track_infos_follow(
                            &store,
                            "the staged crossing's sides",
                            &diagnostics,
                        );
                        let held_tracks = advance::track_infos_follow(
                            &store,
                            "the tracks playback holds",
                            &diagnostics,
                        );
                        let mut service = PlaybackService {
                            store,
                            records,
                            people,
                            ids,
                            diagnostics,
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
                            file_buffers: FileBuffers::new(service_arbiter, playing_bytes),
                            starvation_episode: None,
                            last_position_persist: None,
                            first_audio_pending: None,
                            renderer: Renderer::Local,
                            clock,
                            staged_sides,
                            held_tracks,
                        };
                        // "Restore on launch" off starts with nothing in playback;
                        // opening the library emptied the queue for it.
                        if restore_playback {
                            let restored =
                                caught("restore playback", service.restore_from_cache()).await;
                            if let Err(failed) = restored {
                                service.recover_from_panic(failed).await;
                            }
                        } else {
                            debug!("restore on launch is off; starting with nothing in playback");
                        }
                        service.run().await;
                    });
                })
            });
        PlaybackHandle::new(worker, progress_handle, arbiter, playing_bytes_rx)
    }

    pub(super) async fn run(&mut self) {
        info!("PlaybackService started");
        let mut audio_event_tick = tokio::time::interval(std::time::Duration::from_millis(10));
        audio_event_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut countdown = side_countdown::SideCountdownWait::new();
        loop {
            let follows = caught_sync(
                "follow the side countdown, the staged sides and the held tracks",
                || {
                    countdown.follow(self.side_pause_countdown_deadline(), self.clock.as_ref());
                    self.follow_staged_sides();
                    self.follow_held_tracks();
                },
            );
            if let Err(failed) = follows {
                self.recover_from_panic(failed).await;
                continue;
            }
            let wake = tokio::select! {
                (_, sides) = self.staged_sides.next() => Wake::StagedSides(sides),
                _ = audio_event_tick.tick(), if self.output.is_some() || !self.buffered_listening.is_empty() || self.audible_completion.is_some() || self.preview.is_active() => Wake::AudioTick,
                deadline = countdown.elapsed(), if countdown.is_armed() => Wake::CountdownElapsed(deadline),
                Some(command) = self.command_rx.recv() => Wake::Command(command),
                (held, infos) = self.held_tracks.next() => Wake::HeldTracks { held, infos },
                else => break,
            };
            // One wake at a time under one boundary: a panic in it is reported
            // and stops playback with an error, and the loop serves the next.
            match caught("serve playback", self.serve(wake)).await {
                Ok(ControlFlow::Continue(())) => {}
                Ok(ControlFlow::Break(())) => break,
                Err(failed) => self.recover_from_panic(failed).await,
            }
        }
        info!("PlaybackService stopped");
    }

    /// Act on what woke the loop. `Break` once a shutdown has been served.
    async fn serve(&mut self, wake: Wake) -> ControlFlow<()> {
        match wake {
            Wake::StagedSides(sides) => self.handle_staged_crossing_sides(sides).await,
            Wake::AudioTick => {
                self.drain_current_audio_events().await;
                self.follow_preview_output_device().await;
                self.checkpoint_listening().await;
                self.publish_audible_completion();
            }
            Wake::CountdownElapsed(deadline) => self.side_pause_countdown_elapsed(deadline).await,
            Wake::Command(command) => return self.handle_command(command).await,
            Wake::HeldTracks { held, infos } => self.handle_held_tracks(held, infos).await,
        }
        ControlFlow::Continue(())
    }

    /// A panic stopped what the loop was serving: report it, and stop
    /// playback with an error. The pipeline it left can't be trusted, so its
    /// output goes first, before the stop reads anything from it.
    async fn recover_from_panic(&mut self, bug: Bug) {
        let failed = self.diagnostics.panicked(bug);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PlaybackError {
                reason: crate::PlaybackErrorReason::internal(&failed),
            },
        );
        self.discard_output();
        if let Err(again) = caught("stop playback after a panic", self.stop()).await {
            self.diagnostics.panicked(again);
        }
    }
}

/// Turn a progress event into the command it asks of the service loop: a
/// completed track advances the queue, and a mid-flight failure halts playback.
fn follow_progress(
    command_tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>,
    progress: PlaybackProgress,
) {
    match progress {
        PlaybackProgress::TrackCompleted { track_id } => {
            info!("Auto-advance: Track completed, sending AutoAdvance command: {track_id}");
            dispatch_command(command_tx, PlaybackCommand::AutoAdvance { track_id });
        }
        // A mid-flight read failure fails the buffer and the decoder exits
        // without a TrackCompleted, so without this the UI would sit in
        // Playing forever.
        PlaybackProgress::PlaybackError { .. } => {
            dispatch_command(command_tx, PlaybackCommand::HaltOnError);
        }
        _ => {}
    }
}

/// What woke the service loop.
enum Wake {
    StagedSides(Vec<PlaybackTrackInfo>),
    AudioTick,
    CountdownElapsed(chrono::DateTime<chrono::Utc>),
    Command(PlaybackCommand),
    /// The library's facts of the tracks playback holds, `held`, which
    /// leave out each one it no longer holds.
    HeldTracks {
        held: Vec<String>,
        infos: Vec<PlaybackTrackInfo>,
    },
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
