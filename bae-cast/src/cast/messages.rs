//! The JSON a Cast frame carries, by namespace.
//!
//! [`Request`] is what bae sends. [`Inbound::from_frame`] reads what a
//! receiver sends: by namespace, then by `type`. A frame of a namespace bae
//! doesn't use, or of a type it doesn't know, is [`Inbound::Ignored`]. A frame
//! of a type it knows whose body can't be read is an error: going on would
//! leave what the client holds wrong.

use std::time::Duration;

use bae_playback::renderer::{RendererMedia, RendererPlayerState};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

use super::wire::{CastMessage, PayloadType};

pub(crate) const NAMESPACE_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub(crate) const NAMESPACE_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub(crate) const NAMESPACE_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub(crate) const NAMESPACE_MEDIA: &str = "urn:x-cast:com.google.cast.media";

/// A request or notice bae sends. Requests that expect an answer carry the
/// `request_id` the answer echoes.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Request<'a> {
    /// Open a virtual connection to the message's destination.
    Connect,
    /// Close the virtual connection to the message's destination.
    Close,
    Ping,
    Pong,
    ReceiverStatus {
        request_id: u32,
    },
    Launch {
        request_id: u32,
        app_id: &'a str,
    },
    SetVolume {
        request_id: u32,
        level: f32,
    },
    MediaStatus {
        request_id: u32,
    },
    Load {
        request_id: u32,
        session_id: &'a str,
        media: &'a RendererMedia,
    },
    Play {
        request_id: u32,
        media_session_id: i32,
    },
    Pause {
        request_id: u32,
        media_session_id: i32,
    },
    Seek {
        request_id: u32,
        media_session_id: i32,
        position: Duration,
    },
    StopMedia {
        request_id: u32,
        media_session_id: i32,
    },
}

impl Request<'_> {
    pub(crate) fn namespace(&self) -> &'static str {
        match self {
            Request::Connect | Request::Close => NAMESPACE_CONNECTION,
            Request::Ping | Request::Pong => NAMESPACE_HEARTBEAT,
            Request::ReceiverStatus { .. } | Request::Launch { .. } | Request::SetVolume { .. } => {
                NAMESPACE_RECEIVER
            }
            Request::MediaStatus { .. }
            | Request::Load { .. }
            | Request::Play { .. }
            | Request::Pause { .. }
            | Request::Seek { .. }
            | Request::StopMedia { .. } => NAMESPACE_MEDIA,
        }
    }

    pub(crate) fn payload(&self) -> Value {
        match *self {
            Request::Connect => json!({"type": "CONNECT", "origin": {}, "userAgent": "bae"}),
            Request::Close => json!({"type": "CLOSE"}),
            Request::Ping => json!({"type": "PING"}),
            Request::Pong => json!({"type": "PONG"}),
            Request::ReceiverStatus { request_id } => {
                json!({"type": "GET_STATUS", "requestId": request_id})
            }
            Request::Launch { request_id, app_id } => {
                json!({"type": "LAUNCH", "requestId": request_id, "appId": app_id})
            }
            Request::SetVolume { request_id, level } => {
                json!({"type": "SET_VOLUME", "requestId": request_id, "volume": {"level": level}})
            }
            Request::MediaStatus { request_id } => {
                json!({"type": "GET_STATUS", "requestId": request_id})
            }
            Request::Load { request_id, session_id, media } => json!({
                "type": "LOAD",
                "requestId": request_id,
                "sessionId": session_id,
                "media": load_media(media),
                "autoplay": true,
                "currentTime": 0,
            }),
            Request::Play { request_id, media_session_id } => {
                json!({"type": "PLAY", "requestId": request_id, "mediaSessionId": media_session_id})
            }
            Request::Pause { request_id, media_session_id } => json!({
                "type": "PAUSE", "requestId": request_id, "mediaSessionId": media_session_id,
            }),
            Request::Seek { request_id, media_session_id, position } => json!({
                "type": "SEEK",
                "requestId": request_id,
                "mediaSessionId": media_session_id,
                "currentTime": position.as_secs_f64(),
            }),
            Request::StopMedia { request_id, media_session_id } => json!({
                "type": "STOP", "requestId": request_id, "mediaSessionId": media_session_id,
            }),
        }
    }

    /// The frame that carries this request from `source_id` to
    /// `destination_id`.
    pub(crate) fn into_frame(self, source_id: &str, destination_id: &str) -> CastMessage {
        CastMessage::text(source_id, destination_id, self.namespace(), self.payload().to_string())
    }
}

/// The `media` of a LOAD: a music track the receiver fetches from `url`.
fn load_media(media: &RendererMedia) -> Value {
    let images: Vec<Value> = media.cover_url.iter().map(|url| json!({"url": url})).collect();
    let mut loaded = json!({
        "contentId": media.url,
        "contentType": media.content_type,
        "streamType": "BUFFERED",
        "metadata": {
            "metadataType": 3,
            "title": media.title,
            "albumName": media.album,
            "artist": media.artist,
            "albumArtist": media.artist,
            "images": images,
        },
    });
    if let Some(duration) = media.duration {
        loaded["duration"] = json!(duration.as_secs_f64());
    }
    loaded
}

/// A frame a receiver sent, read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inbound {
    Ping,
    Pong,
    /// The sender of the frame closed its virtual connection to bae.
    Close {
        source_id: String,
    },
    ReceiverStatus {
        request_id: u32,
        status: ReceiverStatusBody,
    },
    LaunchStatus {
        request_id: u32,
        status: Option<String>,
    },
    LaunchError {
        request_id: u32,
        reason: Option<String>,
        extended_error: Option<String>,
    },
    MediaStatus {
        request_id: u32,
        entries: Vec<MediaStatusEntry>,
    },
    /// The receiver refused or failed the request with this id.
    Refused(Refusal),
    /// A frame bae doesn't use: an unused namespace, a type it doesn't know,
    /// or a payload with no `type` at all.
    Ignored {
        namespace: String,
        kind: Option<String>,
    },
}

/// Why the receiver won't do a request: a message type that says so, with the
/// reason or code it gave.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Refusal {
    pub(crate) request_id: u32,
    pub(crate) kind: String,
    pub(crate) reason: Option<String>,
    pub(crate) detailed_error_code: Option<i64>,
}

impl Refusal {
    /// What to tell the person: the type, then the reason and code it gave.
    pub(crate) fn describe(&self) -> String {
        let mut text = self.kind.clone();
        if let Some(reason) = &self.reason {
            text.push_str(&format!(" ({reason})"));
        }
        if let Some(code) = self.detailed_error_code {
            text.push_str(&format!(" (code {code})"));
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct ReceiverStatusBody {
    /// The running apps. A receiver with none running leaves the field out.
    #[serde(default)]
    pub(crate) applications: Vec<Application>,
    pub(crate) volume: Option<ReceiverVolume>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Application {
    pub(crate) app_id: String,
    pub(crate) session_id: String,
    pub(crate) transport_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct ReceiverVolume {
    pub(crate) level: Option<f64>,
}

/// One media session's entry of a MEDIA_STATUS.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaStatusEntry {
    pub(crate) media_session_id: i32,
    pub(crate) player_state: PlayerState,
    /// Why the player is idle. The receiver drops it once it no longer applies.
    #[serde(default)]
    pub(crate) idle_reason: Option<IdleReason>,
    /// Seconds into the media. Anything but a number is not a position.
    #[serde(default, deserialize_with = "number_or_none")]
    pub(crate) current_time: Option<f64>,
    /// How fast the position moves while playing. Anything but a number is no
    /// rate.
    #[serde(default, deserialize_with = "number_or_none")]
    pub(crate) playback_rate: Option<f64>,
    /// The media, which the receiver sends only when it changed.
    #[serde(default)]
    pub(crate) media: Option<MediaInfo>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct MediaInfo {
    /// Seconds. `null`, or anything but a number, is no duration.
    #[serde(default, deserialize_with = "number_or_none")]
    pub(crate) duration: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum PlayerState {
    Idle,
    Playing,
    Paused,
    Buffering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum IdleReason {
    Cancelled,
    Interrupted,
    Finished,
    Error,
}

impl MediaStatusEntry {
    /// What the player is doing. Idle for reason FINISHED is the track's end;
    /// every other idle, whatever the reason or none, is a plain idle.
    pub(crate) fn renderer_state(&self) -> RendererPlayerState {
        match (self.player_state, self.idle_reason) {
            (PlayerState::Playing, _) => RendererPlayerState::Playing,
            (PlayerState::Paused, _) => RendererPlayerState::Paused,
            (PlayerState::Buffering, _) => RendererPlayerState::Buffering,
            (PlayerState::Idle, Some(IdleReason::Finished)) => RendererPlayerState::Finished,
            (PlayerState::Idle, _) => RendererPlayerState::Idle,
        }
    }
}

fn number_or_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    Ok(Option::<Value>::deserialize(deserializer)?.and_then(|value| value.as_f64()))
}

/// A frame of a namespace bae reads that can't be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MessageError {
    #[error("a {namespace} frame carries a binary payload")]
    BinaryPayload { namespace: String },
    #[error("a {namespace} frame carries no text payload")]
    NoPayload { namespace: String },
    #[error("a {namespace} payload is not JSON: {source}")]
    NotJson { namespace: String, source: serde_json::Error },
    #[error("a {kind} message in {namespace} can't be read: {source}")]
    Unreadable { namespace: String, kind: String, source: serde_json::Error },
}

impl Inbound {
    pub(crate) fn from_frame(frame: &CastMessage) -> Result<Self, MessageError> {
        let namespace = frame.namespace.as_str();
        if !matches!(
            namespace,
            NAMESPACE_CONNECTION | NAMESPACE_HEARTBEAT | NAMESPACE_RECEIVER | NAMESPACE_MEDIA
        ) {
            return Ok(Inbound::Ignored { namespace: frame.namespace.clone(), kind: None });
        }
        let owned = || namespace.to_string();
        if frame.payload_type == PayloadType::Binary as i32 {
            return Err(MessageError::BinaryPayload { namespace: owned() });
        }
        let Some(text) = frame.payload_utf8.as_deref() else {
            return Err(MessageError::NoPayload { namespace: owned() });
        };
        let payload: Value = serde_json::from_str(text)
            .map_err(|source| MessageError::NotJson { namespace: owned(), source })?;
        let Some(kind) = payload.get("type").and_then(Value::as_str).map(str::to_string) else {
            return Ok(Inbound::Ignored { namespace: owned(), kind: None });
        };
        let unreadable =
            |source| MessageError::Unreadable { namespace: owned(), kind: kind.clone(), source };
        let ignored = || Inbound::Ignored { namespace: owned(), kind: Some(kind.clone()) };
        // A spontaneous status carries no request id, which reads as 0. So does
        // one that isn't a number bae could have sent.
        let request_id = payload
            .get("requestId")
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .unwrap_or(0);
        let refusal = |payload: &Value| Refusal {
            request_id,
            kind: kind.clone(),
            reason: payload.get("reason").and_then(Value::as_str).map(str::to_string),
            detailed_error_code: payload.get("detailedErrorCode").and_then(Value::as_i64),
        };
        let text_field = |name: &str| payload.get(name).and_then(Value::as_str).map(str::to_string);
        Ok(match (namespace, kind.as_str()) {
            (NAMESPACE_HEARTBEAT, "PING") => Inbound::Ping,
            (NAMESPACE_HEARTBEAT, "PONG") => Inbound::Pong,
            (NAMESPACE_CONNECTION, "CLOSE") => {
                Inbound::Close { source_id: frame.source_id.clone() }
            }
            (NAMESPACE_RECEIVER, "RECEIVER_STATUS") => {
                let status = payload.get("status").cloned().unwrap_or_else(|| json!({}));
                Inbound::ReceiverStatus {
                    request_id,
                    status: serde_json::from_value(status).map_err(unreadable)?,
                }
            }
            (NAMESPACE_RECEIVER, "LAUNCH_STATUS") => {
                Inbound::LaunchStatus { request_id, status: text_field("status") }
            }
            (NAMESPACE_RECEIVER, "LAUNCH_ERROR") => Inbound::LaunchError {
                request_id,
                reason: text_field("reason"),
                extended_error: text_field("extendedError"),
            },
            (NAMESPACE_RECEIVER, "INVALID_REQUEST") => Inbound::Refused(refusal(&payload)),
            (NAMESPACE_MEDIA, "MEDIA_STATUS") => {
                let entries = payload.get("status").cloned().unwrap_or_else(|| json!([]));
                Inbound::MediaStatus {
                    request_id,
                    entries: serde_json::from_value(entries).map_err(unreadable)?,
                }
            }
            (
                NAMESPACE_MEDIA,
                "LOAD_FAILED"
                | "LOAD_CANCELLED"
                | "INVALID_PLAYER_STATE"
                | "INVALID_REQUEST"
                | "ERROR",
            ) => Inbound::Refused(refusal(&payload)),
            _ => ignored(),
        })
    }
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;
