use bae_util::test_channels::HANG_GUARD;
use serde_json::{json, Value};

use super::super::fake_receiver::{FakeReceiver, Received, Reply, Script};
use super::super::messages::{
    NAMESPACE_CONNECTION, NAMESPACE_HEARTBEAT, NAMESPACE_MEDIA, NAMESPACE_RECEIVER,
};
use super::*;
use bae_playback::renderer::{RendererSessionEvent, SessionId};

const SHORT: Deadlines =
    Deadlines { launch: Duration::from_secs(10), command: Duration::from_secs(10) };

fn media() -> RendererMedia {
    RendererMedia {
        url: "http://192.0.2.10:4000/stream/track-1".to_string(),
        content_type: "audio/flac".to_string(),
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        cover_url: None,
        duration: Some(Duration::from_millis(215_500)),
    }
}

async fn client_over_a_fake_receiver() -> (CastClient, FakeReceiver) {
    let (client_end, fake_end) = tokio::io::duplex(64 * 1024);
    let fake = FakeReceiver::serve_receiver(fake_end);
    let client = connect_to(client_end, SHORT).await.expect("the launch is answered");
    (client, fake)
}

async fn connect_to(
    stream: tokio::io::DuplexStream,
    deadlines: Deadlines,
) -> Result<CastClient, RendererError> {
    CastClient::connect_over(
        stream,
        &Diagnostics::noop(),
        &tokio::runtime::Handle::current(),
        deadlines,
    )
    .await
}

async fn failed_connect(fake_script: Script) -> RendererError {
    let (client_end, fake_end) = tokio::io::duplex(64 * 1024);
    let fake = FakeReceiver::serve_receiver(fake_end);
    fake.answer_next("LAUNCH", fake_script);
    match connect_to(client_end, SHORT).await {
        Ok(_) => panic!("the launch should have failed"),
        Err(error) => error,
    }
}

/// The client's end, waited for no longer than a hung test is given.
async fn ended(client: &CastClient) -> End {
    tokio::time::timeout(HANG_GUARD, client.ended()).await.expect("the connection ended in time")
}

/// Return once the client has taken in everything the receiver sent before
/// this call: a PING sent now is answered after the earlier messages are read.
async fn heard_everything(fake: &mut FakeReceiver) {
    requests(fake).await;
}

/// The requests the receiver was sent since the last call, without the
/// heartbeat. A PING is sent and its PONG awaited first, so that everything the
/// client sent before this call has arrived.
async fn requests(fake: &mut FakeReceiver) -> Vec<Received> {
    fake.ping();
    let mut sent = Vec::new();
    loop {
        let received = fake.next_received().await;
        if received.namespace == NAMESPACE_HEARTBEAT {
            if received.kind() == "PONG" {
                return sent;
            }
        } else {
            sent.push(received);
        }
    }
}

#[tokio::test]
async fn connecting_connects_to_the_platform_launches_the_app_then_connects_to_its_transport() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    let sent = requests(&mut fake).await;
    let summary: Vec<(&str, &str)> =
        sent.iter().map(|r| (r.kind(), r.destination_id.as_str())).collect();
    assert_eq!(
        summary,
        [("CONNECT", "receiver-0"), ("LAUNCH", "receiver-0"), ("CONNECT", "transport-1")]
    );
    assert_eq!(sent[1].payload["appId"], "CC1AD845");
    assert_eq!(sent[1].source_id, "sender-0");
    assert!(client.end().is_none());
}

#[tokio::test]
async fn a_launch_answered_by_a_bare_receiver_status_connects() {
    let (client_end, fake_end) = tokio::io::duplex(64 * 1024);
    let fake = FakeReceiver::serve_receiver(fake_end);
    fake.answer_next("LAUNCH", Script::Replies(vec![Reply::ReceiverStatusWithApp]));
    connect_to(client_end, SHORT).await.expect("a bare RECEIVER_STATUS answers the launch");
}

#[tokio::test]
async fn a_launch_waiting_for_the_person_to_allow_it_connects_once_it_is_allowed() {
    let (client_end, fake_end) = tokio::io::duplex(64 * 1024);
    let fake = FakeReceiver::serve_receiver(fake_end);
    fake.answer_next(
        "LAUNCH",
        Script::Replies(vec![
            Reply::LaunchStatus("USER_PENDING_AUTHORIZATION"),
            Reply::LaunchStatus("USER_ALLOWED"),
            Reply::ReceiverStatusWithApp,
        ]),
    );
    connect_to(client_end, SHORT).await.expect("the launch is answered by the status");
}

#[tokio::test]
async fn a_launch_status_alone_does_not_answer_the_launch() {
    let (client_end, fake_end) = tokio::io::duplex(64 * 1024);
    let fake = FakeReceiver::serve_receiver(fake_end);
    fake.answer_next("LAUNCH", Script::Replies(vec![Reply::LaunchStatus("USER_ALLOWED")]));
    let deadlines = Deadlines { launch: Duration::from_millis(50), ..SHORT };
    let error = connect_to(client_end, deadlines).await.err().expect("no answer");
    assert!(matches!(error, RendererError::Launch(text) if text.contains("did not answer")));
}

#[tokio::test]
async fn a_launch_error_fails_the_connect_naming_the_reason_and_the_extended_error() {
    let error = failed_connect(Script::Replies(vec![Reply::LaunchError {
        reason: "NOT_ALLOWED",
        extended_error: Some("USER_NOT_ALLOWED"),
    }]))
    .await;
    let RendererError::Launch(text) = error else { panic!("not a launch error: {error:?}") };
    assert!(text.contains("NOT_ALLOWED") && text.contains("USER_NOT_ALLOWED"), "{text}");
}

#[tokio::test]
async fn a_launch_that_is_refused_fails_the_connect_as_a_launch_error() {
    let error = failed_connect(Script::Replies(vec![Reply::Raw {
        namespace: NAMESPACE_RECEIVER,
        payload: json!({"type": "INVALID_REQUEST", "reason": "INVALID_COMMAND"}),
    }]))
    .await;
    assert!(matches!(error, RendererError::Launch(text) if text.contains("INVALID_COMMAND")));
}

#[tokio::test]
async fn a_load_answered_by_a_media_status_with_its_request_id_takes_that_media_session() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    assert!(client.media_session().is_none());
    client.load(&media()).await.expect("the load is answered");

    let session = client.media_session().expect("bae's media session");
    assert_eq!(session.id, 1);
    assert_eq!(session.state, RendererPlayerState::Playing);
    assert_eq!(session.duration, Some(215.5));
    let load = requests(&mut fake).await.into_iter().find(|r| r.kind() == "LOAD").expect("a LOAD");
    assert_eq!(
        (load.namespace.as_str(), load.destination_id.as_str()),
        (NAMESPACE_MEDIA, "transport-1")
    );
    assert_eq!(load.payload["sessionId"], "session-1");
    assert_eq!(load.payload["media"]["contentId"], "http://192.0.2.10:4000/stream/track-1");
}

fn refusals() -> Vec<(&'static str, Value)> {
    vec![
        ("LOAD_FAILED", json!({"type": "LOAD_FAILED", "itemId": 1, "detailedErrorCode": 104})),
        ("LOAD_CANCELLED", json!({"type": "LOAD_CANCELLED"})),
        ("INVALID_PLAYER_STATE", json!({"type": "INVALID_PLAYER_STATE"})),
        ("INVALID_COMMAND", json!({"type": "INVALID_REQUEST", "reason": "INVALID_COMMAND"})),
        ("104", json!({"type": "ERROR", "detailedErrorCode": 104})),
    ]
}

#[tokio::test]
async fn a_load_the_receiver_refuses_fails_as_a_command_error_and_the_connection_stays_up() {
    for (named, payload) in refusals() {
        let (client, fake) = client_over_a_fake_receiver().await;
        fake.answer_next(
            "LOAD",
            Script::Replies(vec![Reply::Raw { namespace: NAMESPACE_MEDIA, payload }]),
        );
        let error = client.load(&media()).await.expect_err("the load is refused");
        let RendererError::Command(text) = error else { panic!("{named}: {error:?}") };
        assert!(text.contains(named), "{named}: {text}");
        assert!(client.end().is_none(), "{named}");
        assert_eq!(client.waiting_requests(), 0, "{named}");
        assert!(client.media_session().is_none(), "{named}: a refused load makes no session");
    }
}

#[tokio::test(start_paused = true)]
async fn a_command_the_receiver_does_not_answer_fails_as_a_command_error_and_leaves_the_table() {
    let (client, fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    fake.answer_next("PAUSE", Script::NoAnswer);

    let error = client.pause().await.expect_err("no answer");
    assert!(matches!(&error, RendererError::Command(text) if text.contains("10 s")), "{error:?}");
    assert_eq!(client.waiting_requests(), 0);
    assert!(client.end().is_none());
    assert_eq!(
        client.media_session().unwrap().state,
        RendererPlayerState::Playing,
        "a command that took no effect the receiver stated moves nothing"
    );
    client.pause().await.expect("the next command is answered");
    assert_eq!(client.media_session().unwrap().state, RendererPlayerState::Paused);
}

#[tokio::test]
async fn commands_reach_the_receiver_and_the_answers_move_the_state() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    client.pause().await.unwrap();
    assert_eq!(client.status().player_state, RendererPlayerState::Paused);
    client.play().await.unwrap();
    assert_eq!(client.status().player_state, RendererPlayerState::Playing);
    client.seek(Duration::from_millis(30_250)).await.unwrap();
    assert_eq!(client.status().position, Some(Duration::from_millis(30_250)));
    client.set_volume(Volume::new(0.5).unwrap()).await.unwrap();
    assert_eq!(client.status().volume, Some(0.5));
    client.stop().await.unwrap();
    assert_eq!(client.status().player_state, RendererPlayerState::Idle);

    let kinds: Vec<(String, String)> = requests(&mut fake)
        .await
        .into_iter()
        .skip(3)
        .map(|r| (r.kind().to_string(), r.destination_id))
        .collect();
    let expected = [
        ("LOAD", "transport-1"),
        ("PAUSE", "transport-1"),
        ("PLAY", "transport-1"),
        ("SEEK", "transport-1"),
        ("SET_VOLUME", "receiver-0"),
        ("STOP", "transport-1"),
    ];
    assert_eq!(kinds, expected.map(|(k, d)| (k.to_string(), d.to_string())));
}

#[tokio::test]
async fn a_media_command_before_any_load_fails_without_asking_the_receiver() {
    let (client, _fake) = client_over_a_fake_receiver().await;
    assert!(matches!(client.play().await, Err(RendererError::Command(_))));
    assert_eq!(client.waiting_requests(), 0);
}

#[tokio::test]
async fn a_status_nobody_asked_for_is_followed() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();

    fake.another_sender_pauses();
    heard_everything(&mut fake).await;
    let paused = client.media_session().unwrap();
    assert_eq!(paused.state, RendererPlayerState::Paused);
    assert_eq!(paused.duration, Some(215.5), "a status without media keeps the duration");
    assert!(client.end().is_none(), "another sender's pause is followed and the session goes on");

    fake.finish_track();
    heard_everything(&mut fake).await;
    assert_eq!(client.status().player_state, RendererPlayerState::Finished);
}

#[tokio::test]
async fn a_status_states_what_it_states_and_no_more() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();

    fake.push_media_status(json!({
        "mediaSessionId": 1, "playerState": "PLAYING", "currentTime": "soon",
        "media": {"duration": null},
    }));
    heard_everything(&mut fake).await;
    let session = client.media_session().unwrap();
    assert_eq!((session.current_time, session.duration), (None, None));
    assert_eq!(client.status().position, None);

    fake.push_media_status(json!({
        "mediaSessionId": 1, "playerState": "PLAYING", "currentTime": 12.5,
    }));
    heard_everything(&mut fake).await;
    let session = client.media_session().unwrap();
    assert_eq!(session.current_time, Some(12.5));
    assert_eq!(session.duration, None, "the duration the last media stated was none");
}

#[tokio::test]
async fn an_empty_status_list_is_idle() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    fake.send_from(
        "transport-1",
        NAMESPACE_MEDIA,
        json!({"type": "MEDIA_STATUS", "requestId": 0, "status": []}),
    );
    heard_everything(&mut fake).await;
    let status = client.status();
    assert_eq!((status.player_state, status.position), (RendererPlayerState::Idle, None));
    assert!(client.end().is_none());
}

#[tokio::test]
async fn a_device_switched_off_mid_play_ends_the_session_once_as_lost() {
    let (client, fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();

    fake.another_sender_pauses();
    fake.stop_app();
    fake.close_transport();
    let end = ended(&client).await;
    assert_eq!(end, End::Lost("the receiver's media app ended".to_string()));
    assert_eq!(client.end(), Some(&end), "the close that follows changes nothing");
}

#[tokio::test]
async fn a_close_from_the_apps_transport_ends_the_session_as_lost() {
    let (client, fake) = client_over_a_fake_receiver().await;
    fake.close_transport();
    let End::Lost(reason) = ended(&client).await else { panic!("not a loss") };
    assert!(reason.contains("transport-1 closed its connection"), "{reason}");
}

#[tokio::test]
async fn the_same_transport_with_a_new_session_id_is_an_app_that_ended() {
    let (client, fake) = client_over_a_fake_receiver().await;
    fake.replace_app_keeping_transport();
    assert_eq!(ended(&client).await, End::Lost("the receiver's media app ended".to_string()));
}

#[tokio::test]
async fn another_senders_load_ends_the_session_as_lost() {
    let (client, fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    fake.another_sender_loads();
    assert_eq!(ended(&client).await, End::Lost(TOOK_OVER.to_string()));
}

#[tokio::test(start_paused = true)]
async fn another_senders_load_while_bae_waits_for_its_own_load_is_not_taken_in() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    fake.answer_next("LOAD", Script::NoAnswer);

    let second_media = media();
    let waiting = client.load(&second_media);
    let others = async {
        fake.another_sender_loads();
        heard_everything(&mut fake).await;
        assert!(client.end().is_none(), "statuses for other sessions wait for the load's answer");
        assert_eq!(client.media_session().unwrap().id, 1);
    };
    let (loaded, ()) = tokio::join!(waiting, others);
    assert!(matches!(loaded, Err(RendererError::Command(_))));
}

#[tokio::test]
async fn the_session_before_the_loaded_one_may_still_be_said_interrupted() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    client.load(&media()).await.unwrap();
    assert_eq!(client.media_session().unwrap().id, 2);

    fake.push_media_status(json!({
        "mediaSessionId": 1, "playerState": "IDLE", "idleReason": "INTERRUPTED",
    }));
    heard_everything(&mut fake).await;
    assert!(client.end().is_none());
    assert_eq!(client.status().player_state, RendererPlayerState::Playing);
}

#[tokio::test]
async fn a_receiver_that_goes_away_fails_the_requests_waiting_and_those_that_follow() {
    let (client, fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    fake.answer_next("PAUSE", Script::NoAnswer);
    let waiting = client.pause();
    let going = async {
        fake.drop_connection();
    };
    let (paused, ()) = tokio::join!(waiting, going);
    assert!(matches!(paused, Err(RendererError::Connection(_))), "{paused:?}");
    assert_eq!(client.waiting_requests(), 0);
    let end = ended(&client).await;
    assert_eq!(client.play().await, Err(ended_error(&end)));
}

#[tokio::test]
async fn ending_a_session_stops_the_media_then_closes_the_connections_then_the_socket() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    requests(&mut fake).await;

    client.close().await;
    let mut sent = Vec::new();
    while let Some(received) = fake.next_received_or_end().await {
        let kind = received.kind().to_string();
        sent.push((received.namespace, kind, received.destination_id));
    }
    assert_eq!(
        sent,
        [
            (NAMESPACE_MEDIA.to_string(), "STOP".to_string(), "transport-1".to_string()),
            (NAMESPACE_CONNECTION.to_string(), "CLOSE".to_string(), "transport-1".to_string()),
            (NAMESPACE_CONNECTION.to_string(), "CLOSE".to_string(), "receiver-0".to_string()),
        ]
    );
    assert_eq!(client.end(), Some(&End::Closed), "a close the client made is not a loss");
}

#[tokio::test]
async fn a_client_connects_over_tls_and_plays_in_the_receivers_app() {
    let (mut fake, port) = FakeReceiver::serve_tls(true).await;
    let client = CastClient::connect(
        IpAddr::from([127, 0, 0, 1]),
        port,
        &Diagnostics::noop(),
        &tokio::runtime::Handle::current(),
    )
    .await
    .expect("the receiver launches its app");
    client.load(&media()).await.unwrap();
    assert_eq!(client.status().player_state, RendererPlayerState::Playing);
    assert_eq!(fake.next_received().await.kind(), "CONNECT");
}

#[tokio::test]
async fn casting_again_after_a_lost_connection_connects_from_scratch_with_a_new_launch() {
    let (first, fake) = client_over_a_fake_receiver().await;
    fake.drop_connection();
    ended(&first).await;
    assert!(matches!(first.load(&media()).await, Err(RendererError::Connection(_))));

    let (second, mut second_fake) = client_over_a_fake_receiver().await;
    let launches = requests(&mut second_fake)
        .await
        .into_iter()
        .filter(|received| received.kind() == "LAUNCH")
        .count();
    assert_eq!(launches, 1, "a new client launches the app again");
    second.load(&media()).await.expect("the new client plays");
}

#[tokio::test]
async fn the_status_carries_the_rate_the_receiver_stated_when_it_can_be_used() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    assert_eq!(client.status().rate, Some(1.0));

    for (stated, rate) in [
        (json!(0.5), Some(0.5)),
        (json!(0), None),
        (json!(-1), None),
        (json!("x"), None),
        (json!(null), None),
    ] {
        fake.push_media_status(json!({
            "mediaSessionId": 1, "playerState": "PLAYING", "currentTime": 3.0, "playbackRate": stated,
        }));
        heard_everything(&mut fake).await;
        assert_eq!(client.status().rate, rate, "{stated}");
    }
}

/// What playback hears of a client through the sink it hands over: its events,
/// and the clock that stamps them.
struct Listener {
    heard: tokio::sync::mpsc::UnboundedReceiver<RendererSessionEvent>,
    clock: Arc<bae_playback::ManualPlaybackClock>,
    sink: StatusSink,
}

fn listener() -> Listener {
    let (events, heard) = tokio::sync::mpsc::unbounded_channel();
    let clock = Arc::new(bae_playback::ManualPlaybackClock::new(chrono::DateTime::UNIX_EPOCH));
    let sink = StatusSink::new(
        SessionId(4),
        clock.clone(),
        Arc::new(move |event| {
            drop(events.send(event));
        }),
    );
    Listener { heard, clock, sink }
}

impl Listener {
    async fn next(&mut self) -> RendererSessionEvent {
        tokio::time::timeout(HANG_GUARD, self.heard.recv())
            .await
            .expect("an event in time")
            .expect("the client holds the sink")
    }

    /// The events heard so far, without waiting.
    fn so_far(&mut self) -> Vec<RendererSessionEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.heard.try_recv() {
            events.push(event);
        }
        events
    }
}

fn status_of(event: RendererSessionEvent) -> (ReceiverStatus, chrono::DateTime<chrono::Utc>) {
    match event {
        RendererSessionEvent::Status { session, status, observed_at } => {
            assert_eq!(session, SessionId(4));
            (status, observed_at)
        }
        other => panic!("not a status: {other:?}"),
    }
}

#[tokio::test]
async fn a_client_says_the_status_it_holds_once_when_it_is_asked_to_report() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    heard_everything(&mut fake).await;
    let mut listener = listener();

    client.report_to(listener.sink.clone());

    let (status, _) = status_of(listener.next().await);
    assert_eq!(status.player_state, RendererPlayerState::Playing);
    assert_eq!(status.duration, Some(Duration::from_millis(215_500)));
    assert!(listener.so_far().is_empty(), "one report, not one for each thing it holds");
}

#[tokio::test]
async fn a_pause_the_device_makes_is_said_as_it_happens_and_stamped_by_the_clock() {
    let (client, fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    let mut listener = listener();
    client.report_to(listener.sink.clone());
    listener.next().await;

    listener.clock.advance(Duration::from_secs(9));
    fake.another_sender_pauses();
    let (paused, observed_at) = status_of(listener.next().await);

    assert_eq!(paused.player_state, RendererPlayerState::Paused);
    assert_eq!(observed_at, chrono::DateTime::UNIX_EPOCH + chrono::Duration::seconds(9));
    fake.finish_track();
    let (finished, _) = status_of(listener.next().await);
    assert_eq!(finished.player_state, RendererPlayerState::Finished);
}

#[tokio::test]
async fn a_volume_the_receiver_states_is_said() {
    let (client, _fake) = client_over_a_fake_receiver().await;
    let mut listener = listener();
    client.report_to(listener.sink.clone());
    listener.next().await;

    client.set_volume(Volume::new(0.5).unwrap()).await.unwrap();

    let (status, _) = status_of(listener.next().await);
    assert_eq!(status.volume, Some(0.5));
}

#[tokio::test]
async fn what_changes_nothing_of_bae_is_not_said() {
    let (client, mut fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    let mut listener = listener();
    client.report_to(listener.sink.clone());
    listener.next().await;

    fake.ping();
    fake.send_json(NAMESPACE_MEDIA, json!({"type": "SOMETHING_NEW", "requestId": 0}));
    heard_everything(&mut fake).await;

    assert!(listener.so_far().is_empty(), "nothing of bae's media or volume changed");
}

#[tokio::test]
async fn a_lost_connection_is_said_once_and_not_again_by_a_failing_command() {
    let (client, fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    let mut listener = listener();
    client.report_to(listener.sink.clone());
    listener.next().await;

    fake.drop_connection();
    let lost = listener.next().await;
    ended(&client).await;
    let failed = client.pause().await.expect_err("the connection is gone");

    assert!(
        matches!(
            &lost,
            RendererSessionEvent::Ended {
                session: SessionId(4),
                error: RendererError::Connection(_)
            }
        ),
        "{lost:?}"
    );
    assert!(matches!(failed, RendererError::Connection(_)));
    assert!(listener.so_far().is_empty(), "the loss is said once");
}

#[tokio::test]
async fn closing_the_client_says_nothing_is_lost() {
    let (client, _fake) = client_over_a_fake_receiver().await;
    client.load(&media()).await.unwrap();
    let mut listener = listener();
    client.report_to(listener.sink.clone());
    listener.next().await;

    client.close().await;
    ended(&client).await;

    assert!(
        listener.so_far().is_empty(),
        "nothing reaches playback once the client is closed, not even the stop's answer"
    );
}

#[tokio::test]
async fn a_client_already_lost_says_so_when_it_is_asked_to_report() {
    let (client, fake) = client_over_a_fake_receiver().await;
    fake.drop_connection();
    ended(&client).await;
    let mut listener = listener();

    client.report_to(listener.sink.clone());

    assert!(matches!(listener.next().await, RendererSessionEvent::Ended { .. }));
    assert!(listener.so_far().is_empty());
}
