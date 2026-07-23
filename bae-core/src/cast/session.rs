//! A live cast session: one connected device, driven on its own thread.
//!
//! [`CastSession`] owns a [`CastChannel`] and the thread that talks to it. The
//! thread interleaves commands (load/play/pause/seek/volume/stop) with a status
//! poll on a fixed interval, reporting every status — and the session's end — to
//! a caller-supplied callback. The callback is the seam to the playback service:
//! the session never names playback types, so bae-core's cast module stays
//! decoupled from the renderer that consumes it.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use tracing::{debug, warn};

use super::channel::{CastChannel, CastError, CastMedia, CastPlayerState, ReceiverStatus};

/// How often the session polls the receiver for status between commands.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A status update from the session's poll loop. `ended` marks the terminal
/// update: the connection was lost or the receiver stopped, and no further
/// updates follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CastSessionStatus {
    pub player_state: CastPlayerState,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub volume: f32,
    pub ended: bool,
}

impl CastSessionStatus {
    fn from_receiver(status: ReceiverStatus) -> Self {
        Self {
            player_state: status.player_state,
            position: status.position,
            duration: status.duration,
            volume: status.volume,
            ended: false,
        }
    }

    /// The terminal update sent once when the session ends.
    fn ended() -> Self {
        Self {
            player_state: CastPlayerState::Idle,
            position: None,
            duration: None,
            volume: 0.0,
            ended: true,
        }
    }
}

/// Called on the session thread for every status change and once for session
/// end. The playback service supplies one that dispatches the status back onto
/// its command loop.
pub type StatusCallback = Arc<dyn Fn(CastSessionStatus) + Send + Sync>;

/// Commands the session thread executes against the channel.
enum SessionCommand {
    Load(Box<CastMedia>),
    Play,
    Pause,
    Seek(Duration),
    SetVolume(f32),
    Stop,
}

/// A connected cast session. Dropping it ends the poll loop and joins its
/// thread. Commands are fire-and-forget onto the session thread.
pub struct CastSession {
    /// `None` only transiently, during `Drop`: the sender is dropped first so
    /// the thread's `recv_timeout` sees the disconnect and exits before the
    /// join.
    command_tx: Option<mpsc::Sender<SessionCommand>>,
    thread: Option<JoinHandle<()>>,
}

impl CastSession {
    /// Start a session over an already-connected `channel`, spawning its poll
    /// thread. `on_status` receives every status update and the terminal
    /// `ended` update.
    pub fn start(channel: Box<dyn CastChannel>, on_status: StatusCallback) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || run_session(channel, command_rx, on_status));
        Self {
            command_tx: Some(command_tx),
            thread: Some(thread),
        }
    }

    /// Load `media` on the receiver and start playing it.
    pub fn load(&self, media: CastMedia) {
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

    /// Stop playback on the receiver, leaving the session connected.
    pub fn stop(&self) {
        self.send(SessionCommand::Stop);
    }

    fn send(&self, command: SessionCommand) {
        let sent = self
            .command_tx
            .as_ref()
            .is_some_and(|tx| tx.send(command).is_ok());
        if !sent {
            debug!("cast session command dropped: the session thread has ended");
        }
    }
}

impl Drop for CastSession {
    fn drop(&mut self) {
        // Drop the sender first so the thread's `recv_timeout` wakes with a
        // disconnect and exits its loop; only then can the join complete.
        self.command_tx = None;
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                warn!("cast session thread panicked before join");
            }
        }
    }
}

/// The session thread body: interleave commands with a status poll every
/// [`POLL_INTERVAL`], reporting each status. A connection failure — from a
/// command or a poll — ends the session (the receiver stopped, or the network
/// dropped): report the terminal update once and exit.
fn run_session(
    mut channel: Box<dyn CastChannel>,
    command_rx: mpsc::Receiver<SessionCommand>,
    on_status: StatusCallback,
) {
    // An initial poll surfaces the receiver's starting state promptly.
    if poll_and_report(channel.as_mut(), &on_status).is_break() {
        on_status(CastSessionStatus::ended());
        return;
    }

    loop {
        match command_rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => {
                if run_command(channel.as_mut(), command).is_break() {
                    on_status(CastSessionStatus::ended());
                    return;
                }
                // Reflect the command's effect immediately rather than waiting
                // for the next poll tick.
                if poll_and_report(channel.as_mut(), &on_status).is_break() {
                    on_status(CastSessionStatus::ended());
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if poll_and_report(channel.as_mut(), &on_status).is_break() {
                    on_status(CastSessionStatus::ended());
                    return;
                }
            }
            // The session was dropped: end quietly without a terminal callback
            // (the owner is going away and isn't listening).
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Run one command. A [`CastError::Connection`] is terminal (`Break`); other
/// errors are logged and playback continues (`Continue`).
fn run_command(
    channel: &mut dyn CastChannel,
    command: SessionCommand,
) -> std::ops::ControlFlow<()> {
    let result = match command {
        SessionCommand::Load(media) => channel.load(&media),
        SessionCommand::Play => channel.play(),
        SessionCommand::Pause => channel.pause(),
        SessionCommand::Seek(position) => channel.seek(position),
        SessionCommand::SetVolume(level) => channel.set_volume(level),
        SessionCommand::Stop => channel.stop(),
    };
    classify(result)
}

/// Poll status and report it. A connection failure is terminal.
fn poll_and_report(
    channel: &mut dyn CastChannel,
    on_status: &StatusCallback,
) -> std::ops::ControlFlow<()> {
    match channel.poll_status() {
        Ok(status) => {
            on_status(CastSessionStatus::from_receiver(status));
            std::ops::ControlFlow::Continue(())
        }
        Err(error) => classify(Err(error)),
    }
}

/// Map a channel result to control flow: a lost connection ends the session; any
/// other error is logged and the session continues.
fn classify(result: Result<(), CastError>) -> std::ops::ControlFlow<()> {
    match result {
        Ok(()) => std::ops::ControlFlow::Continue(()),
        Err(CastError::Connection(detail)) => {
            debug!("cast session ending: {detail}");
            std::ops::ControlFlow::Break(())
        }
        Err(error) => {
            warn!("cast command failed (session continues): {error}");
            std::ops::ControlFlow::Continue(())
        }
    }
}
