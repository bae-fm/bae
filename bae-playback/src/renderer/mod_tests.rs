//! Session command-flow and status-reporting tests against a fake channel — the
//! session drives any [`RendererLink`], so this exercises it without a real
//! Cast or UPnP device.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::channel::{
    fake_status as status, wait_for_state, FakeChannel, ReceiverStatus, RendererError,
    RendererLink, RendererPlayerState,
};
use super::session::{EventCallback, RendererSession, RendererSessionEvent, SessionId, StatusSink};
use super::RendererMedia;

/// Collects the events the session reports, and lets a test wait for one that
/// matches a predicate.
/// A clock for a session whose statuses a test reads no time from.
fn test_clock() -> crate::PlaybackClockRef {
    Arc::new(crate::ManualPlaybackClock::new(chrono::DateTime::UNIX_EPOCH))
}

#[derive(Clone)]
struct Events {
    reported: Arc<(Mutex<Vec<RendererSessionEvent>>, Condvar)>,
}

impl Events {
    fn new() -> Self {
        Self { reported: Arc::new((Mutex::new(Vec::new()), Condvar::new())) }
    }

    fn callback(&self) -> EventCallback {
        let reported = self.reported.clone();
        Arc::new(move |s| {
            reported.0.lock().unwrap().push(s);
            reported.1.notify_all();
        })
    }

    fn wait_for(&self, predicate: impl Fn(&RendererSessionEvent) -> bool) -> bool {
        self.wait_until(|events| events.iter().any(&predicate))
    }

    /// Wait until the events reported so far satisfy `condition`; `false`
    /// when the hang guard passes first.
    fn wait_until(&self, condition: impl Fn(&[RendererSessionEvent]) -> bool) -> bool {
        let (events, reported) = &*self.reported;
        let (_events, result) = reported
            .wait_timeout_while(
                events.lock().unwrap(),
                bae_util::test_channels::HANG_GUARD,
                |events| !condition(events),
            )
            .unwrap();
        !result.timed_out()
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
    let sink = Events::new();

    let session = RendererSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        channel.polled(),
        test_clock(),
        SessionId(0),
        sink.callback(),
    )
    .expect("start the session thread");
    session.load(media("http://host/track"));
    session.pause();
    session.seek(Duration::from_secs(42));
    session.set_volume(crate::Volume::new(0.3).unwrap());

    // Dropping the session joins its thread after the commands queued ahead
    // of the drop, so each has been applied once it returns.
    drop(session);
    let applied = {
        let s = state.lock().unwrap();
        s.loads.len() == 1 && s.pauses == 1 && s.seeks.len() == 1 && s.volumes.len() == 1
    };

    assert!(applied, "the session must route every command to the channel");
    let s = state.lock().unwrap();
    assert_eq!(s.loads[0].url, "http://host/track");
    assert_eq!(s.seeks[0], Duration::from_secs(42));
    assert_eq!(s.volumes[0], 0.3);
}

#[test]
fn session_reports_polled_status_to_the_callback() {
    let channel = FakeChannel::new();
    channel.state.lock().unwrap().default_status = Some(status(RendererPlayerState::Playing));
    let sink = Events::new();

    let session = RendererSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        channel.polled(),
        test_clock(),
        SessionId(0),
        sink.callback(),
    )
    .expect("start the session thread");
    let saw_playing = sink.wait_for(|event| {
        matches!(event, RendererSessionEvent::Status { status: s, .. } if s.player_state == RendererPlayerState::Playing)
    });
    drop(session);

    assert!(saw_playing, "polled renderer status must flow to the status callback");
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
        .push_back(Err(RendererError::Connection("renderer stopped".to_string())));
    let sink = Events::new();

    let session = RendererSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        channel.polled(),
        test_clock(),
        SessionId(0),
        sink.callback(),
    )
    .expect("start the session thread");
    let ended = sink.wait_for(|event| {
        *event
            == RendererSessionEvent::Ended {
                session: SessionId(0),
                error: RendererError::Connection("renderer stopped".to_string()),
            }
    });
    drop(session);

    assert!(ended, "a lost connection must end the session with why it was lost");
}

/// A load the renderer refuses is reported with why, and the session goes on
/// polling rather than ending.
#[test]
fn session_reports_a_refused_command_and_continues() {
    let channel = FakeChannel::new();
    let refusal = RendererError::Command("renderer returned 500 to a SOAP action".to_string());
    channel.state.lock().unwrap().refuse_next_load = Some(refusal.clone());
    let sink = Events::new();

    let session = RendererSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        channel.polled(),
        test_clock(),
        SessionId(0),
        sink.callback(),
    )
    .expect("start the session thread");
    session.load(media("http://host/track"));
    // The refusal, then a status polled after it.
    let refused_then_polled = sink.wait_until(|events| {
        events
            .iter()
            .position(|event| {
                *event
                    == RendererSessionEvent::CommandFailed {
                        session: SessionId(0),
                        error: refusal.clone(),
                    }
            })
            .is_some_and(|at| {
                events[at + 1..]
                    .iter()
                    .any(|event| matches!(event, RendererSessionEvent::Status { .. }))
            })
    });
    drop(session);

    assert!(
        refused_then_polled,
        "a refused load must be reported with its cause, and polling must go on"
    );
    assert!(!sink
        .reported
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, RendererSessionEvent::Ended { .. })));
}

#[test]
fn session_reports_finished_for_queue_advance() {
    let channel = FakeChannel::new();
    channel.state.lock().unwrap().poll_script.push_back(Ok(status(RendererPlayerState::Finished)));
    let sink = Events::new();

    let session = RendererSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        channel.polled(),
        test_clock(),
        SessionId(0),
        sink.callback(),
    )
    .expect("start the session thread");
    let saw_finished = sink.wait_for(|event| {
        matches!(event, RendererSessionEvent::Status { status: s, .. } if s.player_state == RendererPlayerState::Finished)
    });
    drop(session);

    assert!(
        saw_finished,
        "a renderer end-of-media status must surface as Finished for queue advance"
    );
}

/// A clock the test moves, so a status's stamp says which instant it was said
/// at.
fn manual_clock() -> (Arc<crate::ManualPlaybackClock>, crate::PlaybackClockRef) {
    let clock = Arc::new(crate::ManualPlaybackClock::new(chrono::DateTime::UNIX_EPOCH));
    (clock.clone(), clock)
}

fn session_over(
    link: RendererLink,
    clock: crate::PlaybackClockRef,
    id: SessionId,
    events: &Events,
) -> RendererSession {
    RendererSession::start(
        &bae_diagnostics::Diagnostics::noop(),
        link,
        clock,
        id,
        events.callback(),
    )
    .expect("start the session thread")
}

/// The sink a pushed session handed its renderer.
fn handed_sink(channel: &FakeChannel) -> StatusSink {
    assert!(
        wait_for_state(&channel.state, |state| state.sink.is_some()),
        "a pushed session hands its renderer a sink at its start"
    );
    channel.state.lock().unwrap().sink.clone().expect("the sink was just seen")
}

#[test]
fn a_pushed_status_reaches_the_callback_stamped_by_the_clock_and_nothing_polls() {
    let channel = FakeChannel::new();
    let events = Events::new();
    let (manual, clock) = manual_clock();
    let session = session_over(channel.pushed(), clock, SessionId(3), &events);
    let sink = handed_sink(&channel);

    manual.advance(Duration::from_secs(7));
    sink.report(status(RendererPlayerState::Paused));
    let heard = events.wait_for(|event| {
        *event
            == RendererSessionEvent::Status {
                session: SessionId(3),
                status: status(RendererPlayerState::Paused),
                observed_at: after(7),
            }
    });
    // A command's round trip through the thread: every poll the session
    // would have made first has been made by the time it is applied.
    session.pause();
    drop(session);

    assert!(heard, "a pushed status reaches the callback with the clock's time");
    assert_eq!(channel.state.lock().unwrap().polls, 0, "a pushed renderer is never polled");
    assert_eq!(events.reported.0.lock().unwrap().len(), 1, "nothing but the pushed status");
}

#[test]
fn a_loss_the_renderer_reports_and_a_command_connection_error_end_the_session_once() {
    let channel = FakeChannel::new();
    let events = Events::new();
    let session = session_over(channel.pushed(), test_clock(), SessionId(0), &events);
    let sink = handed_sink(&channel);
    let lost = RendererError::Connection("the receiver went away".to_string());

    sink.lost(lost.clone());
    channel.state.lock().unwrap().refuse_next_load = Some(lost.clone());
    session.load(media("http://host/track"));
    session.pause();
    drop(session);

    let reported = events.reported.0.lock().unwrap();
    assert_eq!(
        *reported,
        vec![RendererSessionEvent::Ended { session: SessionId(0), error: lost }],
        "the renderer's own report is the only end"
    );
}

#[test]
fn closing_a_session_sends_no_end() {
    for link in [FakeChannel::pushed, FakeChannel::polled] {
        let channel = FakeChannel::new();
        let events = Events::new();
        let session = session_over(link(&channel), test_clock(), SessionId(0), &events);
        session.load(media("http://host/track"));
        drop(session);

        assert!(
            !events
                .reported
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, RendererSessionEvent::Ended { .. })),
            "dropping the session is not the link being lost"
        );
        assert_eq!(channel.state.lock().unwrap().stops, 1, "the renderer is told to stop");
    }
}

fn position_status(
    player_state: RendererPlayerState,
    position: Option<u64>,
    rate: Option<f64>,
) -> ReceiverStatus {
    ReceiverStatus {
        player_state,
        position: position.map(Duration::from_secs),
        duration: None,
        volume: None,
        rate,
    }
}

fn after(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::UNIX_EPOCH + chrono::Duration::seconds(seconds)
}

#[test]
fn a_playing_position_moves_on_at_the_stated_rate() {
    let status = position_status(RendererPlayerState::Playing, Some(30), Some(1.0));
    assert_eq!(status.position_at(after(0), after(0)), Some(Duration::from_secs(30)));
    assert_eq!(status.position_at(after(0), after(4)), Some(Duration::from_secs(34)));
    let slow = position_status(RendererPlayerState::Playing, Some(30), Some(0.5));
    assert_eq!(slow.position_at(after(0), after(4)), Some(Duration::from_secs(32)));
}

#[test]
fn a_position_that_is_not_playing_or_has_no_rate_holds() {
    for state in [
        RendererPlayerState::Paused,
        RendererPlayerState::Buffering,
        RendererPlayerState::Idle,
        RendererPlayerState::Finished,
    ] {
        let status = position_status(state, Some(30), Some(1.0));
        assert_eq!(
            status.position_at(after(0), after(9)),
            Some(Duration::from_secs(30)),
            "{state:?}"
        );
    }
    let unrated = position_status(RendererPlayerState::Playing, Some(30), None);
    assert_eq!(unrated.position_at(after(0), after(9)), Some(Duration::from_secs(30)));
}

#[test]
fn no_stated_position_is_no_position_whatever_the_time() {
    let status = position_status(RendererPlayerState::Playing, None, Some(1.0));
    assert_eq!(status.position_at(after(0), after(9)), None);
}

#[test]
fn a_position_does_not_run_back_or_past_the_media() {
    let mut status = position_status(RendererPlayerState::Playing, Some(30), Some(1.0));
    assert_eq!(status.position_at(after(9), after(0)), Some(Duration::from_secs(30)));
    status.duration = Some(Duration::from_secs(33));
    assert_eq!(status.position_at(after(0), after(9)), Some(Duration::from_secs(33)));
    status.duration = Some(Duration::from_secs(10));
    assert_eq!(
        status.position_at(after(0), after(9)),
        Some(Duration::from_secs(30)),
        "a duration shorter than the stated position doesn't pull it back"
    );
}
