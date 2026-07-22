//! Parse the device-local `playback_state` resume cache at one boundary.
//!
//! The row is read back at startup as a single fallible parse: [`from_row`]
//! either yields a fully-valid [`PersistedPlayback`] or discards the whole cache
//! (logged once) on any out-of-domain value. No consumer downstream patches a
//! bad field — by the time the service holds a `PersistedPlayback`, every value
//! is already in range.

use tracing::warn;

use super::{ContextSnapshot, ContextSource, QueueSnapshot, RepeatMode};
use crate::db::DbPlaybackState;

/// The validated device-local resume cache: a queue snapshot plus the live
/// position / volume / mute the service replays on startup. Constructed only by
/// [`PersistedPlayback::from_row`], so holding one means the row parsed cleanly.
pub struct PersistedPlayback {
    pub queue: QueueSnapshot,
    pub position_ms: Option<u64>,
    pub volume: f32,
    pub is_muted: bool,
}

impl PersistedPlayback {
    /// Parse the raw DB row into a fully-valid shape, or `None` if the row is
    /// corrupt (the resume cache is then discarded — logged once). Every
    /// out-of-domain value discards the whole row rather than being patched: the
    /// row is our own write, so anything out of range means a corrupted local
    /// cache, and a partial restore is worse than a fresh start.
    pub fn from_row(row: DbPlaybackState) -> Option<PersistedPlayback> {
        let manual: Vec<String> = match serde_json::from_str(&row.manual) {
            Ok(manual) => manual,
            Err(e) => {
                warn!(
                    "discarding the playback resume cache: manual lane is not valid JSON ({e}): {:?}",
                    row.manual
                );
                return None;
            }
        };

        let repeat = match repeat_from_str(&row.repeat) {
            Ok(repeat) => repeat,
            Err(e) => {
                warn!(
                    "discarding the playback resume cache: unrecognized repeat {:?} ({e})",
                    row.repeat
                );
                return None;
            }
        };

        let context = match row.context {
            Some(ctx) => {
                // A malformed source discards the whole row; `source_from_str`
                // logs the specific reason before returning `None`.
                let source = source_from_str(ctx.source)?;
                Some(ContextSnapshot {
                    source,
                    shuffled: ctx.shuffled,
                })
            }
            None => None,
        };

        let position_ms = match row.position_ms {
            Some(position) => match u64::try_from(position) {
                Ok(position) => Some(position),
                Err(_) => {
                    warn!(
                        "discarding the playback resume cache: negative position {}",
                        position
                    );
                    return None;
                }
            },
            None => None,
        };

        Some(PersistedPlayback {
            queue: QueueSnapshot {
                context,
                manual,
                current_track_id: row.current_track_id,
                repeat,
            },
            position_ms,
            volume: row.volume,
            is_muted: row.is_muted,
        })
    }
}

/// The `playback_state.source` value standing for a library context. A release
/// `source` is a UUID, which can never equal this literal, so the two are
/// unambiguous in the column.
const LIBRARY_SOURCE: &str = "library";

/// Encode a [`ContextSource`] for the `playback_state.source` column: a release
/// stores its id, several releases store a JSON array of their ids, the library
/// the `LIBRARY_SOURCE` sentinel. The three are mutually unambiguous — a release
/// id is a UUID (never `[…]`, never the sentinel), a JSON array always starts
/// with `[`, and the sentinel is a fixed word. A present context always has a
/// `source`; "no context" is the NULL column, handled one level up.
pub fn source_to_str(source: &ContextSource) -> String {
    match source {
        ContextSource::Release(id) => id.clone(),
        ContextSource::Releases(ids) => {
            serde_json::to_string(ids).expect("release ids serialize to a JSON array")
        }
        ContextSource::Library => LIBRARY_SOURCE.to_string(),
    }
}

/// Decode the `playback_state.source` column back to a [`ContextSource`]: the
/// sentinel is the library, a `[…]` value is the JSON array of a multi-release
/// source, anything else is a bare release id. `None` discards the resume cache —
/// a value that opens like a JSON array but doesn't parse (or parses empty) is a
/// corrupt local write, not a release id.
fn source_from_str(source: String) -> Option<ContextSource> {
    if source == LIBRARY_SOURCE {
        return Some(ContextSource::Library);
    }
    if source.starts_with('[') {
        let ids: Vec<String> = match serde_json::from_str(&source) {
            Ok(ids) => ids,
            Err(e) => {
                warn!(
                    "discarding the playback resume cache: context source looks like a \
                     release-id array but is not valid JSON ({e}): {source:?}"
                );
                return None;
            }
        };
        if ids.is_empty() {
            warn!("discarding the playback resume cache: empty release-id array as context source");
            return None;
        }
        return Some(ContextSource::releases(ids));
    }
    Some(ContextSource::Release(source))
}

/// Serialize `RepeatMode` for the `playback_state.repeat` column.
pub fn repeat_to_str(mode: RepeatMode) -> String {
    match serde_json::to_value(mode).expect("RepeatMode serializes to JSON") {
        serde_json::Value::String(s) => s,
        other => panic!("RepeatMode serialized to a non-string JSON value: {other:?}"),
    }
}

/// Parse the `playback_state.repeat` column.
fn repeat_from_str(s: &str) -> Result<RepeatMode, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbPlaybackContext;

    /// A row with a real context, manual lane, and current track parses into the
    /// matching `PersistedPlayback`.
    fn valid_row() -> DbPlaybackState {
        DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: "rel-A".to_string(),
                shuffled: true,
            }),
            manual: r#"["m1","m2"]"#.to_string(),
            repeat: "context".to_string(),
            current_track_id: Some("t3".to_string()),
            position_ms: Some(42_000),
            volume: 0.5,
            is_muted: true,
        }
    }

    #[test]
    fn valid_row_round_trips() {
        let parsed = PersistedPlayback::from_row(valid_row()).expect("a valid row parses");
        let context = parsed.queue.context.expect("the context survives");
        assert_eq!(context.source, ContextSource::Release("rel-A".into()));
        assert!(context.shuffled);
        assert_eq!(parsed.queue.manual, vec!["m1", "m2"]);
        assert_eq!(parsed.queue.current_track_id.as_deref(), Some("t3"));
        assert_eq!(parsed.queue.repeat, RepeatMode::Context);
        assert_eq!(parsed.position_ms, Some(42_000));
        assert!((parsed.volume - 0.5).abs() < f32::EPSILON);
        assert!(parsed.is_muted);
    }

    /// The `shuffled` flag round-trips both ways — a sequential row is not
    /// silently read back as shuffled.
    #[test]
    fn shuffled_flag_round_trips_both_ways() {
        for shuffled in [false, true] {
            let row = DbPlaybackState {
                context: Some(DbPlaybackContext {
                    source: "rel-A".to_string(),
                    shuffled,
                }),
                ..valid_row()
            };
            let parsed = PersistedPlayback::from_row(row).expect("a valid row parses");
            let context = parsed.queue.context.expect("the context survives");
            assert_eq!(context.shuffled, shuffled);
        }
    }

    /// A row whose `source` is the library sentinel decodes back to the library
    /// source — the resume cache for "shuffle library" survives a restart.
    #[test]
    fn library_source_row_round_trips() {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: source_to_str(&ContextSource::Library),
                shuffled: true,
            }),
            ..valid_row()
        };
        let parsed = PersistedPlayback::from_row(row).expect("a valid row parses");
        let context = parsed.queue.context.expect("the context survives");
        assert_eq!(context.source, ContextSource::Library);
    }

    /// Every source kind survives the `playback_state.source` encode/decode: a
    /// release id is its own string, a multi-release source is a JSON array, the
    /// library is the sentinel, and none is mistaken for another (a release id is
    /// a UUID, never `[…]`, never the sentinel).
    #[test]
    fn source_encoding_round_trips() {
        for source in [
            ContextSource::Release("550e8400-e29b-41d4-a716-446655440000".into()),
            ContextSource::Releases(vec![
                "550e8400-e29b-41d4-a716-446655440000".into(),
                "6ba7b810-9dad-11d1-80b4-00c04fd430c8".into(),
            ]),
            ContextSource::Library,
        ] {
            assert_eq!(source_from_str(source_to_str(&source)), Some(source));
        }
    }

    /// A single-element release array collapses to `Release` on decode, matching
    /// the constructor invariant that `Releases` never holds one id.
    #[test]
    fn single_element_release_array_decodes_as_release() {
        let encoded = serde_json::to_string(&["only-one"]).unwrap();
        assert_eq!(
            source_from_str(encoded),
            Some(ContextSource::Release("only-one".into()))
        );
    }

    /// A source that opens like a JSON array but doesn't parse discards the row.
    #[test]
    fn malformed_source_array_discards_the_row() {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: "[not valid json".to_string(),
                shuffled: false,
            }),
            ..valid_row()
        };
        assert!(PersistedPlayback::from_row(row).is_none());
    }

    /// A multi-release source survives a full row round-trip through `from_row`.
    #[test]
    fn releases_context_row_round_trips() {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: source_to_str(&ContextSource::Releases(vec![
                    "rel-A".into(),
                    "rel-B".into(),
                ])),
                shuffled: false,
            }),
            ..valid_row()
        };
        let parsed = PersistedPlayback::from_row(row).expect("a valid row parses");
        let context = parsed.queue.context.expect("the context survives");
        assert_eq!(
            context.source,
            ContextSource::Releases(vec!["rel-A".into(), "rel-B".into()])
        );
    }

    #[test]
    fn repeat_encoding_uses_lowercase_names() {
        for (mode, encoded) in [
            (RepeatMode::Off, "off"),
            (RepeatMode::Track, "track"),
            (RepeatMode::Context, "context"),
        ] {
            assert_eq!(repeat_to_str(mode), encoded);
            assert_eq!(repeat_from_str(encoded).expect("repeat parses"), mode);
        }
    }

    #[test]
    fn malformed_manual_json_discards_the_row() {
        let row = DbPlaybackState {
            manual: "not json".to_string(),
            ..valid_row()
        };
        assert!(PersistedPlayback::from_row(row).is_none());
    }

    #[test]
    fn unknown_repeat_discards_the_row() {
        let row = DbPlaybackState {
            repeat: "sideways".to_string(),
            ..valid_row()
        };
        assert!(PersistedPlayback::from_row(row).is_none());
    }

    #[test]
    fn negative_position_discards_the_row() {
        let row = DbPlaybackState {
            position_ms: Some(-1),
            ..valid_row()
        };
        assert!(PersistedPlayback::from_row(row).is_none());
    }

    /// A row with no context and no saved position parses cleanly to `None` for
    /// both — a single track (or nothing) was playing, with position not yet
    /// recorded.
    #[test]
    fn missing_context_and_position_parse_as_none() {
        let row = DbPlaybackState {
            context: None,
            position_ms: None,
            ..valid_row()
        };
        let parsed = PersistedPlayback::from_row(row).expect("a valid row parses");
        assert!(parsed.queue.context.is_none());
        assert!(parsed.position_ms.is_none());
    }
}
