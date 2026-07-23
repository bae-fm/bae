//! The UPnP SOAP action layer: envelopes out, responses in.
//!
//! A UPnP MediaRenderer is controlled by SOAP over HTTP — each action is a POST
//! of an XML envelope to a service's control URL, with a `SOAPACTION` header
//! naming the action. This module builds those envelopes (AVTransport's
//! `SetAVTransportURI`/`Play`/`Pause`/`Seek`/`Stop`, RenderingControl's
//! `SetVolume`, and the `GetPositionInfo`/`GetTransportInfo` polls) and parses
//! the two responses the poll reads. It is pure string work — the blocking HTTP
//! POST that carries an envelope is the caller's, so the envelope construction
//! and response parsing here are testable without a network.

use std::time::Duration;

/// The AVTransport service type, named in every AVTransport `SOAPACTION` header
/// and in the `u:` namespace of its action elements.
pub const AV_TRANSPORT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
/// The RenderingControl service type, for `SetVolume`.
pub const RENDERING_CONTROL: &str = "urn:schemas-upnp-org:service:RenderingControl:1";

/// One built SOAP request: the `SOAPACTION` header value and the envelope body
/// to POST. The control URL it goes to is the caller's (it differs per service).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoapRequest {
    /// The `SOAPACTION` header value, quoted: `"<service-type>#<action>"`.
    pub soap_action: String,
    /// The full `<s:Envelope>` XML to send as the request body.
    pub body: String,
}

/// The metadata `SetAVTransportURI` carries as a DIDL-Lite document: what the
/// renderer shows on-screen, plus the `protocolInfo` MIME that tells it how to
/// decode the stream. Mirrors the fields a Cast LOAD carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DidlMetadata<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    /// Cover-art URL, shown by renderers with a screen. `None` when absent.
    pub cover_url: Option<&'a str>,
    /// The MIME type of the served bytes (`audio/flac`, `audio/mpeg`, …), placed
    /// in the `<res>` `protocolInfo` so the renderer knows the codec.
    pub content_type: &'a str,
}

/// `SetAVTransportURI(InstanceID=0, CurrentURI=url, CurrentURIMetaData=<DIDL>)`.
/// The DIDL document is XML-escaped as it is embedded, since it rides as text
/// inside the envelope.
pub fn set_av_transport_uri(url: &str, metadata: &DidlMetadata) -> SoapRequest {
    let didl = didl_lite(url, metadata);
    let arguments = format!(
        "<InstanceID>0</InstanceID>\
         <CurrentURI>{}</CurrentURI>\
         <CurrentURIMetaData>{}</CurrentURIMetaData>",
        xml_escape(url),
        xml_escape(&didl),
    );
    av_transport_action("SetAVTransportURI", &arguments)
}

/// `Play(InstanceID=0, Speed=1)`.
pub fn play() -> SoapRequest {
    av_transport_action("Play", "<InstanceID>0</InstanceID><Speed>1</Speed>")
}

/// `Pause(InstanceID=0)`.
pub fn pause() -> SoapRequest {
    av_transport_action("Pause", "<InstanceID>0</InstanceID>")
}

/// `Stop(InstanceID=0)`.
pub fn stop() -> SoapRequest {
    av_transport_action("Stop", "<InstanceID>0</InstanceID>")
}

/// `Seek(InstanceID=0, Unit=REL_TIME, Target=H:MM:SS)`.
pub fn seek(position: Duration) -> SoapRequest {
    let arguments = format!(
        "<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>{}</Target>",
        format_hms(position),
    );
    av_transport_action("Seek", &arguments)
}

/// `GetTransportInfo(InstanceID=0)` — the poll that reads `CurrentTransportState`.
pub fn get_transport_info() -> SoapRequest {
    av_transport_action("GetTransportInfo", "<InstanceID>0</InstanceID>")
}

/// `GetPositionInfo(InstanceID=0)` — the poll that reads `RelTime`/`TrackDuration`.
pub fn get_position_info() -> SoapRequest {
    av_transport_action("GetPositionInfo", "<InstanceID>0</InstanceID>")
}

/// `SetVolume(InstanceID=0, Channel=Master, DesiredVolume=0..=100)` on
/// RenderingControl. `level` (0.0–1.0) is clamped and scaled to the UPnP 0–100
/// integer range.
pub fn set_volume(level: f32) -> SoapRequest {
    let percent = (level.clamp(0.0, 1.0) * 100.0).round() as u32;
    let arguments = format!(
        "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>{percent}</DesiredVolume>"
    );
    action(RENDERING_CONTROL, "SetVolume", &arguments)
}

/// Build an AVTransport action envelope.
fn av_transport_action(name: &str, arguments: &str) -> SoapRequest {
    action(AV_TRANSPORT, name, arguments)
}

/// Build a SOAP envelope for `action_name` on `service_type` with the given
/// already-rendered argument elements.
fn action(service_type: &str, action_name: &str, arguments: &str) -> SoapRequest {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
         <s:Body>\
         <u:{action_name} xmlns:u=\"{service_type}\">{arguments}</u:{action_name}>\
         </s:Body>\
         </s:Envelope>"
    );
    SoapRequest {
        soap_action: format!("\"{service_type}#{action_name}\""),
        body,
    }
}

/// The DIDL-Lite document embedded in `SetAVTransportURI`'s metadata argument.
/// One `musicTrack` item whose `<res>` carries the stream URL and its
/// `protocolInfo` MIME. Text fields are XML-escaped.
fn didl_lite(url: &str, metadata: &DidlMetadata) -> String {
    let cover = match metadata.cover_url {
        Some(cover_url) => format!(
            "<upnp:albumArtURI>{}</upnp:albumArtURI>",
            xml_escape(cover_url)
        ),
        None => String::new(),
    };
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">\
         <item id=\"0\" parentID=\"-1\" restricted=\"1\">\
         <dc:title>{title}</dc:title>\
         <dc:creator>{artist}</dc:creator>\
         <upnp:artist>{artist}</upnp:artist>\
         <upnp:album>{album}</upnp:album>\
         {cover}\
         <upnp:class>object.item.audioItem.musicTrack</upnp:class>\
         <res protocolInfo=\"http-get:*:{content_type}:*\">{url}</res>\
         </item>\
         </DIDL-Lite>",
        title = xml_escape(metadata.title),
        artist = xml_escape(metadata.artist),
        album = xml_escape(metadata.album),
        content_type = xml_escape(metadata.content_type),
        url = xml_escape(url),
    )
}

/// Escape the five XML special characters so arbitrary text (titles, URLs, and a
/// whole embedded DIDL document) is safe inside element content and attributes.
/// UTF-8 (unicode) passes through unchanged.
pub fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Render a duration as UPnP `H:MM:SS` (the `REL_TIME` seek target format).
fn format_hms(position: Duration) -> String {
    let secs = position.as_secs();
    format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

/// What `GetPositionInfo` reports: the current position into the track and the
/// track's duration, each absent when the renderer omits it or returns the
/// `NOT_IMPLEMENTED` / zero placeholders many use before playback starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PositionInfo {
    pub rel_time: Option<Duration>,
    pub track_duration: Option<Duration>,
}

/// The renderer's transport state from `GetTransportInfo`'s
/// `CurrentTransportState`. `Other` carries any value outside the set we act on,
/// so an unknown state is a plain unknown rather than a misread of a known one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Playing,
    PausedPlayback,
    Stopped,
    Transitioning,
    NoMediaPresent,
    Other,
}

/// Parse `GetPositionInfo`'s response into position and duration. A missing,
/// `NOT_IMPLEMENTED`, or unparseable time is carried as `None`.
pub fn parse_position_info(body: &str) -> PositionInfo {
    PositionInfo {
        rel_time: element_text(body, "RelTime").and_then(|t| parse_hms(&t)),
        track_duration: element_text(body, "TrackDuration").and_then(|t| parse_hms(&t)),
    }
}

/// Parse `GetTransportInfo`'s response into a [`TransportState`]. A response
/// without the element is treated as `Other` (unknown), never as a known state.
pub fn parse_transport_state(body: &str) -> TransportState {
    match element_text(body, "CurrentTransportState").as_deref() {
        Some("PLAYING") => TransportState::Playing,
        Some("PAUSED_PLAYBACK") | Some("PAUSED_RECORDING") => TransportState::PausedPlayback,
        Some("STOPPED") => TransportState::Stopped,
        Some("TRANSITIONING") => TransportState::Transitioning,
        Some("NO_MEDIA_PRESENT") => TransportState::NoMediaPresent,
        _ => TransportState::Other,
    }
}

/// The text of the first element with local name `local_name` anywhere in `xml`,
/// ignoring XML namespaces (UPnP action arguments are unprefixed, but the
/// wrapping action element is `u:`-prefixed, so matching on the local name is
/// what reads both). `None` when the element is absent or the XML doesn't parse.
fn element_text(xml: &str, local_name: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    doc.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == local_name)
        .and_then(|node| node.text())
        .map(str::to_string)
}

/// Parse a UPnP `H:MM:SS` (or `HH:MM:SS`, with an optional fractional-seconds
/// suffix) time into a duration. `NOT_IMPLEMENTED`, empty, and malformed values
/// yield `None`.
fn parse_hms(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() || value == "NOT_IMPLEMENTED" {
        return None;
    }
    let mut parts = value.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds_field = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    // Seconds may carry a fractional part (`SS.mmm`); take the whole-second part.
    let seconds: u64 = seconds_field
        .split_once('.')
        .map(|(whole, _frac)| whole)
        .unwrap_or(seconds_field)
        .parse()
        .ok()?;
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    Some(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
}
