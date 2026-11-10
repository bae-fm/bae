//! The playback thread: one [`PlaybackService`] after another.
//!
//! The thread outlives the services it runs. One that ends on a panic, or
//! because the audio output won't open, is told once to whoever follows
//! playback, and no other starts until the handle sends a command: a device
//! that can't open is tried once per press, never in a loop.
//!
//! The handle's commands wait in the thread's one channel, so a command sent
//! while a service dies or the next one starts stays in order and reaches the
//! next one. What a service's own parts send it carries that service's number
//! ([`OwnCommands`]): a part that outlived its service, or the report of its
//! end, reaches no other. The command that starts a service comes off the
//! channel, so each start spends a command: when that service ends before
//! serving it, the command ends with it, its waiting caller fails, and the
//! end's report tells the person.
//!
//! A fresh service holds only what outlives one: the tables and the rows.

use super::*;

use bae_util::task_failure::{caught, caught_sync, Bug};
use bae_util::worker_thread::WorkerThread;
use std::ops::ControlFlow;

/// Names the thread, and the bug a panic that ends a service is reported as.
const PLAYBACK_THREAD: &str = "playback service thread";

/// What the playback thread's channel carries.
#[derive(Debug)]
pub(crate) enum PlaybackMessage {
    /// From the handle, for whichever service serves it.
    Asked(PlaybackCommand),
    /// From a service's own parts, for that service alone.
    Own { service: u64, command: PlaybackCommand },
}

/// The device a service opens its outputs from, which tells the given report
/// when a stream's device reports the stream can't go on.
pub(super) type ServiceAudioDevice =
    Box<dyn Fn(crate::audio_output::InvalidationReport) -> Arc<dyn AudioOutputDevice> + Send>;

/// How one service's parts send it commands.
#[derive(Clone)]
pub(crate) struct OwnCommands {
    service: u64,
    channel: tokio_mpsc::UnboundedSender<PlaybackMessage>,
}

impl OwnCommands {
    /// The first service's, over the thread's `channel`.
    fn first(channel: tokio_mpsc::UnboundedSender<PlaybackMessage>) -> Self {
        Self { service: 0, channel }
    }

    /// A first service's commands, over a channel of their own.
    #[cfg(test)]
    pub(crate) fn channel() -> (Self, tokio_mpsc::UnboundedReceiver<PlaybackMessage>) {
        let (channel, messages) = tokio_mpsc::unbounded_channel();
        (Self::first(channel), messages)
    }

    fn of_the_next_service(&self) -> Self {
        Self { service: self.service + 1, channel: self.channel.clone() }
    }
}

/// Send a service a command of its own, warning if the thread has shut down.
pub(crate) fn dispatch_command(commands: &OwnCommands, command: PlaybackCommand) {
    let message = PlaybackMessage::Own { service: commands.service, command };
    if let Err(unsent) = commands.channel.send(message) {
        warn!("playback command channel closed; dropped {:?}", unsent.0);
    }
}

/// The next command for the service numbered `service`, leaving behind what
/// an ended service's parts sent.
async fn next_command(
    messages: &mut tokio_mpsc::UnboundedReceiver<PlaybackMessage>,
    service: u64,
) -> Option<PlaybackCommand> {
    loop {
        match messages.recv().await? {
            PlaybackMessage::Asked(command) => return Some(command),
            PlaybackMessage::Own { service: sender, command } if sender == service => {
                return Some(command)
            }
            PlaybackMessage::Own { command, .. } => {
                debug!("dropped {command:?}: the service it was for has ended");
            }
        }
    }
}

/// With no service running, wait for the command that starts one: any the
/// handle sends but a state save, which has nothing to save, and a shutdown,
/// which ends the wait with none.
fn next_starter(
    messages: &mut tokio_mpsc::UnboundedReceiver<PlaybackMessage>,
) -> Option<PlaybackCommand> {
    loop {
        let message =
            messages.blocking_recv().expect("the thread holds a sender of its own channel");
        match message {
            PlaybackMessage::Own { command, .. } => {
                debug!("dropped {command:?}: the service it was for has ended");
            }
            PlaybackMessage::Asked(PlaybackCommand::Shutdown(reply)) => {
                if reply.send(()).is_err() {
                    debug!("shutdown asked for by a caller that stopped waiting");
                }
                return None;
            }
            PlaybackMessage::Asked(PlaybackCommand::SaveState(reply)) => {
                if reply.send(()).is_err() {
                    debug!("state save asked for by a caller that stopped waiting");
                }
            }
            PlaybackMessage::Asked(command) => return Some(command),
        }
    }
}

/// How a service starts.
enum ServiceStart {
    /// As the library opens: it resumes what the library holds when
    /// "restore on launch" says so.
    Launch { restore_playback: bool },
    /// After another service ended, taking up what that one left, to serve
    /// `starter` first. A service that ends before serving it takes it along.
    AfterAnEnd { starter: PlaybackCommand },
}

/// What the thread starts each service from, and what outlives them.
struct PlaybackThread {
    store: bae_store::Store,
    records: bae_records::Records,
    people: bae_people::People,
    ids: coven::IdSourceRef,
    queue_tables: crate::tables::PlaybackQueueTables,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    progress_handle: PlaybackProgressHandle,
    position_update_interval_ms: u32,
    clock: crate::PlaybackClockRef,
    audio_focus: crate::AudioFocus,
    audio_device: ServiceAudioDevice,
    arbiter: Arc<FetchArbiter>,
    playing_bytes: tokio::sync::watch::Sender<Option<PlayingBytes>>,
}

impl PlaybackThread {
    /// Run services over the thread's channel until a shutdown has been
    /// served, the first as the library opens.
    fn run(
        self,
        channel: tokio_mpsc::UnboundedSender<PlaybackMessage>,
        messages: &mut tokio_mpsc::UnboundedReceiver<PlaybackMessage>,
        restore_playback: bool,
    ) {
        let mut commands = OwnCommands::first(channel);
        let mut start = ServiceStart::Launch { restore_playback };
        loop {
            // Each event it follows catches its own panic; a panic outside
            // them ends the follower, and the join below goes on.
            let follower = self.diagnostics.spawn_reported(
                &self.runtime_handle,
                "follow the playback service",
                follow_the_service(
                    self.progress_handle.subscribe_all(),
                    commands.clone(),
                    self.diagnostics.clone(),
                ),
                |_| {},
            );
            let reason = match caught_sync(PLAYBACK_THREAD, || {
                self.serve(start, &commands, messages)
            }) {
                Ok(Ok(())) => return,
                Ok(Err(error)) => super::output::output_start_failed(
                    &self.diagnostics,
                    "open the audio output",
                    &PlaybackError::AudioOutput(error),
                ),
                Err(bug) => crate::PlaybackErrorReason::internal(self.diagnostics.panicked(bug)),
            };
            self.report_the_end(reason);
            // Its follower ends on hearing that end, which the fan-out wrote
            // into `playback_now` first: the next service reads the level
            // this one set, and its own follower hears nothing of this one.
            if let Err(error) = self.runtime_handle.block_on(follower.outcome()) {
                warn!("the ended service's follower ended without hearing its end: {error}");
            }
            let Some(starter) = next_starter(messages) else {
                return;
            };
            commands = commands.of_the_next_service();
            start = ServiceStart::AfterAnEnd { starter };
        }
    }

    /// Run one service until a shutdown has been served; why it can't run
    /// when the audio output won't open.
    fn serve(
        &self,
        start: ServiceStart,
        commands: &OwnCommands,
        messages: &mut tokio_mpsc::UnboundedReceiver<PlaybackMessage>,
    ) -> Result<(), crate::AudioError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime");
        rt.block_on(async {
            let audio_device = (self.audio_device)({
                let commands = commands.clone();
                crate::audio_output::InvalidationReport::new(move || {
                    dispatch_command(&commands, PlaybackCommand::OutputInvalidated)
                })
            });
            let output = audio_device.open_output()?;
            // At full volume until playback restores the level the person set.
            let audio_output =
                PlayerOutput::new(output, AudioOutputControls::new(crate::Volume::FULL));
            let mut service =
                self.service(rt.handle().clone(), commands, audio_device, audio_output);
            let (taken_up, starter) = match start {
                ServiceStart::Launch { restore_playback: true } => {
                    (caught("restore playback", service.restore_from_cache()).await, None)
                }
                // "Restore on launch" off starts with nothing in playback;
                // opening the library emptied the queue for it.
                ServiceStart::Launch { restore_playback: false } => {
                    debug!("restore on launch is off; starting with nothing in playback");
                    (Ok(()), None)
                }
                ServiceStart::AfterAnEnd { starter } => {
                    let taken_up = service.restore_after_an_end();
                    (caught("take up what the ended service left", taken_up).await, Some(starter))
                }
            };
            if let Err(failed) = taken_up {
                service.recover_from_panic(failed).await;
            }
            service.run(starter, messages).await;
            Ok(())
        })
    }

    /// A service with nothing playing, over this thread's parts.
    fn service(
        &self,
        runtime: tokio::runtime::Handle,
        commands: &OwnCommands,
        audio_device: Arc<dyn AudioOutputDevice>,
        audio_output: PlayerOutput,
    ) -> PlaybackService {
        PlaybackService {
            runtime: runtime.clone(),
            store: self.store.clone(),
            records: self.records.clone(),
            people: self.people.clone(),
            ids: self.ids.clone(),
            diagnostics: self.diagnostics.clone(),
            command_tx: commands.clone(),
            progress_tx: self.progress_tx.clone(),
            playback_queue: PlaybackQueue::new(self.queue_tables.clone()),
            audio_focus: self.audio_focus.clone(),
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
            preview: PreviewPlayer::new(
                runtime.clone(),
                self.progress_tx.clone(),
                commands.clone(),
                self.position_update_interval_ms,
                self.diagnostics.clone(),
            ),
            position_update_interval_ms: self.position_update_interval_ms,
            file_buffers: FileBuffers::new(
                runtime.clone(),
                self.arbiter.clone(),
                self.playing_bytes.clone(),
            ),
            starvation_episode: None,
            persisted_position: None,
            first_audio_pending: None,
            renderer: Renderer::Local,
            clock: self.clock.clone(),
            staged_sides: advance::track_infos_follow(
                &self.store,
                "the staged crossing's sides",
                &self.diagnostics,
            ),
            held_tracks: advance::track_infos_follow(
                &self.store,
                "the tracks playback holds",
                &self.diagnostics,
            ),
        }
    }

    /// The running service ended for `reason`: tell whoever follows playback,
    /// the one time it is told, and let go of what the service held outside
    /// itself. Playback is stopped, here, previewing nothing.
    fn report_the_end(&self, reason: crate::PlaybackErrorReason) {
        self.arbiter.set_waiting(None);
        self.playing_bytes.send_replace(None);
        self.audio_focus.release();
        for event in [
            PlaybackProgress::PlaybackError { reason },
            PlaybackProgress::StateChanged { state: PlaybackState::Stopped },
            PlaybackProgress::RemoteStatusChanged { remote: crate::RemotePlayback::Local },
            PlaybackProgress::PreviewStateChanged(crate::PreviewState::Idle),
            PlaybackProgress::ServiceEnded,
        ] {
            emit_progress(&self.progress_tx, event);
        }
    }
}

/// Turn what the service reports into the commands it asks of itself, until
/// its end is heard.
async fn follow_the_service(
    mut feed: crate::PlaybackProgressFeed,
    commands: OwnCommands,
    diagnostics: Diagnostics,
) {
    while let Some(progress) = feed.recv().await {
        if matches!(progress, PlaybackProgress::ServiceEnded) {
            return;
        }
        // One event at a time: a panic turning one into a command is
        // reported, and the next is served.
        let followed =
            caught_sync("follow a playback event", || follow_progress(&commands, progress));
        if let Err(bug) = followed {
            diagnostics.panicked(bug);
        }
    }
}

/// Turn a progress event into the command it asks of the service loop: a
/// completed track advances the queue, and a mid-flight failure halts playback.
fn follow_progress(commands: &OwnCommands, progress: PlaybackProgress) {
    match progress {
        PlaybackProgress::TrackCompleted { track_id } => {
            info!("Auto-advance: Track completed, sending AutoAdvance command: {track_id}");
            dispatch_command(commands, PlaybackCommand::AutoAdvance { track_id });
        }
        // A mid-flight read failure fails the buffer and the decoder exits
        // without a TrackCompleted, so without this the UI would sit in
        // Playing forever.
        PlaybackProgress::PlaybackError { .. } => {
            dispatch_command(commands, PlaybackCommand::HaltOnError);
        }
        _ => {}
    }
}

impl PlaybackService {
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
        audio_device: ServiceAudioDevice,
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
        let bugs = diagnostics.bug_sink();
        let thread = PlaybackThread {
            store,
            records,
            people,
            ids,
            queue_tables,
            diagnostics,
            runtime_handle,
            progress_tx,
            progress_handle: progress_handle.clone(),
            position_update_interval_ms,
            clock,
            audio_focus,
            audio_device,
            arbiter: arbiter.clone(),
            playing_bytes,
        };
        let worker = WorkerThread::spawn(PLAYBACK_THREAD, bugs, move |channel, messages| {
            thread.run(channel, messages, restore_playback)
        });
        PlaybackHandle::new(worker, progress_handle, arbiter, playing_bytes_rx)
    }

    /// Serve `starter`, then `commands`, and what else wakes the loop, until
    /// a shutdown has been served.
    pub(super) async fn run(
        &mut self,
        mut starter: Option<PlaybackCommand>,
        commands: &mut tokio_mpsc::UnboundedReceiver<PlaybackMessage>,
    ) {
        info!("PlaybackService started");
        let service = self.command_tx.service;
        let mut countdown = deadline_wait::DeadlineWait::new();
        let mut audible = deadline_wait::DeadlineWait::new();
        loop {
            let follows =
                caught_sync("follow the deadlines, the staged sides and the held tracks", || {
                    countdown.follow(self.side_pause_countdown_deadline(), self.clock.as_ref());
                    audible.follow(self.audible_deadline(), self.clock.as_ref());
                    self.follow_staged_sides();
                    self.follow_held_tracks();
                });
            if let Err(failed) = follows {
                self.recover_from_panic(failed).await;
                continue;
            }
            let wake = if let Some(command) = starter.take() {
                Wake::Command(command)
            } else {
                tokio::select! {
                    (_, sides) = self.staged_sides.next() => Wake::StagedSides(sides),
                    () = audio_events_ready(&self.output) => Wake::AudioEvents,
                    deadline = countdown.elapsed(), if countdown.is_armed() => Wake::CountdownElapsed(deadline),
                    _ = audible.elapsed(), if audible.is_armed() => Wake::AudibleDeadline,
                    Some(command) = next_command(commands, service) => Wake::Command(command),
                    (held, infos) = self.held_tracks.next() => Wake::HeldTracks { held, infos },
                    else => break,
                }
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
            Wake::AudioEvents => {
                self.drain_current_audio_events().await;
                self.checkpoint_listening().await;
            }
            Wake::AudibleDeadline => {
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
    pub(super) async fn recover_from_panic(&mut self, bug: Bug) {
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

/// Complete once the output, while there is one, pushed audio events.
async fn audio_events_ready(output: &Option<OutputStream>) {
    match output {
        Some(output) => output.audio_events.ready().await,
        None => std::future::pending().await,
    }
}

/// What woke the service loop.
enum Wake {
    StagedSides(Vec<PlaybackTrackInfo>),
    /// The output pushed audio events: drain them.
    AudioEvents,
    CountdownElapsed(chrono::DateTime<chrono::Utc>),
    /// Audio still in the AirPlay receiver has been heard, or a play in it
    /// came to count as played (`audible_deadline`).
    AudibleDeadline,
    Command(PlaybackCommand),
    /// The library's facts of the tracks playback holds, `held`, which
    /// leave out each one it no longer holds.
    HeldTracks {
        held: Vec<String>,
        infos: Vec<PlaybackTrackInfo>,
    },
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
