//! Parse the device-local `playback_state` resume cache at one boundary.
//!
//! The row is read back at startup as a single fallible parse: [`from_row`]
//! either yields a fully-valid [`PersistedPlayback`] or discards the whole cache
//! (logged once) on any out-of-domain value. No consumer downstream patches a
//! bad field — by the time the service holds a `PersistedPlayback`, every value
//! is already in range.

use tracing::warn;

use super::{ContextSnapshot, QueueSnapshot, RepeatMode, Traversal};
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
                warn!("discarding the playback resume cache: manual lane is not valid JSON: {e}");
                return None;
            }
        };

        let repeat = match repeat_from_str(&row.repeat) {
            Some(repeat) => repeat,
            None => {
                warn!(
                    "discarding the playback resume cache: unrecognized repeat {:?}",
                    row.repeat
                );
                return None;
            }
        };

        let context = match row.context {
            Some(ctx) => {
                let cursor = match usize::try_from(ctx.cursor) {
                    Ok(cursor) => cursor,
                    Err(_) => {
                        warn!(
                            "discarding the playback resume cache: negative context cursor {}",
                            ctx.cursor
                        );
                        return None;
                    }
                };
                let traversal = match ctx.shuffle_seed {
                    Some(seed) => Traversal::Shuffled { seed: seed as u64 },
                    None => Traversal::Sequential,
                };
                Some(ContextSnapshot {
                    source: ctx.source,
                    traversal,
                    cursor,
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

/// Serialize `RepeatMode` for the `playback_state.repeat` column.
pub fn repeat_to_str(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Context => "context",
    }
}

/// Parse the `playback_state.repeat` column, or `None` for an unrecognized value
/// (the boundary parse discards the whole cache when this is `None`).
fn repeat_from_str(s: &str) -> Option<RepeatMode> {
    match s {
        "off" => Some(RepeatMode::Off),
        "track" => Some(RepeatMode::Track),
        "context" => Some(RepeatMode::Context),
        _ => None,
    }
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
                shuffle_seed: Some(99),
                cursor: 2,
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
        assert_eq!(context.source, "rel-A");
        assert_eq!(context.cursor, 2);
        assert!(matches!(
            context.traversal,
            Traversal::Shuffled { seed: 99 }
        ));
        assert_eq!(parsed.queue.manual, vec!["m1", "m2"]);
        assert_eq!(parsed.queue.current_track_id.as_deref(), Some("t3"));
        assert_eq!(parsed.queue.repeat, RepeatMode::Context);
        assert_eq!(parsed.position_ms, Some(42_000));
        assert!((parsed.volume - 0.5).abs() < f32::EPSILON);
        assert!(parsed.is_muted);
    }

    #[test]
    fn sequential_context_has_no_seed() {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: "rel-A".to_string(),
                shuffle_seed: None,
                cursor: 0,
            }),
            ..valid_row()
        };
        let parsed = PersistedPlayback::from_row(row).expect("a valid row parses");
        let context = parsed.queue.context.expect("the context survives");
        assert!(matches!(context.traversal, Traversal::Sequential));
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
    fn negative_cursor_discards_the_row() {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: "rel-A".to_string(),
                shuffle_seed: None,
                cursor: -1,
            }),
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
}
