use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::channel::{CastChannel, CastError, CastMedia, CastPlayerState, ReceiverStatus};
use super::discovery::map_device;
use super::session::{CastSession, CastSessionStatus};

// -- discovery: record → device mapping --------------------------------------

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

#[test]
fn resolved_service_maps_to_device() {
    let device = map_device(
        "Living Room._googlecast._tcp.local.",
        8009,
        [v4(192, 168, 1, 40)].into_iter(),
        Some("abcd1234"),
        Some("Living Room Speaker"),
    )
    .expect("a resolved cast service maps to a device");

    assert_eq!(device.id, "abcd1234");
    assert_eq!(device.name, "Living Room Speaker");
    assert_eq!(device.addr, v4(192, 168, 1, 40));
    assert_eq!(device.port, 8009);
}

#[test]
fn resolved_service_without_id_is_unusable() {
    assert!(
        map_device(
            "No Id._googlecast._tcp.local.",
            8009,
            [v4(192, 168, 1, 41)].into_iter(),
            None,
            Some("No Id"),
        )
        .is_none(),
        "a service with no device id can't be routed to, so it is dropped"
    );
}

#[test]
fn resolved_service_falls_back_to_instance_name_without_fn() {
    let device = map_device(
        "Kitchen._googlecast._tcp.local.",
        8009,
        [v4(192, 168, 1, 42)].into_iter(),
        Some("kitchen-id"),
        None,
    )
    .expect("maps with a fallback name");
    assert_eq!(device.name, "Kitchen");
}

#[test]
fn resolved_service_prefers_ipv4() {
    let device = map_device(
        "Dual._googlecast._tcp.local.",
        8009,
        [IpAddr::V6(Ipv6Addr::LOCALHOST), v4(192, 168, 1, 43)].into_iter(),
        Some("dual-id"),
        Some("Dual Stack"),
    )
    .expect("maps");
    assert_eq!(
        device.addr,
        v4(192, 168, 1, 43),
        "an IPv4 address is preferred over IPv6 for the LAN media fetch"
    );
}

#[test]
fn resolved_service_without_address_is_unusable() {
    assert!(
        map_device(
            "Addrless._googlecast._tcp.local.",
            8009,
            std::iter::empty(),
            Some("addrless-id"),
            Some("Addrless"),
        )
        .is_none(),
        "a service with no address can't be reached, so it is dropped"
    );
}

// -- session: fake channel command flow + status → callback ------------------

/// A scriptable fake channel: records the commands the session issues and hands
/// back queued (or a default) status on each poll. Shared with the test thread
/// through an `Arc<Mutex<_>>` because the session moves the channel onto its own
/// thread.
#[derive(Default)]
struct FakeState {
    loads: Vec<CastMedia>,
    plays: u32,
    pauses: u32,
    seeks: Vec<Duration>,
    volumes: Vec<f32>,
    stops: u32,
    /// Status responses returned by successive polls; once drained, `default`
    /// is returned. A `Connection` error ends the session.
    poll_script: VecDeque<Result<ReceiverStatus, CastError>>,
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

fn status(player_state: CastPlayerState) -> ReceiverStatus {
    ReceiverStatus {
        player_state,
        position: None,
        duration: None,
        volume: 1.0,
    }
}

impl CastChannel for FakeChannel {
    fn load(&mut self, media: &CastMedia) -> Result<(), CastError> {
        self.state.lock().unwrap().loads.push(media.clone());
        Ok(())
    }
    fn play(&mut self) -> Result<(), CastError> {
        self.state.lock().unwrap().plays += 1;
        Ok(())
    }
    fn pause(&mut self) -> Result<(), CastError> {
        self.state.lock().unwrap().pauses += 1;
        Ok(())
    }
    fn seek(&mut self, position: Duration) -> Result<(), CastError> {
        self.state.lock().unwrap().seeks.push(position);
        Ok(())
    }
    fn set_volume(&mut self, level: f32) -> Result<(), CastError> {
        self.state.lock().unwrap().volumes.push(level);
        Ok(())
    }
    fn stop(&mut self) -> Result<(), CastError> {
        self.state.lock().unwrap().stops += 1;
        Ok(())
    }
    fn poll_status(&mut self) -> Result<ReceiverStatus, CastError> {
        let mut state = self.state.lock().unwrap();
        if let Some(scripted) = state.poll_script.pop_front() {
            return scripted;
        }
        Ok(state
            .default_status
            .unwrap_or_else(|| status(CastPlayerState::Playing)))
    }
}

/// Collects the statuses the session reports, and lets a test wait for one that
/// matches a predicate.
#[derive(Clone)]
struct StatusSink {
    reported: Arc<Mutex<Vec<CastSessionStatus>>>,
}

impl StatusSink {
    fn new() -> Self {
        Self {
            reported: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn callback(&self) -> super::session::StatusCallback {
        let reported = self.reported.clone();
        Arc::new(move |s| reported.lock().unwrap().push(s))
    }

    fn wait_for(&self, predicate: impl Fn(&CastSessionStatus) -> bool) -> bool {
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

fn media(url: &str) -> CastMedia {
    CastMedia {
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

    let session = CastSession::start(Box::new(channel), sink.callback());
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
    channel.state.lock().unwrap().default_status = Some(status(CastPlayerState::Playing));
    let sink = StatusSink::new();

    let session = CastSession::start(Box::new(channel), sink.callback());
    let saw_playing = sink.wait_for(|s| s.player_state == CastPlayerState::Playing && !s.ended);
    drop(session);

    assert!(
        saw_playing,
        "polled receiver status must flow to the status callback"
    );
}

#[test]
fn session_ends_on_receiver_side_stop() {
    let channel = FakeChannel::new();
    // The receiver drops the connection (user stopped casting from the device):
    // the next poll returns a connection error.
    channel
        .state
        .lock()
        .unwrap()
        .poll_script
        .push_back(Err(CastError::Connection("receiver stopped".to_string())));
    let sink = StatusSink::new();

    let session = CastSession::start(Box::new(channel), sink.callback());
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
        .push_back(Ok(status(CastPlayerState::Finished)));
    let sink = StatusSink::new();

    let session = CastSession::start(Box::new(channel), sink.callback());
    let saw_finished = sink.wait_for(|s| s.player_state == CastPlayerState::Finished && !s.ended);
    drop(session);

    assert!(
        saw_finished,
        "a receiver IDLE(finished) status must surface as Finished for queue advance"
    );
}
