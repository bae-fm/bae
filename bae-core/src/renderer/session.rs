//! A live remote-renderer session: one connected device, driven on its own
//! thread.
//!
//! [`RendererSession`] owns a [`RendererChannel`] and the thread that talks to
//! it. The thread interleaves commands (load/play/pause/seek/volume/stop) with a
//! status poll on a fixed interval, reporting every status, every refused
//! command, and the session's end with its cause, to a caller-supplied
//! callback. The callback is the seam to the playback service: the session
//! never names playback types, so this stays decoupled from the renderer that
//! consumes it. Both the Cast and the UPnP channels are driven
//! by this one session unchanged.

use std::ops::ControlFlow;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use tracing::{debug, warn};

use super::channel::{ReceiverStatus, RendererChannel, RendererError, RendererMedia};

/// How often the session polls the renderer for status between commands.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// What the session reports: each status the renderer gives, each command it
/// refused, and the session's end.
#[derive(Debug, Clone, PartialEq)]
pub enum RendererSessionEvent {
    /// A status read off the renderer.
    Status(ReceiverStatus),
    /// The renderer refused a command — a load, a play, a seek. The session
    /// goes on: the link is still up, and the next command may land.
    CommandFailed(RendererError),
    /// The link to the renderer was lost: the device stopped its app, went
    /// unreachable, or the network dropped. No further events follow.
    Ended(RendererError),
}

/// Called on the session thread for every event. The playback service supplies
/// one that dispatches the event back onto its command loop.
pub type EventCallback = Arc<dyn Fn(RendererSessionEvent) + Send + Sync>;

/// Commands the session thread executes against the channel.
enum SessionCommand {
    Load(Box<RendererMedia>),
    Play,
    Pause,
    Seek(Duration),
    SetVolume(f32),
    Stop,
}

/// A connected remote-renderer session. Dropping it ends the poll loop and joins
/// its thread. Commands are fire-and-forget onto the session thread.
pub struct RendererSession {
    /// `None` only transiently, during `Drop`: the sender is dropped first so
    /// the thread's `recv_timeout` sees the disconnect and exits before the
    /// join.
    command_tx: Option<mpsc::Sender<SessionCommand>>,
    thread: Option<JoinHandle<()>>,
}

impl RendererSession {
    /// Start a session over an already-connected `channel`, spawning its poll
    /// thread. `on_event` receives every status, every refused command, and
    /// the session's end.
    pub fn start(channel: Box<dyn RendererChannel>, on_event: EventCallback) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || run_session(channel, command_rx, on_event));
        Self {
            command_tx: Some(command_tx),
            thread: Some(thread),
        }
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

    pub fn set_volume(&self, level: f32) {
        self.send(SessionCommand::SetVolume(level));
    }

    /// Stop playback on the renderer, leaving the session connected.
    pub fn stop(&self) {
        self.send(SessionCommand::Stop);
    }

    fn send(&self, command: SessionCommand) {
        let sent = self
            .command_tx
            .as_ref()
            .is_some_and(|tx| tx.send(command).is_ok());
        if !sent {
            debug!("renderer session command dropped: the session thread has ended");
        }
    }
}

impl Drop for RendererSession {
    fn drop(&mut self) {
        // Drop the sender first so the thread's `recv_timeout` wakes with a
        // disconnect and exits its loop; only then can the join complete.
        self.command_tx = None;
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                warn!("renderer session thread panicked before join");
            }
        }
    }
}

/// The session thread body: interleave commands with a status poll every
/// [`POLL_INTERVAL`], reporting each status. A connection failure — from a
/// command or a poll — ends the session (the renderer stopped, or the network
/// dropped): report the end, with why, once and exit. The session's owner
/// dropping it ends the thread quietly: nobody is listening.
fn run_session(
    mut channel: Box<dyn RendererChannel>,
    command_rx: mpsc::Receiver<SessionCommand>,
    on_event: EventCallback,
) {
    if let ControlFlow::Break(error) = drive(channel.as_mut(), &command_rx, &on_event) {
        warn!("renderer session ended: {error}");
        on_event(RendererSessionEvent::Ended(error));
    }
}

/// Run the session until its link is lost (`Break`, with why) or its owner
/// drops it (`Continue`).
fn drive(
    channel: &mut dyn RendererChannel,
    command_rx: &mpsc::Receiver<SessionCommand>,
    on_event: &EventCallback,
) -> ControlFlow<RendererError> {
    // An initial poll surfaces the renderer's starting state promptly.
    poll_and_report(channel, on_event)?;
    loop {
        match command_rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => {
                run_command(channel, command, on_event)?;
                // Reflect the command's effect immediately rather than waiting
                // for the next poll tick.
                poll_and_report(channel, on_event)?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => poll_and_report(channel, on_event)?,
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
    on_event: &EventCallback,
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
            on_event(RendererSessionEvent::CommandFailed(error));
            ControlFlow::Continue(())
        }
    }
}

/// Poll status and report it. A connection failure is terminal; any other
/// failure to read status is warned and the next poll tries again.
fn poll_and_report(
    channel: &mut dyn RendererChannel,
    on_event: &EventCallback,
) -> ControlFlow<RendererError> {
    match channel.poll_status() {
        Ok(status) => {
            on_event(RendererSessionEvent::Status(status));
            ControlFlow::Continue(())
        }
        Err(error @ RendererError::Connection(_)) => ControlFlow::Break(error),
        Err(error) => {
            warn!("renderer status poll failed (session continues): {error}");
            ControlFlow::Continue(())
        }
    }
}
