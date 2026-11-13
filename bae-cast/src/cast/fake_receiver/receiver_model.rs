//! What a Default Media Receiver says, for the fake receiver.
//!
//! The model holds the running app, the media sessions and the player. Given
//! a request it returns the messages the receiver sends back, as the study of
//! other Cast senders' tests records them: LAUNCH is answered by
//! LAUNCH_STATUS `USER_ALLOWED` then RECEIVER_STATUS; LOAD, PLAY, PAUSE, SEEK
//! and STOP by a MEDIA_STATUS carrying the request id; SET_VOLUME and
//! GET_STATUS by RECEIVER_STATUS.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::super::messages::{NAMESPACE_CONNECTION, NAMESPACE_MEDIA, NAMESPACE_RECEIVER};

/// The Default Media Receiver's app id.
const APP_ID: &str = "CC1AD845";

/// A message the receiver sends: from, to, namespace, payload.
pub(super) struct Outgoing {
    pub(super) source_id: String,
    pub(super) destination_id: String,
    pub(super) namespace: &'static str,
    pub(super) payload: Value,
}

/// One message of a scripted answer.
#[derive(Clone)]
pub(crate) enum Reply {
    /// LAUNCH_STATUS with this status.
    LaunchStatus(&'static str),
    /// LAUNCH_ERROR.
    LaunchError { reason: &'static str, extended_error: Option<&'static str> },
    /// The RECEIVER_STATUS of a launched app, launching it if it isn't running.
    ReceiverStatusWithApp,
    /// A message as it is, with the request's id added if it carries none.
    Raw { namespace: &'static str, payload: Value },
}

/// What the receiver does with the next request of one type.
#[derive(Clone)]
pub(crate) enum Script {
    /// Say nothing.
    NoAnswer,
    /// Send these, in order, in place of its usual answer.
    Replies(Vec<Reply>),
}

struct App {
    session_id: String,
    transport_id: String,
}

pub(super) struct Model {
    app: Option<App>,
    apps_launched: u32,
    media_session_id: Option<i32>,
    media_sessions: i32,
    player_state: &'static str,
    idle_reason: Option<&'static str>,
    current_time: f64,
    duration: Option<f64>,
    volume: f64,
    scripts: HashMap<String, Script>,
}

impl Model {
    pub(super) fn new() -> Self {
        Self {
            app: None,
            apps_launched: 0,
            media_session_id: None,
            media_sessions: 0,
            player_state: "IDLE",
            idle_reason: None,
            current_time: 0.0,
            duration: None,
            volume: 1.0,
            scripts: HashMap::new(),
        }
    }

    pub(super) fn script(&mut self, kind: &str, script: Script) {
        self.scripts.insert(kind.to_string(), script);
    }

    fn transport_id(&self) -> String {
        self.app.as_ref().map_or_else(|| "receiver-0".to_string(), |a| a.transport_id.clone())
    }

    fn receiver_status(&self, request_id: u64) -> Value {
        let applications: Vec<Value> = self
            .app
            .iter()
            .map(|app| {
                json!({
                    "appId": APP_ID,
                    "displayName": "Default Media Receiver",
                    "sessionId": app.session_id,
                    "transportId": app.transport_id,
                    "namespaces": [{"name": "urn:x-cast:com.google.cast.media"}],
                })
            })
            .collect();
        let mut status = json!({"volume": {"level": self.volume, "muted": false}});
        if !applications.is_empty() {
            status["applications"] = json!(applications);
        }
        json!({"type": "RECEIVER_STATUS", "requestId": request_id, "status": status})
    }

    fn media_entry(&self, with_media: bool) -> Value {
        let mut entry = json!({
            "mediaSessionId": self.media_session_id,
            "playerState": self.player_state,
            "currentTime": self.current_time,
            "playbackRate": 1,
        });
        if let Some(reason) = self.idle_reason {
            entry["idleReason"] = json!(reason);
        }
        if with_media {
            entry["media"] = json!({"contentId": "stream", "duration": self.duration});
        }
        entry
    }

    fn media_status(&self, request_id: u64, with_media: bool) -> Value {
        let status = if self.media_session_id.is_some() {
            vec![self.media_entry(with_media)]
        } else {
            vec![]
        };
        json!({"type": "MEDIA_STATUS", "requestId": request_id, "status": status})
    }

    fn say(&self, namespace: &'static str, payload: Value, to: &str) -> Outgoing {
        let source_id = match namespace {
            NAMESPACE_MEDIA => self.transport_id(),
            _ => "receiver-0".to_string(),
        };
        Outgoing { source_id, destination_id: to.to_string(), namespace, payload }
    }

    fn launch(&mut self) {
        if self.app.is_none() {
            self.apps_launched += 1;
            self.app = Some(App {
                session_id: format!("session-{}", self.apps_launched),
                transport_id: format!("transport-{}", self.apps_launched),
            });
        }
    }

    fn reply(&mut self, reply: Reply, request_id: u64) -> Outgoing {
        match reply {
            Reply::LaunchStatus(status) => self.say(
                NAMESPACE_RECEIVER,
                json!({"type": "LAUNCH_STATUS", "requestId": request_id, "status": status}),
                "sender-0",
            ),
            Reply::LaunchError { reason, extended_error } => {
                let mut payload =
                    json!({"type": "LAUNCH_ERROR", "requestId": request_id, "reason": reason});
                if let Some(extended) = extended_error {
                    payload["extendedError"] = json!(extended);
                }
                self.say(NAMESPACE_RECEIVER, payload, "sender-0")
            }
            Reply::ReceiverStatusWithApp => {
                self.launch();
                let payload = self.receiver_status(request_id);
                self.say(NAMESPACE_RECEIVER, payload, "sender-0")
            }
            Reply::Raw { namespace, mut payload } => {
                if payload.get("requestId").is_none() {
                    payload["requestId"] = json!(request_id);
                }
                self.say(namespace, payload, "sender-0")
            }
        }
    }

    /// The messages the receiver sends back for `payload`, a request of
    /// `namespace`.
    pub(super) fn handle(&mut self, namespace: &str, payload: &Value) -> Vec<Outgoing> {
        let kind = payload.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let request_id = payload.get("requestId").and_then(Value::as_u64).unwrap_or(0);
        if let Some(script) = self.scripts.remove(&kind) {
            return match script {
                Script::NoAnswer => vec![],
                Script::Replies(replies) => {
                    replies.into_iter().map(|reply| self.reply(reply, request_id)).collect()
                }
            };
        }
        match (namespace, kind.as_str()) {
            (NAMESPACE_RECEIVER, "GET_STATUS") => {
                let payload = self.receiver_status(request_id);
                vec![self.say(NAMESPACE_RECEIVER, payload, "sender-0")]
            }
            (NAMESPACE_RECEIVER, "LAUNCH") => vec![
                self.reply(Reply::LaunchStatus("USER_ALLOWED"), request_id),
                self.reply(Reply::ReceiverStatusWithApp, request_id),
            ],
            (NAMESPACE_RECEIVER, "SET_VOLUME") => {
                if let Some(level) = payload["volume"]["level"].as_f64() {
                    self.volume = level;
                }
                let payload = self.receiver_status(request_id);
                vec![self.say(NAMESPACE_RECEIVER, payload, "sender-0")]
            }
            (NAMESPACE_MEDIA, "LOAD") => {
                self.media_sessions += 1;
                self.media_session_id = Some(self.media_sessions);
                self.player_state = "PLAYING";
                self.idle_reason = None;
                self.current_time = payload["currentTime"].as_f64().unwrap_or(0.0);
                self.duration = payload["media"]["duration"].as_f64();
                self.media_answer(request_id, true)
            }
            (NAMESPACE_MEDIA, "PLAY") => {
                self.player_state = "PLAYING";
                self.media_answer(request_id, false)
            }
            (NAMESPACE_MEDIA, "PAUSE") => {
                self.player_state = "PAUSED";
                self.media_answer(request_id, false)
            }
            (NAMESPACE_MEDIA, "SEEK") => {
                self.current_time = payload["currentTime"].as_f64().unwrap_or(0.0);
                self.media_answer(request_id, false)
            }
            (NAMESPACE_MEDIA, "STOP") => {
                self.player_state = "IDLE";
                self.idle_reason = Some("CANCELLED");
                self.media_answer(request_id, false)
            }
            (NAMESPACE_MEDIA, "GET_STATUS") => self.media_answer(request_id, false),
            _ => vec![],
        }
    }

    fn media_answer(&self, request_id: u64, with_media: bool) -> Vec<Outgoing> {
        let payload = self.media_status(request_id, with_media);
        vec![self.say(NAMESPACE_MEDIA, payload, "sender-0")]
    }

    /// A MEDIA_STATUS nobody asked for, from the model as it is.
    pub(super) fn broadcast_media_status(&self) -> Outgoing {
        self.say(NAMESPACE_MEDIA, self.media_status(0, false), "*")
    }

    /// A MEDIA_STATUS nobody asked for, carrying `entry` as it is.
    pub(super) fn push_media_status(&self, entry: Value) -> Outgoing {
        let payload = json!({"type": "MEDIA_STATUS", "requestId": 0, "status": [entry]});
        self.say(NAMESPACE_MEDIA, payload, "*")
    }

    /// The device's own buttons, or another sender, pause the player.
    pub(super) fn pause_by_another_sender(&mut self) -> Outgoing {
        self.player_state = "PAUSED";
        self.broadcast_media_status()
    }

    /// The track plays to its end.
    pub(super) fn finish_track(&mut self) -> Outgoing {
        self.player_state = "IDLE";
        self.idle_reason = Some("FINISHED");
        self.broadcast_media_status()
    }

    /// Another sender loads its own media: the old session is interrupted,
    /// then a new one plays.
    pub(super) fn load_by_another_sender(&mut self) -> Vec<Outgoing> {
        self.player_state = "IDLE";
        self.idle_reason = Some("INTERRUPTED");
        let interrupted = self.broadcast_media_status();
        self.media_sessions += 1;
        self.media_session_id = Some(self.media_sessions);
        self.player_state = "PLAYING";
        self.idle_reason = None;
        vec![interrupted, self.broadcast_media_status()]
    }

    /// The app ends: its RECEIVER_STATUS lists no applications.
    pub(super) fn stop_app(&mut self) -> Outgoing {
        self.app = None;
        self.media_session_id = None;
        let payload = self.receiver_status(0);
        self.say(NAMESPACE_RECEIVER, payload, "*")
    }

    /// Another app takes the receiver: a new session id, and the old
    /// transport id kept.
    pub(super) fn replace_app_keeping_transport(&mut self) -> Outgoing {
        self.apps_launched += 1;
        if let Some(app) = &mut self.app {
            app.session_id = format!("session-{}", self.apps_launched);
        }
        let payload = self.receiver_status(0);
        self.say(NAMESPACE_RECEIVER, payload, "*")
    }

    /// The app's transport closes its connection.
    pub(super) fn close_transport(&self) -> Outgoing {
        Outgoing {
            source_id: self.transport_id(),
            destination_id: "sender-0".to_string(),
            namespace: NAMESPACE_CONNECTION,
            payload: json!({"type": "CLOSE"}),
        }
    }
}
