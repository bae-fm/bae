//! Discovery-parsing and SOAP wire tests, all against canned fixtures — no
//! network. The device-description fixtures carry the real-world quirks the
//! parser must survive: a missing `friendlyName`, a relative `controlURL`, and a
//! control URL naming another host and port.

use std::time::Duration;

use super::discovery::{parse_device_description, parse_ssdp_location};
use super::soap::{
    self, parse_position_info, parse_transport_state, DidlMetadata, PositionInfo, TransportState,
};
use crate::renderer::{RendererConnection, RendererDevice};

/// The DLNA connection's control URLs, for asserting on a parsed device.
fn dlna_urls(device: &RendererDevice) -> (String, Option<String>) {
    match &device.connection {
        RendererConnection::Dlna {
            av_transport_url,
            rendering_control_url,
        } => (av_transport_url.clone(), rendering_control_url.clone()),
        _ => panic!("a device description must parse to a DLNA connection"),
    }
}

// -- SSDP response → location -------------------------------------------------

#[test]
fn ssdp_response_yields_location_case_insensitively() {
    // Real responders vary header case; the parser must not depend on it.
    let response = "HTTP/1.1 200 OK\r\n\
        CACHE-CONTROL: max-age=1800\r\n\
        Location: http://192.168.1.50:8080/description.xml\r\n\
        ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
        USN: uuid:abcd::urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
    assert_eq!(
        parse_ssdp_location(response).as_deref(),
        Some("http://192.168.1.50:8080/description.xml")
    );
}

#[test]
fn ssdp_response_without_location_is_none() {
    let response = "HTTP/1.1 200 OK\r\nST: ssdp:all\r\n\r\n";
    assert!(parse_ssdp_location(response).is_none());
}

// -- device description → device ----------------------------------------------

/// A renderer description whose control URLs are relative to the description's
/// own location — the common case.
fn relative_urls_description() -> &'static str {
    r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>Living Room Receiver</friendlyName>
    <UDN>uuid:11111111-2222-3333-4444-555555555555</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>AVTransport/control</controlURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>RenderingControl/control</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#
}

#[test]
fn description_with_relative_control_urls_resolves_against_location() {
    let device = parse_device_description(
        relative_urls_description(),
        "http://192.168.1.50:8080/desc/description.xml",
    )
    .expect("a renderer with AVTransport is usable");

    assert_eq!(device.id, "uuid:11111111-2222-3333-4444-555555555555");
    assert_eq!(device.name, "Living Room Receiver");
    assert_eq!(device.kind(), crate::renderer::RendererKind::Dlna);
    let (av, rc) = dlna_urls(&device);
    assert_eq!(av, "http://192.168.1.50:8080/desc/AVTransport/control");
    assert_eq!(
        rc.as_deref(),
        Some("http://192.168.1.50:8080/desc/RenderingControl/control")
    );
}

#[test]
fn description_without_friendly_name_falls_back_to_host() {
    let xml = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <UDN>uuid:no-name-device</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>/upnp/control/AVTransport1</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;
    let device = parse_device_description(xml, "http://10.0.0.9:49152/dev.xml")
        .expect("still usable without a friendly name");
    assert_eq!(device.name, "10.0.0.9");
    let (av, rc) = dlna_urls(&device);
    // An absolute-path controlURL resolves against the location's origin.
    assert_eq!(av, "http://10.0.0.9:49152/upnp/control/AVTransport1");
    // No RenderingControl service: volume is simply unsupported.
    assert!(rc.is_none());
}

#[test]
fn description_control_url_on_another_host_and_port_is_kept_absolute() {
    let xml = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <friendlyName>Split Renderer</friendlyName>
    <UDN>uuid:split</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>http://192.168.1.77:9000/AVTransport/control</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;
    let device =
        parse_device_description(xml, "http://192.168.1.50:8080/description.xml").expect("usable");
    assert_eq!(
        dlna_urls(&device).0,
        "http://192.168.1.77:9000/AVTransport/control",
        "an absolute controlURL on another host/port is used verbatim"
    );
}

#[test]
fn description_url_base_overrides_location_for_resolution() {
    let xml = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <URLBase>http://192.168.1.50:2870/</URLBase>
  <device>
    <friendlyName>Base Renderer</friendlyName>
    <UDN>uuid:base</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>ctl/avt</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;
    let device =
        parse_device_description(xml, "http://192.168.1.50:8080/desc.xml").expect("usable");
    assert_eq!(
        dlna_urls(&device).0,
        "http://192.168.1.50:2870/ctl/avt",
        "control URLs resolve against URLBase when the description carries one"
    );
}

#[test]
fn description_without_av_transport_is_unusable() {
    let xml = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <friendlyName>No Transport</friendlyName>
    <UDN>uuid:no-avt</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <controlURL>rc</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;
    assert!(
        parse_device_description(xml, "http://host/desc.xml").is_none(),
        "a device that can't be played to (no AVTransport) is dropped"
    );
}

// -- SOAP envelope construction -----------------------------------------------

#[test]
fn set_av_transport_uri_carries_url_and_escaped_didl() {
    let request = soap::set_av_transport_uri(
        "http://10.0.0.5:9000/stream?id=tr-1&format=raw",
        &DidlMetadata {
            title: "Song",
            artist: "Artist",
            album: "Album",
            cover_url: Some("http://10.0.0.5:9000/cover?id=tr-1"),
            content_type: "audio/flac",
        },
    );

    assert_eq!(
        request.soap_action,
        "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\""
    );
    // The stream URL appears in CurrentURI with its `&` escaped.
    assert!(
        request.body.contains(
            "<CurrentURI>http://10.0.0.5:9000/stream?id=tr-1&amp;format=raw</CurrentURI>"
        ),
        "{}",
        request.body
    );
    // The DIDL rides inside CurrentURIMetaData, itself escaped: its own tags
    // become &lt;/&gt; so the envelope stays well-formed.
    assert!(
        request.body.contains("&lt;DIDL-Lite"),
        "the DIDL document must be escaped as text: {}",
        request.body
    );
    assert!(
        request
            .body
            .contains("protocolInfo=&quot;http-get:*:audio/flac:*&quot;"),
        "the protocolInfo MIME rides in the escaped DIDL: {}",
        request.body
    );
}

#[test]
fn didl_escapes_special_characters_and_keeps_unicode() {
    let request = soap::set_av_transport_uri(
        "http://host/s",
        &DidlMetadata {
            title: "Rock & <Roll>",
            artist: "Sigur Rós",
            album: "( )",
            cover_url: None,
            content_type: "audio/mpeg",
        },
    );
    // `&` and `<`/`>` in the title are escaped twice over (DIDL text escaping,
    // then the envelope escaping the DIDL): `&` → `&amp;` → `&amp;amp;`.
    assert!(
        request.body.contains("Rock &amp;amp; &amp;lt;Roll&amp;gt;"),
        "special characters must survive both escaping levels: {}",
        request.body
    );
    // Unicode passes through untouched.
    assert!(
        request.body.contains("Sigur Rós"),
        "unicode text is preserved: {}",
        request.body
    );
    // No cover art: no albumArtURI element in the DIDL.
    assert!(
        !request.body.contains("albumArtURI"),
        "a track with no cover omits albumArtURI: {}",
        request.body
    );
}

#[test]
fn transport_actions_name_the_right_soap_action() {
    assert_eq!(
        soap::play().soap_action,
        "\"urn:schemas-upnp-org:service:AVTransport:1#Play\""
    );
    assert!(soap::play().body.contains("<Speed>1</Speed>"));
    assert_eq!(
        soap::pause().soap_action,
        "\"urn:schemas-upnp-org:service:AVTransport:1#Pause\""
    );
    assert_eq!(
        soap::stop().soap_action,
        "\"urn:schemas-upnp-org:service:AVTransport:1#Stop\""
    );
}

#[test]
fn seek_targets_rel_time_in_hms() {
    let request = soap::seek(Duration::from_secs(3 * 3600 + 4 * 60 + 5));
    assert!(request.body.contains("<Unit>REL_TIME</Unit>"));
    assert!(
        request.body.contains("<Target>3:04:05</Target>"),
        "{}",
        request.body
    );
}

#[test]
fn set_volume_scales_to_upnp_percent_on_rendering_control() {
    let request = soap::set_volume(0.5);
    assert_eq!(
        request.soap_action,
        "\"urn:schemas-upnp-org:service:RenderingControl:1#SetVolume\""
    );
    assert!(
        request.body.contains("<DesiredVolume>50</DesiredVolume>"),
        "{}",
        request.body
    );
    assert!(request.body.contains("<Channel>Master</Channel>"));
    // Out-of-range levels clamp to the 0–100 bounds.
    assert!(soap::set_volume(1.7)
        .body
        .contains("<DesiredVolume>100</DesiredVolume>"));
    assert!(soap::set_volume(-0.3)
        .body
        .contains("<DesiredVolume>0</DesiredVolume>"));
}

// -- SOAP response parsing ----------------------------------------------------

#[test]
fn position_info_parses_rel_time_and_duration() {
    let body = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <u:GetPositionInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <Track>1</Track>
      <TrackDuration>0:03:20</TrackDuration>
      <RelTime>0:01:07</RelTime>
      <AbsTime>0:01:07</AbsTime>
    </u:GetPositionInfoResponse>
  </s:Body>
</s:Envelope>"#;
    assert_eq!(
        parse_position_info(body),
        PositionInfo {
            rel_time: Some(Duration::from_secs(67)),
            track_duration: Some(Duration::from_secs(200)),
        }
    );
}

#[test]
fn position_info_treats_not_implemented_and_missing_as_absent() {
    let body = r#"<u:GetPositionInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <TrackDuration>NOT_IMPLEMENTED</TrackDuration>
      <RelTime>0:00:00</RelTime>
    </u:GetPositionInfoResponse>"#;
    let info = parse_position_info(body);
    assert_eq!(info.rel_time, Some(Duration::ZERO));
    assert_eq!(
        info.track_duration, None,
        "NOT_IMPLEMENTED duration is carried as absent, not zero"
    );
}

#[test]
fn transport_state_maps_known_states() {
    let state = |value: &str| {
        parse_transport_state(&format!(
            "<u:GetTransportInfoResponse xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
             <CurrentTransportState>{value}</CurrentTransportState></u:GetTransportInfoResponse>"
        ))
    };
    assert_eq!(state("PLAYING"), TransportState::Playing);
    assert_eq!(state("PAUSED_PLAYBACK"), TransportState::PausedPlayback);
    assert_eq!(state("STOPPED"), TransportState::Stopped);
    assert_eq!(state("TRANSITIONING"), TransportState::Transitioning);
    assert_eq!(state("NO_MEDIA_PRESENT"), TransportState::NoMediaPresent);
    assert_eq!(state("SOMETHING_ELSE"), TransportState::Other);
}

#[test]
fn transport_state_without_element_is_other() {
    assert_eq!(
        parse_transport_state(
            "<u:GetTransportInfoResponse xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
             </u:GetTransportInfoResponse>"
        ),
        TransportState::Other,
        "a response missing the state element is unknown, not a known state"
    );
}

// -- DLNA channel over a fake HTTP renderer -----------------------------------
//
// The routing tests the plan requires: driving a real `DlnaChannel` against a
// fake renderer that records the SOAP actions it receives and answers the status
// polls from scripted state. This is the class of test that caught the Cast
// `stop()` bug — here it pins that our own `stop()` is never misread as an
// end-of-track advance, and that a STOPPED after playing through is.

use std::sync::{Arc, Mutex};

use crate::renderer::{RendererChannel, RendererError, RendererMedia, RendererPlayerState};

use super::channel::DlnaChannel;

/// The fake renderer's scripted state and the record of what it was asked to do.
#[derive(Default)]
struct FakeRenderer {
    /// SOAP action names received, in order (e.g. "SetAVTransportURI", "Play").
    actions: Vec<String>,
    /// The `CurrentTransportState` the next GetTransportInfo returns.
    transport_state: String,
    /// The `RelTime` / `TrackDuration` the next GetPositionInfo returns.
    rel_time: String,
    track_duration: String,
}

type Shared = Arc<Mutex<FakeRenderer>>;

/// Extract the action name from a `SOAPACTION` header value
/// (`"urn:…:AVTransport:1#Play"` → `Play`).
fn action_name(soap_action: &str) -> String {
    soap_action
        .trim_matches('"')
        .rsplit('#')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The SOAP response body for one action, reading scripted state for the polls.
fn response_for(action: &str, state: &FakeRenderer) -> String {
    let inner = match action {
        "GetTransportInfo" => format!(
            "<CurrentTransportState>{}</CurrentTransportState>\
             <CurrentTransportStatus>OK</CurrentTransportStatus><CurrentSpeed>1</CurrentSpeed>",
            state.transport_state
        ),
        "GetPositionInfo" => format!(
            "<Track>1</Track><TrackDuration>{}</TrackDuration><RelTime>{}</RelTime>",
            state.track_duration, state.rel_time
        ),
        _ => String::new(),
    };
    format!(
        "<?xml version=\"1.0\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
         <u:{action}Response xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">{inner}\
         </u:{action}Response></s:Body></s:Envelope>"
    )
}

/// Start a fake renderer on the given runtime, returning its base URL and shared
/// state. One handler serves both control URLs — the action is in the header.
async fn start_fake_renderer(state: Shared) -> String {
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::Router;

    async fn handle(State(state): State<Shared>, headers: HeaderMap, _body: String) -> String {
        let action = headers
            .get("SOAPACTION")
            .and_then(|v| v.to_str().ok())
            .map(action_name)
            .unwrap_or_default();
        let mut guard = state.lock().unwrap();
        guard.actions.push(action.clone());
        response_for(&action, &guard)
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/avt", post(handle))
        .route("/rc", post(handle))
        .with_state(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    format!("http://{addr}")
}

fn test_media() -> RendererMedia {
    RendererMedia {
        url: "http://source/stream?id=t1".to_string(),
        content_type: "audio/flac".to_string(),
        title: "Title".to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        cover_url: None,
        duration: Some(Duration::from_secs(180)),
    }
}

fn channel_to(base: &str) -> DlnaChannel {
    DlnaChannel::connect(format!("{base}/avt"), Some(format!("{base}/rc"))).unwrap()
}

/// Every transport command reaches the renderer as its SOAP action. A load is a
/// `SetAVTransportURI` followed by `Play`; volume goes to RenderingControl.
#[test]
fn channel_routes_each_command_to_its_soap_action() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let state: Shared = Arc::new(Mutex::new(FakeRenderer::default()));
    let base = runtime.block_on(start_fake_renderer(state.clone()));

    let mut channel = channel_to(&base);
    channel.load(&test_media()).unwrap();
    channel.pause().unwrap();
    channel.seek(Duration::from_secs(42)).unwrap();
    channel.set_volume(0.5).unwrap();
    channel.play().unwrap();
    channel.stop().unwrap();

    let actions = state.lock().unwrap().actions.clone();
    assert_eq!(
        actions,
        vec![
            "SetAVTransportURI",
            "Play",
            "Pause",
            "Seek",
            "SetVolume",
            "Play",
            "Stop",
        ],
        "each command must reach the renderer as its SOAP action"
    );
}

/// A renderer that plays through to (near) the track's end and then reports
/// STOPPED is a natural end: the channel surfaces `Finished` for the queue to
/// advance on.
#[test]
fn stopped_after_playing_through_is_finished() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let state: Shared = Arc::new(Mutex::new(FakeRenderer {
        transport_state: "PLAYING".to_string(),
        rel_time: "0:03:00".to_string(),
        track_duration: "0:03:00".to_string(),
        ..FakeRenderer::default()
    }));
    let base = runtime.block_on(start_fake_renderer(state.clone()));
    let mut channel = channel_to(&base);

    // Playing at the end of the track.
    let playing = channel.poll_status().unwrap();
    assert_eq!(playing.player_state, RendererPlayerState::Playing);

    // The renderer stops after playing through.
    state.lock().unwrap().transport_state = "STOPPED".to_string();
    let stopped = channel.poll_status().unwrap();
    assert_eq!(
        stopped.player_state,
        RendererPlayerState::Finished,
        "STOPPED after playing through must be the queue-advance signal"
    );

    // It fires exactly once — a later poll while still STOPPED is idle.
    let again = channel.poll_status().unwrap();
    assert_eq!(again.player_state, RendererPlayerState::Idle);
}

/// The stop() bug guard: our own `stop()` produces a STOPPED that must NOT be
/// read as an end-of-track advance, even though the renderer had played.
#[test]
fn our_own_stop_is_not_read_as_finished() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let state: Shared = Arc::new(Mutex::new(FakeRenderer {
        transport_state: "PLAYING".to_string(),
        rel_time: "0:03:00".to_string(),
        track_duration: "0:03:00".to_string(),
        ..FakeRenderer::default()
    }));
    let base = runtime.block_on(start_fake_renderer(state.clone()));
    let mut channel = channel_to(&base);

    assert_eq!(
        channel.poll_status().unwrap().player_state,
        RendererPlayerState::Playing
    );

    // We stop it; the renderer goes STOPPED as a result.
    channel.stop().unwrap();
    state.lock().unwrap().transport_state = "STOPPED".to_string();

    assert_eq!(
        channel.poll_status().unwrap().player_state,
        RendererPlayerState::Idle,
        "a STOPPED that follows our own stop() must not advance the queue"
    );
}

/// A STOPPED reached mid-track (not near the end — e.g. someone pressed stop on
/// the device's own remote) is idle, not an end-of-track advance.
#[test]
fn stopped_mid_track_is_idle_not_finished() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let state: Shared = Arc::new(Mutex::new(FakeRenderer {
        transport_state: "PLAYING".to_string(),
        rel_time: "0:00:30".to_string(),
        track_duration: "0:03:00".to_string(),
        ..FakeRenderer::default()
    }));
    let base = runtime.block_on(start_fake_renderer(state.clone()));
    let mut channel = channel_to(&base);

    assert_eq!(
        channel.poll_status().unwrap().player_state,
        RendererPlayerState::Playing
    );
    state.lock().unwrap().transport_state = "STOPPED".to_string();
    assert_eq!(
        channel.poll_status().unwrap().player_state,
        RendererPlayerState::Idle,
        "a STOPPED well before the end is not a natural end"
    );
}

/// A poll against an unreachable renderer is a terminal connection error, which
/// the session reads as the remote session ending (resume local).
#[test]
fn poll_of_unreachable_renderer_is_a_connection_error() {
    // Bind then drop a listener to get a port nothing is serving.
    let dead_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mut channel =
        DlnaChannel::connect(format!("http://127.0.0.1:{dead_port}/avt"), None).unwrap();
    assert!(
        matches!(channel.poll_status(), Err(RendererError::Connection(_))),
        "an unreachable renderer must surface as a terminal connection error"
    );
}

// -- end-of-track inference: position-less and zero-duration renderers ---------

/// Drive a channel to PLAYING (recording whatever position the renderer reports),
/// then flip it to STOPPED and return the state the next poll classifies it as.
fn stopped_state_after(rel_time: &str, track_duration: &str) -> RendererPlayerState {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let state: Shared = Arc::new(Mutex::new(FakeRenderer {
        transport_state: "PLAYING".to_string(),
        rel_time: rel_time.to_string(),
        track_duration: track_duration.to_string(),
        ..FakeRenderer::default()
    }));
    let base = runtime.block_on(start_fake_renderer(state.clone()));
    let mut channel = channel_to(&base);
    // Observe PLAYING so has_played is set and any position is recorded.
    assert_eq!(
        channel.poll_status().unwrap().player_state,
        RendererPlayerState::Playing
    );
    state.lock().unwrap().transport_state = "STOPPED".to_string();
    channel.poll_status().unwrap().player_state
}

/// A renderer that never reports a position (`RelTime: NOT_IMPLEMENTED`, a
/// documented quirk) still auto-advances on STOPPED: we can't tell a natural end
/// from a device-remote stop, and working auto-advance is the better default.
#[test]
fn stop_without_any_reported_position_is_finished() {
    assert_eq!(
        stopped_state_after("NOT_IMPLEMENTED", "NOT_IMPLEMENTED"),
        RendererPlayerState::Finished,
        "a position-less renderer must still advance the queue on STOPPED"
    );
}

/// While loading, renderers commonly report `TrackDuration=0:00:00`. Once a real
/// position has been seen, a STOPPED must NOT read as end-of-track off that fake
/// zero duration.
#[test]
fn zero_duration_with_reported_position_is_not_finished() {
    assert_eq!(
        stopped_state_after("0:00:30", "0:00:00"),
        RendererPlayerState::Idle,
        "a zero TrackDuration must not make a mid-track stop look finished"
    );
}

/// The 5s end-of-track slack: within 5s of the reported duration is a natural
/// end; further out is a mid-track stop.
#[test]
fn end_of_track_respects_the_five_second_slack() {
    // duration − 4s → within slack → finished.
    assert_eq!(
        stopped_state_after("0:02:56", "0:03:00"),
        RendererPlayerState::Finished,
        "4s short of the end is a natural end"
    );
    // duration − 6s → outside slack → mid-track stop.
    assert_eq!(
        stopped_state_after("0:02:54", "0:03:00"),
        RendererPlayerState::Idle,
        "6s short of the end is not a natural end"
    );
}

/// A zero `TrackDuration` (reported while a track loads) is unknown, not a real
/// zero-length track; a zero `RelTime` is a genuine position 0.
#[test]
fn zero_track_duration_parses_as_unknown() {
    let body = r#"<u:GetPositionInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <TrackDuration>0:00:00</TrackDuration>
      <RelTime>0:00:00</RelTime>
    </u:GetPositionInfoResponse>"#;
    let info = parse_position_info(body);
    assert_eq!(
        info.rel_time,
        Some(Duration::ZERO),
        "a zero RelTime is a real position, not absent"
    );
    assert_eq!(
        info.track_duration, None,
        "a zero TrackDuration is unknown (still loading), not a real duration"
    );
}
