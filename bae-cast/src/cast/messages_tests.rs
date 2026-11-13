use serde_json::{json, Value};

use super::super::wire::{encode_frame, FrameReader};
use super::*;

fn media() -> RendererMedia {
    RendererMedia {
        url: "http://192.0.2.10:4000/stream/track-1".to_string(),
        content_type: "audio/flac".to_string(),
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        cover_url: Some("http://192.0.2.10:4000/cover/1".to_string()),
        duration: Some(Duration::from_millis(215_500)),
    }
}

/// The payload a request arrives as after the frame codec: encoded, read back.
async fn sent_payload(request: Request<'_>) -> (CastMessage, Value) {
    let frame = request.into_frame("sender-0", "dest-1");
    let bytes = encode_frame(&frame).unwrap();
    let read = FrameReader::new(std::io::Cursor::new(bytes)).next().await.unwrap().unwrap();
    assert_eq!(read, frame);
    let payload = serde_json::from_str(read.payload_utf8.as_deref().unwrap()).unwrap();
    (read, payload)
}

#[tokio::test]
async fn each_request_round_trips_through_the_codec_as_the_json_the_receiver_expects() {
    let media = media();
    let cases: Vec<(Request, &str, Value)> = vec![
        (
            Request::Connect,
            NAMESPACE_CONNECTION,
            json!({"type": "CONNECT", "origin": {}, "userAgent": "bae"}),
        ),
        (Request::Close, NAMESPACE_CONNECTION, json!({"type": "CLOSE"})),
        (Request::Ping, NAMESPACE_HEARTBEAT, json!({"type": "PING"})),
        (Request::Pong, NAMESPACE_HEARTBEAT, json!({"type": "PONG"})),
        (
            Request::ReceiverStatus { request_id: 1 },
            NAMESPACE_RECEIVER,
            json!({"type": "GET_STATUS", "requestId": 1}),
        ),
        (
            Request::Launch { request_id: 2, app_id: "CC1AD845" },
            NAMESPACE_RECEIVER,
            json!({"type": "LAUNCH", "requestId": 2, "appId": "CC1AD845"}),
        ),
        (
            Request::SetVolume { request_id: 3, level: 0.5 },
            NAMESPACE_RECEIVER,
            json!({"type": "SET_VOLUME", "requestId": 3, "volume": {"level": 0.5}}),
        ),
        (
            Request::MediaStatus { request_id: 4 },
            NAMESPACE_MEDIA,
            json!({"type": "GET_STATUS", "requestId": 4}),
        ),
        (
            Request::Load { request_id: 5, session_id: "session-1", media: &media },
            NAMESPACE_MEDIA,
            json!({
                "type": "LOAD", "requestId": 5, "sessionId": "session-1",
                "autoplay": true, "currentTime": 0,
                "media": {
                    "contentId": "http://192.0.2.10:4000/stream/track-1",
                    "contentType": "audio/flac",
                    "streamType": "BUFFERED",
                    "duration": 215.5,
                    "metadata": {
                        "metadataType": 3,
                        "title": "Track Title",
                        "albumName": "Album Title",
                        "artist": "Artist Name",
                        "albumArtist": "Artist Name",
                        "images": [{"url": "http://192.0.2.10:4000/cover/1"}],
                    },
                },
            }),
        ),
        (
            Request::Play { request_id: 6, media_session_id: 9 },
            NAMESPACE_MEDIA,
            json!({"type": "PLAY", "requestId": 6, "mediaSessionId": 9}),
        ),
        (
            Request::Pause { request_id: 7, media_session_id: 9 },
            NAMESPACE_MEDIA,
            json!({"type": "PAUSE", "requestId": 7, "mediaSessionId": 9}),
        ),
        (
            Request::Seek {
                request_id: 8,
                media_session_id: 9,
                position: Duration::from_millis(30_250),
            },
            NAMESPACE_MEDIA,
            json!({"type": "SEEK", "requestId": 8, "mediaSessionId": 9, "currentTime": 30.25}),
        ),
        (
            Request::StopMedia { request_id: 10, media_session_id: 9 },
            NAMESPACE_MEDIA,
            json!({"type": "STOP", "requestId": 10, "mediaSessionId": 9}),
        ),
    ];
    for (request, namespace, expected) in cases {
        let (frame, payload) = sent_payload(request).await;
        assert_eq!(frame.namespace, namespace, "{request:?}");
        assert_eq!(
            (frame.source_id.as_str(), frame.destination_id.as_str()),
            ("sender-0", "dest-1")
        );
        assert_eq!(frame.protocol_version, 0, "bae declares protocol 1.0");
        assert_eq!(payload, expected, "{request:?}");
    }
}

#[tokio::test]
async fn a_load_without_a_cover_or_a_duration_states_neither() {
    let bare = RendererMedia { cover_url: None, duration: None, ..media() };
    let (_, payload) =
        sent_payload(Request::Load { request_id: 1, session_id: "s", media: &bare }).await;
    assert_eq!(payload["media"]["metadata"]["images"], json!([]));
    assert!(payload["media"].get("duration").is_none());
}

fn frame(namespace: &str, payload: Value) -> CastMessage {
    CastMessage::text("receiver-0", "sender-0", namespace, payload.to_string())
}

fn read(namespace: &str, payload: Value) -> Inbound {
    Inbound::from_frame(&frame(namespace, payload)).unwrap()
}

fn media_entries(status: Value) -> Vec<MediaStatusEntry> {
    match read(NAMESPACE_MEDIA, json!({"type": "MEDIA_STATUS", "requestId": 0, "status": status})) {
        Inbound::MediaStatus { entries, .. } => entries,
        other => panic!("not a media status: {other:?}"),
    }
}

#[test]
fn the_heartbeat_and_connection_messages_are_read() {
    assert_eq!(read(NAMESPACE_HEARTBEAT, json!({"type": "PING"})), Inbound::Ping);
    assert_eq!(read(NAMESPACE_HEARTBEAT, json!({"type": "PONG"})), Inbound::Pong);
    assert_eq!(
        read(NAMESPACE_CONNECTION, json!({"type": "CLOSE"})),
        Inbound::Close { source_id: "receiver-0".to_string() }
    );
}

#[test]
fn a_frame_of_a_namespace_bae_does_not_use_is_ignored_without_being_read() {
    let ignored = Inbound::from_frame(&CastMessage::text(
        "receiver-0",
        "sender-0",
        "urn:x-cast:com.google.cast.tp.deviceauth",
        "not json at all".to_string(),
    ))
    .unwrap();
    assert_eq!(
        ignored,
        Inbound::Ignored {
            namespace: "urn:x-cast:com.google.cast.tp.deviceauth".to_string(),
            kind: None
        }
    );
}

#[test]
fn a_type_bae_does_not_know_is_ignored_and_named() {
    assert_eq!(
        read(NAMESPACE_RECEIVER, json!({"type": "DEVICE_UPDATED", "requestId": 0})),
        Inbound::Ignored {
            namespace: NAMESPACE_RECEIVER.to_string(),
            kind: Some("DEVICE_UPDATED".to_string())
        }
    );
    // A known type in the wrong namespace is not that namespace's message.
    assert!(matches!(
        read(NAMESPACE_MEDIA, json!({"type": "RECEIVER_STATUS", "status": {}})),
        Inbound::Ignored { .. }
    ));
}

#[test]
fn a_payload_with_a_response_type_and_no_type_is_ignored() {
    assert_eq!(
        read(NAMESPACE_MEDIA, json!({"responseType": "GET_STATUS", "requestId": 3})),
        Inbound::Ignored { namespace: NAMESPACE_MEDIA.to_string(), kind: None }
    );
}

#[test]
fn a_binary_payload_on_a_namespace_bae_reads_is_an_error() {
    let mut binary = frame(NAMESPACE_MEDIA, json!({}));
    binary.payload_type = PayloadType::Binary as i32;
    binary.payload_utf8 = None;
    binary.payload_binary = Some(vec![1, 2, 3]);
    assert!(matches!(Inbound::from_frame(&binary), Err(MessageError::BinaryPayload { .. })));
}

#[test]
fn a_text_frame_with_no_text_or_with_text_that_is_not_json_is_an_error() {
    let mut empty = frame(NAMESPACE_MEDIA, json!({}));
    empty.payload_utf8 = None;
    assert!(matches!(Inbound::from_frame(&empty), Err(MessageError::NoPayload { .. })));
    let garbage = CastMessage::text("a", "b", NAMESPACE_MEDIA, "{".to_string());
    assert!(matches!(Inbound::from_frame(&garbage), Err(MessageError::NotJson { .. })));
}

#[test]
fn a_known_type_whose_body_cannot_be_read_is_an_error() {
    let no_player_state =
        frame(NAMESPACE_MEDIA, json!({"type": "MEDIA_STATUS", "status": [{"mediaSessionId": 1}]}));
    assert!(matches!(
        Inbound::from_frame(&no_player_state),
        Err(MessageError::Unreadable { kind, .. }) if kind == "MEDIA_STATUS"
    ));
    let unknown_state = frame(
        NAMESPACE_MEDIA,
        json!({"type": "MEDIA_STATUS", "status": [{"mediaSessionId": 1, "playerState": "DANCING"}]}),
    );
    assert!(matches!(Inbound::from_frame(&unknown_state), Err(MessageError::Unreadable { .. })));
}

#[test]
fn a_receiver_status_lists_its_apps_and_volume() {
    let status = read(
        NAMESPACE_RECEIVER,
        json!({"type": "RECEIVER_STATUS", "requestId": 2, "status": {
            "applications": [{
                "appId": "CC1AD845", "sessionId": "session-1", "transportId": "transport-1",
                "displayName": "Default Media Receiver", "namespaces": [{"name": "x"}],
            }],
            "volume": {"level": 0.4, "muted": false},
        }}),
    );
    assert_eq!(
        status,
        Inbound::ReceiverStatus {
            request_id: 2,
            status: ReceiverStatusBody {
                applications: vec![Application {
                    app_id: "CC1AD845".to_string(),
                    session_id: "session-1".to_string(),
                    transport_id: Some("transport-1".to_string()),
                }],
                volume: Some(ReceiverVolume { level: Some(0.4) }),
            }
        }
    );
}

#[test]
fn a_receiver_status_with_no_applications_field_lists_none() {
    let Inbound::ReceiverStatus { status, .. } = read(
        NAMESPACE_RECEIVER,
        json!({"type": "RECEIVER_STATUS", "requestId": 0, "status": {"volume": {"level": 1.0}}}),
    ) else {
        panic!("not a receiver status");
    };
    assert!(status.applications.is_empty());
}

#[test]
fn the_launch_messages_are_read() {
    assert_eq!(
        read(
            NAMESPACE_RECEIVER,
            json!({"type": "LAUNCH_STATUS", "requestId": 2, "status": "USER_ALLOWED"})
        ),
        Inbound::LaunchStatus { request_id: 2, status: Some("USER_ALLOWED".to_string()) }
    );
    assert_eq!(
        read(
            NAMESPACE_RECEIVER,
            json!({"type": "LAUNCH_ERROR", "requestId": 2, "reason": "NOT_ALLOWED", "extendedError": "USER_NOT_ALLOWED"})
        ),
        Inbound::LaunchError {
            request_id: 2,
            reason: Some("NOT_ALLOWED".to_string()),
            extended_error: Some("USER_NOT_ALLOWED".to_string()),
        }
    );
}

#[test]
fn refusals_name_their_type_reason_and_code() {
    let cases = [
        (
            NAMESPACE_MEDIA,
            json!({"type": "LOAD_FAILED", "requestId": 5, "itemId": 1, "detailedErrorCode": 104}),
            "LOAD_FAILED (code 104)",
        ),
        (NAMESPACE_MEDIA, json!({"type": "LOAD_CANCELLED", "requestId": 5}), "LOAD_CANCELLED"),
        (
            NAMESPACE_MEDIA,
            json!({"type": "INVALID_PLAYER_STATE", "requestId": 5}),
            "INVALID_PLAYER_STATE",
        ),
        (
            NAMESPACE_MEDIA,
            json!({"type": "INVALID_REQUEST", "requestId": 5, "reason": "INVALID_COMMAND"}),
            "INVALID_REQUEST (INVALID_COMMAND)",
        ),
        (
            NAMESPACE_MEDIA,
            json!({"type": "ERROR", "requestId": 5, "detailedErrorCode": 104, "reason": "x"}),
            "ERROR (x) (code 104)",
        ),
        (
            NAMESPACE_RECEIVER,
            json!({"type": "INVALID_REQUEST", "requestId": 5, "reason": "DUPLICATE_REQUESTID"}),
            "INVALID_REQUEST (DUPLICATE_REQUESTID)",
        ),
    ];
    for (namespace, payload, described) in cases {
        let Inbound::Refused(refusal) = read(namespace, payload) else { panic!("{described}") };
        assert_eq!(refusal.request_id, 5);
        assert_eq!(refusal.describe(), described);
    }
}

#[test]
fn a_media_status_without_media_has_none_and_one_with_media_has_its_duration() {
    let entries = media_entries(json!([
        {"mediaSessionId": 1, "playerState": "PLAYING", "currentTime": 12.5, "playbackRate": 1,
         "media": {"contentId": "x", "duration": 215.5}},
        {"mediaSessionId": 1, "playerState": "PAUSED", "currentTime": 13.0, "playbackRate": 1},
    ]));
    assert_eq!(entries[0].media, Some(MediaInfo { duration: Some(215.5) }));
    assert_eq!(entries[0].current_time, Some(12.5));
    assert_eq!(entries[1].media, None, "the receiver sends media only when it changed");
}

#[test]
fn a_null_or_missing_duration_is_no_duration() {
    let entries = media_entries(json!([
        {"mediaSessionId": 1, "playerState": "PLAYING", "media": {"duration": null}},
        {"mediaSessionId": 1, "playerState": "PLAYING", "media": {"contentId": "x"}},
    ]));
    assert_eq!(entries[0].media, Some(MediaInfo { duration: None }));
    assert_eq!(entries[1].media, Some(MediaInfo { duration: None }));
}

#[test]
fn a_current_time_that_is_not_a_number_is_not_a_position() {
    let entries = media_entries(json!([
        {"mediaSessionId": 1, "playerState": "PLAYING", "currentTime": "soon"},
        {"mediaSessionId": 1, "playerState": "PLAYING", "currentTime": {"s": 1}},
        {"mediaSessionId": 1, "playerState": "PLAYING"},
    ]));
    for entry in entries {
        assert_eq!(entry.current_time, None);
    }
}

#[test]
fn idle_is_finished_only_for_the_reason_finished() {
    let state = |reason: Option<&str>| {
        let mut entry = json!({"mediaSessionId": 1, "playerState": "IDLE"});
        if let Some(reason) = reason {
            entry["idleReason"] = json!(reason);
        }
        media_entries(json!([entry]))[0].renderer_state()
    };
    assert_eq!(state(None), RendererPlayerState::Idle);
    assert_eq!(state(Some("ERROR")), RendererPlayerState::Idle);
    assert_eq!(state(Some("CANCELLED")), RendererPlayerState::Idle);
    assert_eq!(state(Some("INTERRUPTED")), RendererPlayerState::Idle);
    assert_eq!(state(Some("FINISHED")), RendererPlayerState::Finished);
}

#[test]
fn the_other_player_states_map_across() {
    let entries = media_entries(json!([
        {"mediaSessionId": 1, "playerState": "PLAYING"},
        {"mediaSessionId": 1, "playerState": "PAUSED"},
        {"mediaSessionId": 1, "playerState": "BUFFERING", "currentTime": 0, "items": [{"itemId": 1}]},
    ]));
    let states: Vec<_> = entries.iter().map(MediaStatusEntry::renderer_state).collect();
    assert_eq!(
        states,
        [RendererPlayerState::Playing, RendererPlayerState::Paused, RendererPlayerState::Buffering]
    );
}

#[test]
fn an_empty_status_list_has_no_entries() {
    assert!(media_entries(json!([])).is_empty());
}

#[test]
fn a_playback_rate_that_is_there_and_no_number_is_told_from_one_that_is_absent() {
    let rates = media_entries(json!([
        {"mediaSessionId": 1, "playerState": "PLAYING", "playbackRate": 0.5},
        {"mediaSessionId": 1, "playerState": "PLAYING", "playbackRate": 1},
        {"mediaSessionId": 1, "playerState": "PLAYING"},
        {"mediaSessionId": 1, "playerState": "PLAYING", "playbackRate": null},
        {"mediaSessionId": 1, "playerState": "PLAYING", "playbackRate": "fast"},
        {"mediaSessionId": 1, "playerState": "PLAYING", "playbackRate": {"n": 1}},
    ]))
    .into_iter()
    .map(|entry| entry.playback_rate)
    .collect::<Vec<_>>();
    assert_eq!(
        rates,
        [
            StatedRate::Number(0.5),
            StatedRate::Number(1.0),
            StatedRate::Absent,
            StatedRate::Absent,
            StatedRate::NotANumber("\"fast\"".to_string()),
            StatedRate::NotANumber("{\"n\":1}".to_string()),
        ]
    );
}
