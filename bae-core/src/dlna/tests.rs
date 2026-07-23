//! Discovery-parsing and SOAP wire tests, all against canned fixtures — no
//! network. The device-description fixtures carry the real-world quirks the
//! parser must survive: a missing `friendlyName`, a relative `controlURL`, and a
//! control URL naming another host and port.

use std::time::Duration;

use super::discovery::{parse_device_description, parse_ssdp_location};
use super::soap::{
    self, parse_position_info, parse_transport_state, DidlMetadata, PositionInfo, TransportState,
};

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
    assert_eq!(
        device.av_transport_url,
        "http://192.168.1.50:8080/desc/AVTransport/control"
    );
    assert_eq!(
        device.rendering_control_url.as_deref(),
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
    // An absolute-path controlURL resolves against the location's origin.
    assert_eq!(
        device.av_transport_url,
        "http://10.0.0.9:49152/upnp/control/AVTransport1"
    );
    // No RenderingControl service: volume is simply unsupported.
    assert!(device.rendering_control_url.is_none());
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
        device.av_transport_url, "http://192.168.1.77:9000/AVTransport/control",
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
        device.av_transport_url, "http://192.168.1.50:2870/ctl/avt",
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
