//! Session command-flow and status-reporting tests against a fake channel — the
//! session drives any [`RendererChannel`], so this exercises it without a real
//! Cast or UPnP device.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::channel::{fake_status as status, FakeChannel, RendererError, RendererPlayerState};
use super::session::{EventCallback, RendererSession, RendererSessionEvent};
use super::RendererMedia;

/// Collects the events the session reports, and lets a test wait for one that
/// matches a predicate.
#[derive(Clone)]
struct StatusSink {
    reported: Arc<Mutex<Vec<RendererSessionEvent>>>,
}

impl StatusSink {
    fn new() -> Self {
        Self {
            reported: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn callback(&self) -> EventCallback {
        let reported = self.reported.clone();
        Arc::new(move |s| reported.lock().unwrap().push(s))
    }

    fn wait_for(&self, predicate: impl Fn(&RendererSessionEvent) -> bool) -> bool {
        self.wait_until(|events| events.iter().any(&predicate))
    }

    /// Wait until the events reported so far satisfy `condition`.
    fn wait_until(&self, condition: impl Fn(&[RendererSessionEvent]) -> bool) -> bool {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if condition(&self.reported.lock().unwrap()) {
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
    let saw_playing = sink.wait_for(|event| {
        matches!(event, RendererSessionEvent::Status(s) if s.player_state == RendererPlayerState::Playing)
    });
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
    let ended = sink.wait_for(|event| {
        *event
            == RendererSessionEvent::Ended(RendererError::Connection(
                "renderer stopped".to_string(),
            ))
    });
    drop(session);

    assert!(
        ended,
        "a lost connection must end the session with why it was lost"
    );
}

/// A load the renderer refuses is reported with why, and the session goes on
/// polling rather than ending.
#[test]
fn session_reports_a_refused_command_and_continues() {
    let channel = FakeChannel::new();
    let refusal = RendererError::Command("renderer returned 500 to a SOAP action".to_string());
    channel.state.lock().unwrap().refuse_next_load = Some(refusal.clone());
    let sink = StatusSink::new();

    let session = RendererSession::start(Box::new(channel), sink.callback());
    session.load(media("http://host/track"));
    // The refusal, then a status polled after it.
    let refused_then_polled = sink.wait_until(|events| {
        events
            .iter()
            .position(|event| *event == RendererSessionEvent::CommandFailed(refusal.clone()))
            .is_some_and(|at| {
                events[at + 1..]
                    .iter()
                    .any(|event| matches!(event, RendererSessionEvent::Status(_)))
            })
    });
    drop(session);

    assert!(
        refused_then_polled,
        "a refused load must be reported with its cause, and polling must go on"
    );
    assert!(!sink
        .reported
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, RendererSessionEvent::Ended(_))));
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
    let saw_finished = sink.wait_for(|event| {
        matches!(event, RendererSessionEvent::Status(s) if s.player_state == RendererPlayerState::Finished)
    });
    drop(session);

    assert!(
        saw_finished,
        "a renderer end-of-media status must surface as Finished for queue advance"
    );
}
