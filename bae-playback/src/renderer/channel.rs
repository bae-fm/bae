//! The command wire to one connected remote renderer.
//!
//! A *remote renderer* is a device that fetches a track's audio over HTTP itself
//! and is driven by transport commands — a Cast receiver or a UPnP MediaRenderer.
//! [`RendererChannel`] is the protocol-agnostic seam the session drives: connect
//! once, then LOAD/PLAY/PAUSE/SEEK/volume/STOP and read status. The two real
//! implementations — Cast over CASTV2 and UPnP over SOAP, both in bae-cast —
//! differ only in their wire; the session, poll loop, and everything above them
//! are shared. Tests supply a fake implementation.
//!
//! This trait is specifically the "the device fetches a URL and is driven by
//! commands" shape. A push-audio renderer like AirPlay — where bae keeps
//! decoding and streams the audio to the receiver, which never fetches a URL — is
//! a different renderer flavor and is deliberately outside this trait; it belongs
//! beside `Renderer::Remote` as its own variant, not as a third channel here.

use std::time::Duration;

/// The media a LOAD hands the renderer: the URL it fetches over HTTP and the
/// metadata it shows on-screen. Both a Cast LOAD and a UPnP `SetAVTransportURI`
/// are built from these fields.
#[derive(Debug, Clone, PartialEq)]
pub struct RendererMedia {
    /// The HTTP URL the renderer fetches the audio from.
    pub url: String,
    /// The MIME type of `url`'s bytes (`audio/flac`, `audio/mpeg`, …).
    pub content_type: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// An HTTP URL to cover art, shown on renderers with a screen. `None` when
    /// the track has no cover.
    pub cover_url: Option<String>,
    /// How long the served stream runs, when known: the track's pregap, then
    /// the track. Device positions count in the same stream.
    pub duration: Option<Duration>,
}

/// What the renderer's player is doing, read from its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererPlayerState {
    /// No media is playing for a non-terminal reason — the player is idle before
    /// the first LOAD, or between a LOAD and its buffering, or a load was
    /// cancelled/interrupted. Not a queue-advance signal.
    Idle,
    /// The loaded media reached its natural end. This is the queue-advance
    /// signal (Cast IDLE/FINISHED, or a UPnP transport that went STOPPED after
    /// playing through to the end).
    Finished,
    /// The player is filling its buffer; position is not advancing yet.
    Buffering,
    Playing,
    Paused,
}

/// A snapshot of the renderer's state from one `poll_status`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReceiverStatus {
    pub player_state: RendererPlayerState,
    /// The renderer's playback position into the current media, when reported.
    pub position: Option<Duration>,
    /// The current media's duration, when the renderer reports it.
    pub duration: Option<Duration>,
    /// Renderer volume level (0.0–1.0), or `None` when it wasn't read — carried
    /// as absent rather than invented, so a UI slider isn't jumped to a
    /// fabricated level.
    pub volume: Option<f32>,
    /// How fast the position moves while playing, as a multiple of real time.
    /// `None` when the renderer stated none that can be read (finite and above
    /// zero): the position then holds until the next status.
    pub rate: Option<f64>,
}

impl ReceiverStatus {
    /// Where the renderer's position has reached at `now`, given this status
    /// arrived at `observed_at`: the stated position, plus the time since
    /// times the stated rate while playing, held to the media's duration when
    /// the renderer stated one. A status that isn't playing, or states no
    /// rate, keeps its stated position. `None` when it stated none.
    pub fn position_at(
        &self,
        observed_at: chrono::DateTime<chrono::Utc>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<Duration> {
        let stated = self.position?;
        let (RendererPlayerState::Playing, Some(rate)) = (self.player_state, self.rate) else {
            return Some(stated);
        };
        let elapsed = (now - observed_at).to_std().unwrap_or(Duration::ZERO);
        let moved =
            Duration::try_from_secs_f64(elapsed.as_secs_f64() * rate).unwrap_or(Duration::ZERO);
        let reached = stated + moved;
        Some(self.duration.map_or(reached, |duration| reached.min(duration.max(stated))))
    }
}

/// A failure talking to the renderer. `Connection` is terminal — it means the
/// link to the device is gone (the Cast app was stopped, the UPnP renderer went
/// unreachable, the network dropped), which the session reads as the remote
/// session ending.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RendererError {
    /// The link could not be established or was lost.
    #[error("renderer connection failed: {0}")]
    Connection(String),
    /// Preparing the renderer to receive media failed (Cast app launch).
    #[error("renderer launch failed: {0}")]
    Launch(String),
    /// A media/transport command (load, play, seek, …) failed.
    #[error("renderer command failed: {0}")]
    Command(String),
    /// The session's own thread panicked: a bug in bae, reported where it
    /// was caught.
    #[error("{0}")]
    Panicked(bae_util::task_failure::Bug),
}

/// The operations the session performs against a connected renderer. Each call
/// blocks on the wire, so the session drives it from its own thread. A
/// [`RendererError::Connection`] from any call is terminal (the session ends);
/// tests fake this to record commands and hand back canned statuses.
pub trait RendererChannel: Send {
    /// Load `media` on the renderer and start it playing.
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError>;
    fn play(&mut self) -> Result<(), RendererError>;
    fn pause(&mut self) -> Result<(), RendererError>;
    fn seek(&mut self, position: Duration) -> Result<(), RendererError>;
    /// Set the renderer volume to `level`.
    fn set_volume(&mut self, level: crate::Volume) -> Result<(), RendererError>;
    /// Stop playback on the renderer (the loaded media is unloaded).
    fn stop(&mut self) -> Result<(), RendererError>;
    /// Read the renderer's current status.
    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError>;
}

/// A scriptable fake channel, shared by every test that needs one: it records
/// the commands the session issued and hands back queued — or default — status
/// on each poll. Held behind an `Arc<Mutex<_>>` because the session moves the
/// channel onto its own thread while the test keeps reading what arrived.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct FakeChannelState {
    pub(crate) loads: Vec<RendererMedia>,
    pub(crate) plays: u32,
    pub(crate) pauses: u32,
    pub(crate) seeks: Vec<Duration>,
    pub(crate) volumes: Vec<f32>,
    pub(crate) stops: u32,
    /// Status responses returned by successive polls; once drained,
    /// `default_status` is returned. A `Connection` error ends the session.
    pub(crate) poll_script: std::collections::VecDeque<Result<ReceiverStatus, RendererError>>,
    /// What a poll returns once the script is drained. `None` means playing.
    pub(crate) default_status: Option<ReceiverStatus>,
    /// Sent each load as it arrives, for a test that waits on one.
    pub(crate) load_events: Option<tokio::sync::mpsc::UnboundedSender<RendererMedia>>,
    /// What the next load is refused with, once.
    pub(crate) refuse_next_load: Option<RendererError>,
    /// Woken by every command and poll the channel takes, for
    /// [`wait_for_state`].
    pub(crate) changed: std::sync::Arc<std::sync::Condvar>,
}

/// Block until `predicate` holds of the channel's state, which every command
/// the session applies wakes a check of. `false` when the hang guard passes
/// first.
#[cfg(test)]
pub(crate) fn wait_for_state(
    state: &std::sync::Mutex<FakeChannelState>,
    predicate: impl Fn(&FakeChannelState) -> bool,
) -> bool {
    let guard = state.lock().unwrap();
    let changed = guard.changed.clone();
    let (_guard, result) = changed
        .wait_timeout_while(guard, bae_util::test_channels::HANG_GUARD, |state| !predicate(state))
        .unwrap();
    !result.timed_out()
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct FakeChannel {
    pub(crate) state: std::sync::Arc<std::sync::Mutex<FakeChannelState>>,
}

#[cfg(test)]
impl FakeChannel {
    pub(crate) fn new() -> Self {
        Self { state: std::sync::Arc::new(std::sync::Mutex::new(FakeChannelState::default())) }
    }
}

/// A status carrying `player_state` and nothing a test has to care about.
#[cfg(test)]
pub(crate) fn fake_status(player_state: RendererPlayerState) -> ReceiverStatus {
    ReceiverStatus { player_state, position: None, duration: None, volume: Some(1.0), rate: None }
}

#[cfg(test)]
impl RendererChannel for FakeChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        let mut state = self.state.lock().unwrap();
        if let Some(refusal) = state.refuse_next_load.take() {
            return Err(refusal);
        }
        state.loads.push(media.clone());
        state.changed.notify_all();
        if let Some(load_events) = &state.load_events {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a test that dropped its receiver no longer waits on loads"
            )]
            let _ = load_events.send(media.clone());
        }
        Ok(())
    }
    fn play(&mut self) -> Result<(), RendererError> {
        {
            let mut state = self.state.lock().unwrap();
            state.plays += 1;
            state.changed.notify_all();
        }
        Ok(())
    }
    fn pause(&mut self) -> Result<(), RendererError> {
        {
            let mut state = self.state.lock().unwrap();
            state.pauses += 1;
            state.changed.notify_all();
        }
        Ok(())
    }
    fn seek(&mut self, position: Duration) -> Result<(), RendererError> {
        {
            let mut state = self.state.lock().unwrap();
            state.seeks.push(position);
            state.changed.notify_all();
        }
        Ok(())
    }
    fn set_volume(&mut self, level: crate::Volume) -> Result<(), RendererError> {
        {
            let mut state = self.state.lock().unwrap();
            state.volumes.push(level.get());
            state.changed.notify_all();
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), RendererError> {
        {
            let mut state = self.state.lock().unwrap();
            state.stops += 1;
            state.changed.notify_all();
        }
        Ok(())
    }
    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError> {
        let mut state = self.state.lock().unwrap();
        if let Some(scripted) = state.poll_script.pop_front() {
            return scripted;
        }
        Ok(state.default_status.unwrap_or_else(|| fake_status(RendererPlayerState::Playing)))
    }
}
