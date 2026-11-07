//! The service loop's handling of one command.

use super::*;

use std::ops::ControlFlow;

impl PlaybackService {
    /// Act on `command`. `Break` once a shutdown has been served.
    pub(super) async fn handle_command(&mut self, command: PlaybackCommand) -> ControlFlow<()> {
        // Apply a boundary the callback already crossed before a command can
        // replace its source and discard the finishing track's last samples.
        self.drain_current_audio_events().await;
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
            PlaybackCommand::PlayRelease { release_id, start_track_id, shuffle, reply } => {
                // A shuffled play starts wherever the shuffle puts first.
                let start = match (shuffle, start_track_id) {
                    (true, _) => ReleaseStart::Shuffled,
                    (false, None) => ReleaseStart::First,
                    (false, Some(track_id)) => ReleaseStart::Track(track_id),
                };
                let fill = ContextFill::Release { release_id, start };
                let played =
                    self.play_filled("PlayRelease", fill, PlaybackStartSource::Release).await;
                answer_play(reply, played);
            }
            PlaybackCommand::PlayReleases { release_ids, reply } => {
                // The source names exactly the releases that have tracks.
                let fill = ContextFill::Releases(release_ids);
                let played =
                    self.play_filled("PlayReleases", fill, PlaybackStartSource::Releases).await;
                answer_play(reply, played);
            }
            PlaybackCommand::PlayTracks { track_ids, start_index, reply } => {
                let fill = ContextFill::Tracks { track_ids, start: start_index };
                let played =
                    self.play_filled("PlayTracks", fill, PlaybackStartSource::Tracks).await;
                answer_play(reply, played);
            }
            PlaybackCommand::PlayLibraryShuffled(reply) => {
                let played = self
                    .play_filled(
                        "PlayLibraryShuffled",
                        ContextFill::LibraryShuffled,
                        PlaybackStartSource::LibraryShuffled,
                    )
                    .await;
                answer_play(reply, played);
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
            PlaybackCommand::TrackReady { track_id, generation } => {
                self.resolve_track_ready(track_id, generation).await;
            }
            PlaybackCommand::HaltOnError => {
                self.halt_on_error().await;
            }
            PlaybackCommand::DecodeFailed { message } => {
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: crate::PlaybackErrorReason::internal(format!(
                            "Playback decode failed: {message}"
                        )),
                    },
                );
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
                self.set_muted(muted);
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
                match self.records.get_track_ids_for_release(&release_id).await {
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
                match self.records.get_track_ids_for_release(&release_id).await {
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
            PlaybackCommand::RemoveFromQueue { entry_id, reply } => {
                let removed = match self.playback_queue.remove(entry_id).await {
                    Ok(Some(removed)) => {
                        if self.current_track_id().map(|id| id == removed.track_id).unwrap_or(false)
                        {
                            self.stop().await;
                        } else {
                            self.on_queue_mutated().await;
                        }
                        Ok(())
                    }
                    // The id named no queued entry — a stale UI action against a
                    // queue that already moved on.
                    Ok(None) => {
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                        Err(super::api::QueueEditError::QueueChanged)
                    }
                    Err(e) => {
                        self.queue_failed("remove a queue entry", &e);
                        Err(super::api::QueueEditError::Queue(e))
                    }
                };
                if reply.send(removed).is_err() {
                    debug!("a remove answered a caller that stopped waiting");
                }
            }
            PlaybackCommand::ReorderQueue { entry_id, before, reply } => {
                let moved = match self.playback_queue.reorder(entry_id, before).await {
                    Ok(Some(version)) => {
                        self.on_queue_mutated().await;
                        Ok(version)
                    }
                    Ok(None) => {
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                        Err(super::api::QueueEditError::QueueChanged)
                    }
                    Err(e) => {
                        self.queue_failed("reorder the queue", &e);
                        Err(super::api::QueueEditError::Queue(e))
                    }
                };
                if reply.send(moved).is_err() {
                    debug!("a reorder answered a caller that stopped waiting");
                }
            }
            PlaybackCommand::ClearUpNext(reply) => {
                let cleared = match self.playback_queue.clear_up_next().await {
                    Ok(()) => {
                        self.on_queue_mutated().await;
                        Ok(())
                    }
                    Err(e) => {
                        self.queue_failed("clear Up Next", &e);
                        Err(super::api::QueueEditError::Queue(e))
                    }
                };
                if reply.send(cleared).is_err() {
                    debug!("a clear of Up Next answered a caller that stopped waiting");
                }
            }
            PlaybackCommand::ClearPlayingFrom(reply) => {
                let cleared = match self.playback_queue.clear_playing_from().await {
                    Ok(()) => {
                        self.on_queue_mutated().await;
                        Ok(())
                    }
                    Err(e) => {
                        self.queue_failed("clear the context lane", &e);
                        Err(super::api::QueueEditError::Queue(e))
                    }
                };
                if reply.send(cleared).is_err() {
                    debug!("a clear of the context lane answered a caller that stopped waiting");
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
                    Err(e) => self.queue_failed("shuffle the queue", &e),
                }
            }
            PlaybackCommand::ReevaluateSidePauseStaging => {
                self.reevaluate_side_pause_staging().await;
            }
            PlaybackCommand::SkipTo { entry_id, reply } => {
                let skipped = match self.playback_queue.skip_to(entry_id).await {
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
                        Ok(())
                    }
                    // The id named no queued entry — a stale UI action.
                    Ok(None) => {
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                        Err(super::api::QueueEditError::QueueChanged)
                    }
                    Err(e) => {
                        self.queue_failed("skip to a queue entry", &e);
                        Err(super::api::QueueEditError::Queue(e))
                    }
                };
                if reply.send(skipped).is_err() {
                    debug!("a skip answered a caller that stopped waiting");
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
                self.preview.seek_by_ratio(ratio, self.audio_device.as_ref()).await;
            }
            PlaybackCommand::PreviewCompleted => {
                self.preview_completed();
            }
            #[cfg(any(test, feature = "test-utils"))]
            PlaybackCommand::GetQueue(reply) => {
                let value = match crate::tables::queue_value(&self.store).await {
                    Ok(value) => crate::tables::queue_upcoming(&self.store)
                        .await
                        .map(|upcoming| (value, upcoming)),
                    Err(e) => Err(e),
                }
                .map_err(|e| e.to_string());
                if reply.send(value).is_err() {
                    debug!("queue asked for by a caller that stopped waiting");
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            PlaybackCommand::TakeFills(reply) => {
                if reply.send(self.file_buffers.take_fills()).is_err() {
                    debug!("fills asked for by a caller that stopped waiting");
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            PlaybackCommand::Handled(reply) => {
                if reply.send(()).is_err() {
                    debug!("handled commands asked for by a caller that stopped waiting");
                }
            }
            PlaybackCommand::Shutdown(reply) => {
                if matches!(&self.slot, PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Playing))
                {
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
                return ControlFlow::Break(());
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
            PlaybackCommand::OutputInvalidated => {
                self.follow_output_devices().await;
            }
            PlaybackCommand::AirPlayStreamFailed => {
                self.airplay_stream_failed().await;
            }
            PlaybackCommand::RemoteEvent(event) => {
                self.handle_remote_event(event).await;
            }
        }
        ControlFlow::Continue(())
    }
}

impl PlaybackService {
    /// Fill the queue with what `fill` names and start its first track, saying
    /// what the play did. A play that names nothing to play, or a start that is
    /// gone (an edit removed the track since the caller read it), leaves the
    /// queue as it was and plays nothing rather than some other track.
    async fn play_filled(
        &mut self,
        command: &str,
        fill: ContextFill,
        source: PlaybackStartSource,
    ) -> Result<PlayOutcome, PlayError> {
        match self.playback_queue.play(fill).await {
            Ok(ContextFilled::Playing { current, track_count, left_out }) => {
                self.start_filled_context(current.track_id, source, track_count).await;
                Ok(match left_out {
                    LeftOut::Nothing => PlayOutcome::Playing,
                    LeftOut::Releases(ids) => {
                        warn!("{command}: left out releases without tracks: {ids:?}");
                        PlayOutcome::PlayingWithoutReleases(ids)
                    }
                    LeftOut::Tracks(ids) => {
                        warn!("{command}: left out tracks the library lacks: {ids:?}");
                        PlayOutcome::PlayingWithoutTracks(ids)
                    }
                })
            }
            Ok(ContextFilled::NoTracks) => {
                warn!("{command}: nothing to play");
                Ok(PlayOutcome::NothingToPlay)
            }
            Ok(ContextFilled::StartMissing) => {
                warn!("{command}: the track to start from is gone");
                Ok(PlayOutcome::StartMissing)
            }
            Err(e) => {
                error!("{command}: couldn't fill the queue: {e}");
                self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                Err(PlayError::Queue(e))
            }
        }
    }
}

/// Tell a play's caller what it did. A caller that stopped waiting has no one
/// to tell.
fn answer_play(reply: PlayReply, played: Result<PlayOutcome, PlayError>) {
    if reply.send(played).is_err() {
        debug!("a play answered a caller that stopped waiting");
    }
}
