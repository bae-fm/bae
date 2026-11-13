//! bae's Cast client: launch the Default Media Receiver, load tracks into it
//! and follow it.
//!
//! What the client holds is what the receiver last said in RECEIVER_STATUS and
//! MEDIA_STATUS, written only by the connection's reader. A command never
//! moves it: a command waits for the receiver's answer, with a deadline, and
//! the answer is a message like any other. A request is answered by the
//! message that carries its id; a message nobody asked for, such as a pause
//! from the device's buttons, is followed the same way.
//!
//! The connection ends once, for one reason (see [`End`]). The client ends it
//! when the app is gone from the receiver, when its transport closes, and
//! when another sender takes the player over. Every request still waiting
//! then fails with [`RendererError::Connection`].

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bae_diagnostics::{lock_or_crash, Diagnostics};
use bae_playback::renderer::{
    ReceiverStatus, RendererError, RendererMedia, RendererPlayerState, StatusSink,
};
use bae_playback::Volume;
use tokio::sync::oneshot;
use tracing::{debug, warn};

use super::channel::receiver_time;
use super::connection::{ended_error, CastConnection, End, Handler, OnEnd};
use super::messages::{IdleReason, Inbound, MediaStatusEntry, PlayerState, Request, StatedRate};

/// The Default Media Receiver, Google's built-in player.
const MEDIA_RECEIVER_APP: &str = "CC1AD845";

/// The id every message from bae carries.
const SENDER_ID: &str = "sender-0";
const RECEIVER_ID: &str = "receiver-0";

/// How long the receiver has to answer the launch, and each command.
#[derive(Clone, Copy)]
pub(crate) struct Deadlines {
    pub(crate) launch: Duration,
    pub(crate) command: Duration,
}

pub(crate) const DEADLINES: Deadlines =
    Deadlines { launch: Duration::from_secs(10), command: Duration::from_secs(10) };

/// The app the receiver launched.
#[derive(Debug, Clone, PartialEq)]
struct App {
    session_id: String,
    transport_id: String,
}

/// bae's media session as the receiver last described it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MediaSessionView {
    pub(crate) id: i32,
    pub(crate) state: RendererPlayerState,
    /// Seconds into the media, when the receiver stated a number.
    pub(crate) current_time: Option<f64>,
    /// How fast the position moves while playing, when the receiver stated a
    /// number.
    pub(crate) playback_rate: Option<f64>,
    /// Seconds, from the last status that carried the media.
    pub(crate) duration: Option<f64>,
}

impl MediaSessionView {
    fn from_entry(entry: &MediaStatusEntry, previous_duration: Option<f64>) -> Self {
        Self {
            id: entry.media_session_id,
            state: entry.renderer_state(),
            current_time: entry.current_time,
            playback_rate: readable_rate(&entry.playback_rate),
            // The receiver sends `media` only when it changed.
            duration: entry.media.as_ref().map_or(previous_duration, |media| media.duration),
        }
    }
}

enum RequestKind {
    Launch,
    Load,
    Other,
}

enum Answered {
    Launched(App),
    Done,
}

struct WaitingRequest {
    kind: RequestKind,
    reply: oneshot::Sender<Result<Answered, RendererError>>,
}

/// Everything the client holds. Written by the reader (and by a command
/// adding or removing its own waiting request), under a lock held only for a
/// change, never across an await. The sink playback hands over is told of each
/// change under that lock, so what it hears is in the order the receiver said
/// it; telling it only queues an event.
struct ClientState {
    app: Option<App>,
    volume: Option<f64>,
    media: Option<MediaSessionView>,
    /// The session before the one now loaded: the receiver may still say
    /// that it was interrupted, after it has answered the load.
    previous_media_id: Option<i32>,
    waiting: HashMap<u32, WaitingRequest>,
    /// Where playback hears the receiver's status, once it has asked to.
    /// Taken when the loss is said, so a loss is said once.
    sink: Option<StatusSink>,
}

const TOOK_OVER: &str = "another sender took over the receiver";

impl ClientState {
    fn new() -> Self {
        Self {
            app: None,
            volume: None,
            media: None,
            previous_media_id: None,
            waiting: HashMap::new(),
            sink: None,
        }
    }

    /// What the receiver last said about bae's media and its volume.
    fn status(&self) -> ReceiverStatus {
        let media = self.media.as_ref();
        ReceiverStatus {
            player_state: media.map_or(RendererPlayerState::Idle, |media| media.state),
            position: media.and_then(|media| media.current_time).and_then(receiver_time),
            duration: media.and_then(|media| media.duration).and_then(receiver_time),
            volume: self.volume.map(|level| level as f32),
            rate: media.and_then(|media| media.playback_rate),
        }
    }

    /// Say the status now held to playback, if it is listening.
    fn report(&self) {
        if let Some(sink) = &self.sink {
            sink.report(self.status());
        }
    }

    fn complete(&mut self, request_id: u32, answer: Result<Answered, RendererError>) {
        match self.waiting.remove(&request_id) {
            // A command that gave up has already left the table.
            Some(waiting) => {
                let _gone = waiting.reply.send(answer);
            }
            None => debug!(request_id, "cast answer to a request nobody waits on"),
        }
    }

    /// Take in one message from the receiver. `Ok` says whether it changed
    /// what bae's media or the receiver's volume is. `Err` is the reason the
    /// connection is lost.
    fn take(&mut self, inbound: Inbound) -> Result<bool, String> {
        match inbound {
            Inbound::Close { source_id } => Err(format!("{source_id} closed its connection")),
            Inbound::ReceiverStatus { request_id, status } => {
                let volume_stated = status.volume.and_then(|volume| volume.level);
                if let Some(level) = volume_stated {
                    self.volume = Some(level);
                }
                let moved = volume_stated.is_some();
                if let Some(app) = &self.app {
                    if !status.applications.iter().any(|a| a.session_id == app.session_id) {
                        return Err("the receiver's media app ended".to_string());
                    }
                }
                let launching = matches!(
                    self.waiting.get(&request_id),
                    Some(WaitingRequest { kind: RequestKind::Launch, .. })
                );
                if !launching {
                    self.complete(request_id, Ok(Answered::Done));
                    return Ok(moved);
                }
                let launched =
                    status.applications.iter().find(|a| a.app_id == MEDIA_RECEIVER_APP).and_then(
                        |a| {
                            Some(App {
                                session_id: a.session_id.clone(),
                                transport_id: a.transport_id.clone()?,
                            })
                        },
                    );
                match launched {
                    Some(app) => {
                        self.app = Some(app.clone());
                        self.complete(request_id, Ok(Answered::Launched(app)));
                    }
                    None => self.complete(
                        request_id,
                        Err(RendererError::Launch(
                            "the receiver's status for the launch lists no media app".to_string(),
                        )),
                    ),
                }
                Ok(moved)
            }
            Inbound::LaunchStatus { request_id, status } => {
                debug!(request_id, ?status, "cast launch status");
                Ok(false)
            }
            Inbound::LaunchError { request_id, reason, extended_error } => {
                let mut text = reason.unwrap_or_else(|| "no reason given".to_string());
                if let Some(extended) = extended_error {
                    text.push_str(&format!(" ({extended})"));
                }
                self.complete(request_id, Err(RendererError::Launch(text)));
                Ok(false)
            }
            Inbound::Refused(refusal) => {
                let error = match self.waiting.get(&refusal.request_id) {
                    Some(WaitingRequest { kind: RequestKind::Launch, .. }) => {
                        RendererError::Launch(refusal.describe())
                    }
                    _ => RendererError::Command(refusal.describe()),
                };
                self.complete(refusal.request_id, Err(error));
                Ok(false)
            }
            Inbound::MediaStatus { request_id, entries } => {
                self.take_media_status(request_id, &entries)
            }
            // The connection answers these itself and drops these.
            Inbound::Ping | Inbound::Pong | Inbound::Ignored { .. } => Ok(false),
        }
    }

    fn take_media_status(
        &mut self,
        request_id: u32,
        entries: &[MediaStatusEntry],
    ) -> Result<bool, String> {
        let answers_a_load = matches!(
            self.waiting.get(&request_id),
            Some(WaitingRequest { kind: RequestKind::Load, .. })
        );
        if answers_a_load {
            let Some(entry) = entries.first() else {
                self.complete(
                    request_id,
                    Err(RendererError::Command(
                        "the receiver's answer to the load lists no media".to_string(),
                    )),
                );
                return Ok(false);
            };
            self.previous_media_id = self.media.take().map(|media| media.id);
            self.media = Some(MediaSessionView::from_entry(entry, None));
            self.complete(request_id, Ok(Answered::Done));
            return Ok(true);
        }
        let load_pending = self.waiting.values().any(|w| matches!(w.kind, RequestKind::Load));
        let mut moved = false;
        for entry in entries {
            moved |= self.follow(entry, load_pending)?;
        }
        if entries.is_empty() {
            if let Some(media) = &mut self.media {
                media.state = RendererPlayerState::Idle;
                media.current_time = None;
                moved = true;
            }
        }
        self.complete(request_id, Ok(Answered::Done));
        Ok(moved)
    }

    /// Follow one entry of a status nobody's load answers. `Ok` says whether
    /// it changed bae's media.
    fn follow(&mut self, entry: &MediaStatusEntry, load_pending: bool) -> Result<bool, String> {
        if Some(entry.media_session_id) == self.previous_media_id {
            return Ok(false);
        }
        let Some(media) = &mut self.media else {
            debug!(
                session = entry.media_session_id,
                "cast media status while bae has no media loaded"
            );
            return Ok(false);
        };
        if media.id != entry.media_session_id {
            // While a load waits, its answer settles which session is bae's.
            return if load_pending { Ok(false) } else { Err(TOOK_OVER.to_string()) };
        }
        let interrupted = entry.player_state == PlayerState::Idle
            && entry.idle_reason == Some(IdleReason::Interrupted);
        if interrupted {
            return if load_pending { Ok(false) } else { Err(TOOK_OVER.to_string()) };
        }
        *media = MediaSessionView::from_entry(entry, media.duration);
        Ok(true)
    }
}

/// A connected Cast client: the receiver's media app is launched and bae can
/// load tracks into it.
pub(crate) struct CastClient {
    connection: CastConnection,
    state: Arc<Mutex<ClientState>>,
    diagnostics: Diagnostics,
    next_request_id: AtomicU32,
    deadlines: Deadlines,
}

impl CastClient {
    /// Connect to the receiver at `addr:port` and launch its media app.
    /// `Connection` when the receiver can't be reached, `Launch` when its app
    /// won't start.
    pub(crate) async fn connect(
        addr: IpAddr,
        port: u16,
        diagnostics: &Diagnostics,
        runtime: &tokio::runtime::Handle,
    ) -> Result<Self, RendererError> {
        let (state, handler, on_end) = session(diagnostics);
        let connection =
            CastConnection::open(addr, port, diagnostics, runtime, handler, on_end).await?;
        Self::launch(connection, state, diagnostics, DEADLINES).await
    }

    /// Run a client over `stream`, which is already connected.
    #[cfg(test)]
    pub(crate) async fn connect_over<S>(
        stream: S,
        diagnostics: &Diagnostics,
        runtime: &tokio::runtime::Handle,
        deadlines: Deadlines,
    ) -> Result<Self, RendererError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
    {
        let (state, handler, on_end) = session(diagnostics);
        let connection = CastConnection::start(stream, diagnostics, runtime, handler, on_end);
        Self::launch(connection, state, diagnostics, deadlines).await
    }

    async fn launch(
        connection: CastConnection,
        state: Arc<Mutex<ClientState>>,
        diagnostics: &Diagnostics,
        deadlines: Deadlines,
    ) -> Result<Self, RendererError> {
        let client = Self {
            connection,
            state,
            diagnostics: diagnostics.clone(),
            next_request_id: AtomicU32::new(1),
            deadlines,
        };
        client.connection.send(Request::Connect, SENDER_ID, RECEIVER_ID)?;
        let launched = client
            .ask(RequestKind::Launch, deadlines.launch, RECEIVER_ID, |request_id| Request::Launch {
                request_id,
                app_id: MEDIA_RECEIVER_APP,
            })
            .await
            .map_err(|error| match error {
                RendererError::Command(text) => RendererError::Launch(text),
                other => other,
            })?;
        let Answered::Launched(app) = launched else {
            unreachable!("only the launch's answer completes a launch");
        };
        client.connection.send(Request::Connect, SENDER_ID, &app.transport_id)?;
        Ok(client)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ClientState> {
        lock_or_crash(&self.diagnostics, "the cast client's state", &self.state)
    }

    /// Send a request to `destination` and wait for its answer, within
    /// `deadline`. A request the receiver doesn't answer in time fails as
    /// `Command` and leaves the waiting table.
    async fn ask<'a>(
        &self,
        kind: RequestKind,
        deadline: Duration,
        destination: &str,
        request: impl FnOnce(u32) -> Request<'a>,
    ) -> Result<Answered, RendererError> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        self.state().waiting.insert(request_id, WaitingRequest { kind, reply });
        // The end is set before the waiting table is failed, so a request
        // added after that check is failed by the end that follows it.
        if let Some(end) = self.connection.end() {
            self.state().waiting.remove(&request_id);
            return Err(ended_error(end));
        }
        if let Err(error) = self.connection.send(request(request_id), SENDER_ID, destination) {
            self.state().waiting.remove(&request_id);
            return Err(error);
        }
        match tokio::time::timeout(deadline, answer).await {
            Ok(Ok(answered)) => answered,
            Ok(Err(_dropped)) => {
                self.state().waiting.remove(&request_id);
                Err(self.connection.end().map_or_else(
                    || RendererError::Connection("the answer was dropped".to_string()),
                    ended_error,
                ))
            }
            Err(_elapsed) => {
                self.state().waiting.remove(&request_id);
                Err(RendererError::Command(format!(
                    "the receiver did not answer within {} s",
                    deadline.as_secs()
                )))
            }
        }
    }

    fn app(&self) -> Result<App, RendererError> {
        self.state().app.clone().ok_or_else(|| {
            RendererError::Command("the receiver's media app is not running".to_string())
        })
    }

    fn media_session_id(&self) -> Result<i32, RendererError> {
        self.state().media.as_ref().map(|media| media.id).ok_or_else(|| {
            RendererError::Command("no media loaded on the receiver yet".to_string())
        })
    }

    /// Load `media` and start it. The answer names bae's media session.
    pub(crate) async fn load(&self, media: &RendererMedia) -> Result<(), RendererError> {
        let app = self.app()?;
        self.ask(RequestKind::Load, self.deadlines.command, &app.transport_id, |request_id| {
            Request::Load { request_id, session_id: &app.session_id, media }
        })
        .await
        .map(|_| ())
    }

    pub(crate) async fn play(&self) -> Result<(), RendererError> {
        let media_session_id = self.media_session_id()?;
        self.media_command(|request_id| Request::Play { request_id, media_session_id }).await
    }

    pub(crate) async fn pause(&self) -> Result<(), RendererError> {
        let media_session_id = self.media_session_id()?;
        self.media_command(|request_id| Request::Pause { request_id, media_session_id }).await
    }

    pub(crate) async fn seek(&self, position: Duration) -> Result<(), RendererError> {
        let media_session_id = self.media_session_id()?;
        self.media_command(|request_id| Request::Seek { request_id, media_session_id, position })
            .await
    }

    pub(crate) async fn stop(&self) -> Result<(), RendererError> {
        let media_session_id = self.media_session_id()?;
        self.media_command(|request_id| Request::StopMedia { request_id, media_session_id }).await
    }

    async fn media_command<'a>(
        &self,
        request: impl FnOnce(u32) -> Request<'a>,
    ) -> Result<(), RendererError> {
        let app = self.app()?;
        self.ask(RequestKind::Other, self.deadlines.command, &app.transport_id, request)
            .await
            .map(|_| ())
    }

    /// Set the receiver's volume.
    pub(crate) async fn set_volume(&self, level: Volume) -> Result<(), RendererError> {
        self.ask(RequestKind::Other, self.deadlines.command, RECEIVER_ID, |request_id| {
            Request::SetVolume { request_id, level: level.get() }
        })
        .await
        .map(|_| ())
    }

    /// Say the receiver's status to `sink` from now on: what it holds now,
    /// each change as the receiver says it, and the loss of the connection. A
    /// connection already lost is said at once.
    pub(crate) fn report_to(&self, sink: StatusSink) {
        let mut state = self.state();
        if let Some(End::Lost(reason)) = self.connection.end() {
            sink.lost(RendererError::Connection(reason.clone()));
            return;
        }
        state.sink = Some(sink);
        state.report();
    }

    /// What the receiver last said about bae's media and its volume.
    #[cfg(test)]
    pub(crate) fn status(&self) -> ReceiverStatus {
        self.state().status()
    }

    /// bae's media session as the receiver last described it.
    #[cfg(test)]
    pub(crate) fn media_session(&self) -> Option<MediaSessionView> {
        self.state().media.clone()
    }

    #[cfg(test)]
    pub(crate) fn end(&self) -> Option<&End> {
        self.connection.end()
    }

    /// Wait for the connection's end.
    #[cfg(test)]
    pub(crate) async fn ended(&self) -> End {
        self.connection.ended().await
    }

    /// How many requests wait for an answer.
    #[cfg(test)]
    pub(crate) fn waiting_requests(&self) -> usize {
        self.state().waiting.len()
    }

    /// End the session in order: stop the media, if the receiver says it is
    /// not idle, and wait for the answer, then
    /// close the connections to the app and the receiver, then the socket.
    /// Nothing is reported as lost.
    pub(crate) async fn close(&self) {
        // Playback has ended the session: nothing is said to it from here on,
        // not even the answer to the stop below.
        self.state().sink = None;
        let playing =
            self.state().media.as_ref().is_some_and(|m| m.state != RendererPlayerState::Idle);
        if playing {
            if let Err(error) = self.stop().await {
                debug!("cast media stop before closing failed: {error}");
            }
        }
        let mut farewells = Vec::new();
        if let Ok(app) = self.app() {
            farewells.push(Request::Close.into_frame(SENDER_ID, &app.transport_id));
        }
        farewells.push(Request::Close.into_frame(SENDER_ID, RECEIVER_ID));
        self.connection.close(farewells).await;
    }
}

/// The rate a receiver stated, when it can be used: finite and above zero. One
/// it stated that cannot be, a number or not, is said with what it was, and
/// the position holds between statuses.
fn readable_rate(stated: &StatedRate) -> Option<f64> {
    match stated {
        StatedRate::Absent => None,
        StatedRate::Number(rate) if rate.is_finite() && *rate > 0.0 => Some(*rate),
        StatedRate::Number(rate) => {
            warn!("the Cast receiver stated a playback rate of {rate}; the position holds between statuses");
            None
        }
        StatedRate::NotANumber(text) => {
            warn!("the Cast receiver stated a playback rate of {text}, which is no number; the position holds between statuses");
            None
        }
    }
}

/// The state of a new session and the two callbacks that feed the connection's
/// messages and its end into it.
fn session(diagnostics: &Diagnostics) -> (Arc<Mutex<ClientState>>, Handler, OnEnd) {
    let state = Arc::new(Mutex::new(ClientState::new()));
    let for_messages = state.clone();
    let for_end = state.clone();
    let diagnostics_for_messages = diagnostics.clone();
    let diagnostics_for_end = diagnostics.clone();
    let handler: Handler = Box::new(move |inbound| {
        let mut state =
            lock_or_crash(&diagnostics_for_messages, "the cast client's state", &for_messages);
        if state.take(inbound)? {
            state.report();
        }
        Ok(())
    });
    let on_end: OnEnd = Box::new(move |end| {
        let (waiting, sink) = {
            let mut state =
                lock_or_crash(&diagnostics_for_end, "the cast client's state", &for_end);
            (std::mem::take(&mut state.waiting), state.sink.take())
        };
        if let End::Lost(reason) = end {
            warn!("cast connection lost: {reason}");
            if let Some(sink) = sink {
                sink.lost(RendererError::Connection(reason.clone()));
            }
        }
        for (_, waiting) in waiting {
            let _gone = waiting.reply.send(Err(ended_error(end)));
        }
    });
    (state, handler, on_end)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
