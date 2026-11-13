//! A live remote-renderer session: one connected device, driven on its own
//! thread.
//!
//! [`RendererSession`] owns a [`RendererLink`] and the thread that talks to
//! it. The thread runs commands (load/play/pause/seek/volume/stop) as they
//! come. A [`RendererLink::Polled`] device is also asked for its status on a
//! fixed interval between commands. A [`RendererLink::Pushed`] device says its
//! status as it changes, through the [`StatusSink`] it is given once, and says
//! when its link is lost. Every status, every refused command, and the
//! session's end with its cause, goes to a caller-supplied callback, tagged
//! with the session's id. The callback is the seam to the playback service:
//! the session never names playback types, so this stays decoupled from the
//! renderer that consumes it.

use std::ops::ControlFlow;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use bae_diagnostics::{Diagnostics, ReportedThread};

use tracing::{debug, warn};

use super::channel::{
    PolledRenderer, ReceiverStatus, RendererChannel, RendererError, RendererLink, RendererMedia,
};
use crate::PlaybackClockRef;

/// How often the session polls a polled renderer for status between commands.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Which session an event is from. A session that has been dropped may still
/// have events waiting; they carry its id, and the listener ignores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionId(pub u64);

/// What the session reports: each status the renderer gives, each command it
/// refused, and the session's end.
#[derive(Debug, Clone, PartialEq)]
pub enum RendererSessionEvent {
    /// A status read off, or said by, the renderer, and when it arrived.
    Status {
        session: SessionId,
        status: ReceiverStatus,
        observed_at: chrono::DateTime<chrono::Utc>,
    },
    /// The renderer refused a command — a load, a play, a seek. The session
    /// goes on: the link is still up, and the next command may land.
    CommandFailed { session: SessionId, error: RendererError },
    /// The link to the renderer was lost: the device stopped its app, went
    /// unreachable, or the network dropped. No further events follow.
    Ended { session: SessionId, error: RendererError },
}

impl RendererSessionEvent {
    pub fn session(&self) -> SessionId {
        match self {
            Self::Status { session, .. }
            | Self::CommandFailed { session, .. }
            | Self::Ended { session, .. } => *session,
        }
    }
}

/// Called on the session thread for every event. The playback service supplies
/// one that dispatches the event back onto its command loop.
pub type EventCallback = Arc<dyn Fn(RendererSessionEvent) + Send + Sync>;

/// Where a pushed renderer says what it is doing: each status as it changes,
/// stamped with the playback clock as it is said, and the loss of its link.
#[derive(Clone)]
pub struct StatusSink {
    session: SessionId,
    clock: PlaybackClockRef,
    on_event: EventCallback,
}

impl StatusSink {
    pub fn new(session: SessionId, clock: PlaybackClockRef, on_event: EventCallback) -> Self {
        Self { session, clock, on_event }
    }

    /// Say `status`, as of now.
    pub fn report(&self, status: ReceiverStatus) {
        (self.on_event)(RendererSessionEvent::Status {
            session: self.session,
            status,
            observed_at: self.clock.now(),
        });
    }

    /// Say the renderer refused a command; the link is still up.
    fn command_failed(&self, error: RendererError) {
        (self.on_event)(RendererSessionEvent::CommandFailed { session: self.session, error });
    }

    /// Say the link to the renderer is lost, with why.
    pub fn lost(&self, error: RendererError) {
        (self.on_event)(RendererSessionEvent::Ended { session: self.session, error });
    }
}

/// Commands the session thread executes against the channel.
enum SessionCommand {
    Load(Box<RendererMedia>),
    Play,
    Pause,
    Seek(Duration),
    SetVolume(crate::Volume),
    Stop,
}

/// A connected remote-renderer session. Dropping it stops the renderer, ends the
/// poll loop and joins its thread. Commands are fire-and-forget onto the session
/// thread.
pub struct RendererSession {
    /// `None` only transiently, during `Drop`: the sender is dropped first so
    /// the thread's `recv_timeout` sees the disconnect and exits before the
    /// join.
    command_tx: Option<mpsc::Sender<SessionCommand>>,
    thread: Option<ReportedThread>,
}

impl RendererSession {
    /// Start session `id` over an already-connected `link`, spawning its
    /// thread. `on_event` receives every status, every refused command, and
    /// the session's end. A session thread that panics ends the session with
    /// the panic as its cause.
    pub fn start(
        diagnostics: &Diagnostics,
        link: RendererLink,
        clock: PlaybackClockRef,
        id: SessionId,
        on_event: EventCallback,
    ) -> std::io::Result<Self> {
        let (command_tx, command_rx) = mpsc::channel();
        let on_panic_event = on_event.clone();
        let thread = diagnostics.spawn_thread_reported(
            "renderer session",
            move || run_session(link, command_rx, StatusSink::new(id, clock, on_event)),
            move |failed| {
                on_panic_event(RendererSessionEvent::Ended {
                    session: id,
                    error: RendererError::Panicked(failed),
                })
            },
        )?;
        Ok(Self { command_tx: Some(command_tx), thread: Some(thread) })
    }

    /// Load `media` on the renderer and start playing it.
    pub fn load(&self, media: RendererMedia) {
        self.send(SessionCommand::Load(Box::new(media)));
    }

    pub fn play(&self) {
        self.send(SessionCommand::Play);
    }

    pub fn pause(&self) {
        self.send(SessionCommand::Pause);
    }

    pub fn seek(&self, position: Duration) {
        self.send(SessionCommand::Seek(position));
    }

    pub fn set_volume(&self, level: crate::Volume) {
        self.send(SessionCommand::SetVolume(level));
    }

    fn send(&self, command: SessionCommand) {
        let sent = self.command_tx.as_ref().is_some_and(|tx| tx.send(command).is_ok());
        if !sent {
            debug!("renderer session command dropped: the session thread has ended");
        }
    }
}

impl Drop for RendererSession {
    fn drop(&mut self) {
        // However the session's owner ended, the renderer must not play on
        // unattended: the stop goes ahead of the disconnect, so the thread
        // runs it before it exits. A session whose link was lost has no
        // thread left to take it. This is the only stop the renderer is
        // told: the session's owner ends a cast by dropping it.
        self.send(SessionCommand::Stop);
        // Drop the sender first so the thread's `recv_timeout` wakes with a
        // disconnect and exits its loop; only then can the join complete.
        self.command_tx = None;
        if let Some(thread) = self.thread.take() {
            thread.join();
        }
    }
}

/// The session thread body. A polled renderer interleaves commands with a
/// status poll every [`POLL_INTERVAL`], reporting each status; a connection
/// failure — from a command or a poll — ends the session (the renderer
/// stopped, or the network dropped): report the end, with why, once and exit.
/// A pushed renderer only has its commands run here: the end of its link is
/// said by the renderer itself, through the sink, so a connection failure of a
/// command stops the thread without a second report. The session's owner
/// dropping it ends the thread quietly: nobody is listening.
fn run_session(link: RendererLink, command_rx: mpsc::Receiver<SessionCommand>, sink: StatusSink) {
    match link {
        RendererLink::Polled(mut polled) => {
            if let ControlFlow::Break(error) = drive_polled(polled.as_mut(), &command_rx, &sink) {
                warn!("renderer session ended: {error}");
                sink.lost(error);
            }
        }
        RendererLink::Pushed(mut pushed) => {
            pushed.report_to(sink.clone());
            // A disconnect of the commands is the owner dropping the session.
            while let Ok(command) = command_rx.recv() {
                if let ControlFlow::Break(error) = run_command(pushed.as_mut(), command, &sink) {
                    debug!("renderer session's commands ended with its link: {error}");
                    return;
                }
            }
        }
    }
}

/// Run a polled session until its link is lost (`Break`, with why) or its
/// owner drops it (`Continue`).
fn drive_polled(
    renderer: &mut dyn PolledRenderer,
    command_rx: &mpsc::Receiver<SessionCommand>,
    sink: &StatusSink,
) -> ControlFlow<RendererError> {
    // An initial poll surfaces the renderer's starting state promptly.
    poll_and_report(renderer, sink)?;
    loop {
        match command_rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => {
                run_command(renderer, command, sink)?;
                // Reflect the command's effect immediately rather than waiting
                // for the next poll tick.
                poll_and_report(renderer, sink)?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => poll_and_report(renderer, sink)?,
            Err(mpsc::RecvTimeoutError::Disconnected) => return ControlFlow::Continue(()),
        }
    }
}

/// Run one command. A [`RendererError::Connection`] is terminal (`Break`); a
/// command the renderer refused is warned and reported, and the session goes
/// on.
fn run_command(
    channel: &mut dyn RendererChannel,
    command: SessionCommand,
    sink: &StatusSink,
) -> ControlFlow<RendererError> {
    let result = match command {
        SessionCommand::Load(media) => channel.load(&media),
        SessionCommand::Play => channel.play(),
        SessionCommand::Pause => channel.pause(),
        SessionCommand::Seek(position) => channel.seek(position),
        SessionCommand::SetVolume(level) => channel.set_volume(level),
        SessionCommand::Stop => channel.stop(),
    };
    match result {
        Ok(()) => ControlFlow::Continue(()),
        Err(error @ RendererError::Connection(_)) => ControlFlow::Break(error),
        Err(error) => {
            warn!("renderer command failed (session continues): {error}");
            sink.command_failed(error);
            ControlFlow::Continue(())
        }
    }
}

/// Poll status and report it, stamped as the poll returns: the status is as
/// old as that. A connection failure is terminal; any other failure to read
/// status is warned and the next poll tries again.
fn poll_and_report(
    renderer: &mut dyn PolledRenderer,
    sink: &StatusSink,
) -> ControlFlow<RendererError> {
    match renderer.poll_status() {
        Ok(status) => {
            sink.report(status);
            ControlFlow::Continue(())
        }
        Err(error @ RendererError::Connection(_)) => ControlFlow::Break(error),
        Err(error) => {
            warn!("renderer status poll failed (session continues): {error}");
            ControlFlow::Continue(())
        }
    }
}
