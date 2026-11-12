//! The device-local `playback_state` row: where playback stood, so a restart
//! resumes it.

use super::*;

/// The single device-local `playback_state` row: where playback stood, so a
/// restart resumes it. The queue it plays from is in the queue tables, and
/// the volume it resumes at in `playback_now`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackResumeRow {
    /// The queue entry playing, `None` when its track was deleted.
    pub current: Option<crate::QueueEntry>,
    /// How far into the current track's stream it had played.
    pub position_ms: Option<u64>,
}

/// Read the device-local `playback_state` row, `None` when none is stored.
pub async fn load_playback_state(store: &Store) -> Result<Option<PlaybackResumeRow>, DbError> {
    store.read(|sql| playback_state_on(&sql)).await
}

/// Follow the device-local `playback_state` row as `load_playback_state`
/// reads it.
#[cfg(any(test, feature = "test-utils"))]
pub fn subscribe_playback_state(
    store: &Store,
) -> coven::LiveQuery<Option<PlaybackResumeRow>, bae_store::DbError> {
    store.subscribe(|sql| playback_state_on(&sql))
}

/// The `playback_state` row `sql` reads, `None` when none is stored.
fn playback_state_on(sql: &SqlReadContext<'_>) -> Result<Option<PlaybackResumeRow>, DbError> {
    Ok(sql
        .query_row(
            "SELECT current_entry, current_track_id, position_ms \
                 FROM playback_state WHERE id = 'current'",
            [],
            |row| {
                let entry: Option<String> = row.get("current_entry")?;
                let track_id: Option<String> = row.get("current_track_id")?;
                // The schema keeps `position_ms` non-negative.
                let position_ms: Option<i64> = row.get("position_ms")?;
                let current = match entry.zip(track_id) {
                    Some((entry, track_id)) => Some(crate::QueueEntry {
                        id: entry.parse().map_err(|e: String| {
                            column_conversion_error(row, "current_entry", e)
                        })?,
                        track_id,
                    }),
                    None => None,
                };
                Ok(PlaybackResumeRow {
                    current,
                    position_ms: position_ms.map(|position| position as u64),
                })
            },
        )
        .optional()?)
}

/// Plant damage for a test: the resume row names an entry no queue entry id
/// parses from, so reading it fails.
#[cfg(any(test, feature = "test-utils"))]
pub fn plant_unreadable_resume_row_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_state",
        "PRAGMA foreign_keys = OFF; \
         INSERT OR REPLACE INTO playback_state (id, current_entry, current_track_id, position_ms) \
         VALUES ('current', 'not an entry id', 'a-track', 1000);",
    );
}
