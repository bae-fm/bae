//! Session command-flow and status-reporting tests against a fake channel — the
//! session drives any [`RendererChannel`], so this exercises it without a real
//! Cast or UPnP device.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::channel::{
    ReceiverStatus, RendererChannel, RendererError, RendererMedia, RendererPlayerState,
};
use super::session::{RendererSession, RendererSessionStatus, StatusCallback};

/// A scriptable fake channel: records the commands the session issues and hands
/// back queued (or a default) status on each poll. Shared with the test thread
/// through an `Arc<Mutex<_>>` because the session moves the channel onto its own
/// thread.
#[derive(Default)]
struct FakeState {
    loads: Vec<RendererMedia>,
    plays: u32,
    pauses: u32,
    seeks: Vec<Duration>,
    volumes: Vec<f32>,
    stops: u32,
    /// Status responses returned by successive polls; once drained, `default`
    /// is returned. A `Connection` error ends the session.
    poll_script: VecDeque<Result<ReceiverStatus, RendererError>>,
    default_status: Option<ReceiverStatus>,
}

#[derive(Clone)]
struct FakeChannel {
    state: Arc<Mutex<FakeState>>,
}

impl FakeChannel {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState::default())),
        }
    }
}

fn status(player_state: RendererPlayerState) -> ReceiverStatus {
    ReceiverStatus {
        player_state,
        position: None,
        duration: None,
        volume: Some(1.0),
    }
}

impl RendererChannel for FakeChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        self.state.lock().unwrap().loads.push(media.clone());
        Ok(())
    }
    fn play(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().plays += 1;
        Ok(())
    }
    fn pause(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().pauses += 1;
        Ok(())
    }
    fn seek(&mut self, position: Duration) -> Result<(), RendererError> {
        self.state.lock().unwrap().seeks.push(position);
        Ok(())
    }
    fn set_volume(&mut self, level: f32) -> Result<(), RendererError> {
        self.state.lock().unwrap().volumes.push(level);
        Ok(())
    }
    fn stop(&mut self) -> Result<(), RendererError> {
        self.state.lock().unwrap().stops += 1;
        Ok(())
    }
    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError> {
        let mut state = self.state.lock().unwrap();
        if let Some(scripted) = state.poll_script.pop_front() {
            return scripted;
        }
        Ok(state
            .default_status
            .unwrap_or_else(|| status(RendererPlayerState::Playing)))
    }
}

/// Collects the statuses the session reports, and lets a test wait for one that
/// matches a predicate.
#[derive(Clone)]
struct StatusSink {
    reported: Arc<Mutex<Vec<RendererSessionStatus>>>,
}

impl StatusSink {
    fn new() -> Self {
        Self {
            reported: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn callback(&self) -> StatusCallback {
        let reported = self.reported.clone();
        Arc::new(move |s| reported.lock().unwrap().push(s))
    }

    fn wait_for(&self, predicate: impl Fn(&RendererSessionStatus) -> bool) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if self.reported.lock().unwrap().iter().any(&predicate) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

fn media(url: &str) -> RendererMedia {
    RendererMedia {
        url: url.to_string(),
        content_type: "audio/flac".to_string(),
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        cover_url: None,
        duration: Some(Duration::from_secs(180)),
    }
}

#[test]
fn session_routes_commands_to_the_channel() {
    let channel = FakeChannel::new();
    let state = channel.state.clone();
    let sink = StatusSink::new();

    let session = RendererSession::start(Box::new(channel), sink.callback());
    session.load(media("http://host/track"));
    session.pause();
    session.seek(Duration::from_secs(42));
    session.set_volume(0.3);

    // Wait until every command has been applied by the session thread.
    let applied = {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            {
                let s = state.lock().unwrap();
                if s.loads.len() == 1 && s.pauses == 1 && s.seeks.len() == 1 && s.volumes.len() == 1
                {
                    break true;
                }
            }
            if std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    drop(session);

    assert!(
        applied,
        "the session must route every command to the channel"
    );
    let s = state.lock().unwrap();
    assert_eq!(s.loads[0].url, "http://host/track");
    assert_eq!(s.seeks[0], Duration::from_secs(42));
    assert_eq!(s.volumes[0], 0.3);
}

#[test]
fn session_reports_polled_status_to_the_callback() {
    let channel = FakeChannel::new();
    channel.state.lock().unwrap().default_status = Some(status(RendererPlayerState::Playing));
    let sink = StatusSink::new();

    let session = RendererSession::start(Box::new(channel), sink.callback());
    let saw_playing = sink.wait_for(|s| s.player_state == RendererPlayerState::Playing && !s.ended);
    drop(session);

    assert!(
        saw_playing,
        "polled renderer status must flow to the status callback"
    );
}

#[test]
fn session_ends_on_receiver_side_stop() {
    let channel = FakeChannel::new();
    // The renderer drops the connection (user stopped from the device): the next
    // poll returns a connection error.
    channel
        .state
        .lock()
        .unwrap()
        .poll_script
        .push_back(Err(RendererError::Connection(
            "renderer stopped".to_string(),
        )));
    let sink = StatusSink::new();

    let session = RendererSession::start(Box::new(channel), sink.callback());
    let ended = sink.wait_for(|s| s.ended);
    drop(session);

    assert!(
        ended,
        "a lost connection must end the session with a terminal status"
    );
}

#[test]
fn session_reports_finished_for_queue_advance() {
    let channel = FakeChannel::new();
    channel
        .state
        .lock()
        .unwrap()
        .poll_script
        .push_back(Ok(status(RendererPlayerState::Finished)));
    let sink = StatusSink::new();

    let session = RendererSession::start(Box::new(channel), sink.callback());
    let saw_finished =
        sink.wait_for(|s| s.player_state == RendererPlayerState::Finished && !s.ended);
    drop(session);

    assert!(
        saw_finished,
        "a renderer end-of-media status must surface as Finished for queue advance"
    );
}
