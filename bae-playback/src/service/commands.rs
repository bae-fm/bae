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
                        error!(
                            "PlayRelease: the track to start from is not on release {release_id}"
                        );
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
            PlaybackCommand::PlayTracks { track_ids, start_index } => {
                let named = track_ids.len();
                let fill = ContextFill::Tracks { track_ids, start: start_index };
                match self.playback_queue.play(fill).await {
                    Ok(ContextFilled::Playing { current, track_count }) => {
                        if track_count < named {
                            warn!(
                                "PlayTracks: {} of {named} tracks are not in the library; \
                                 they are left out",
                                named - track_count
                            );
                        }
                        self.start_filled_context(
                            current.track_id,
                            PlaybackStartSource::Tracks,
                            track_count,
                        )
                        .await;
                    }
                    // A start the list doesn't have, or a track the library
                    // no longer holds, plays nothing rather than some other
                    // track.
                    Ok(ContextFilled::StartMissing) => {
                        error!(
                            "PlayTracks: no track in the library to start from at {start_index} of {named}"
                        );
                        self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                    }
                    Ok(ContextFilled::NoTracks) => {
                        warn!("PlayTracks: no tracks named; nothing to play");
                    }
                    Err(e) => {
                        error!("PlayTracks: couldn't fill the queue: {e}");
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
            PlaybackCommand::TrackReady { track_id, generation } => {
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
                match self.database.get_track_ids_for_release(&release_id).await {
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
                match self.database.get_track_ids_for_release(&release_id).await {
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
                        if self.current_track_id().map(|id| id == removed.track_id).unwrap_or(false)
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
            PlaybackCommand::ClearUpNext => match self.playback_queue.clear_up_next().await {
                Ok(()) => self.on_queue_mutated().await,
                Err(e) => self.queue_failed("clear Up Next", e),
            },
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
                self.preview.seek_by_ratio(ratio, self.audio_device.as_ref()).await;
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
                let value = match crate::tables::queue_value(&self.database).await {
                    Ok(value) => crate::tables::queue_upcoming(&self.database)
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
            PlaybackCommand::RemoteEvent(event) => {
                self.handle_remote_event(event).await;
            }
        }
        ControlFlow::Continue(())
    }
}
