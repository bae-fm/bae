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

/// Write the single device-local `playback_state` row (id = 'current'),
/// replacing any existing one. Never synced.
pub async fn save_playback_state(store: &Store, state: &PlaybackResumeRow) -> Result<(), DbError> {
    let state = state.clone();
    store
        .call(move |conn| {
            let (current_entry, current_track_id) = match &state.current {
                Some(current) => (Some(current.id.to_string()), Some(current.track_id.clone())),
                None => (None, None),
            };
            conn.execute(
                "INSERT OR REPLACE INTO playback_state \
                 (id, current_entry, current_track_id, position_ms) \
                 VALUES ('current', ?, ?, ?)",
                params![
                    current_entry,
                    current_track_id,
                    state.position_ms.map(|position| position as i64),
                ],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
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
) -> coven::LiveQuery<Option<PlaybackResumeRow>, coven::CovenError> {
    store.subscribe(|sql| playback_state_on(&sql).map_err(CovenError::from))
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

/// Delete the device-local `playback_state` row (playback stopped).
///
/// Stopping is persisted on every queue change and every stop, so most
/// calls arrive with the row already gone. That case is read, not written:
/// a delete over an empty table is a transaction that changes nothing, and
/// those belong off the sync journal.
pub async fn clear_playback_state(store: &Store) -> Result<(), DbError> {
    let stored = store
        .read(move |sql| {
            Ok(sql.query_row("SELECT 1 FROM playback_state", [], |_| Ok(())).optional()?.is_some())
        })
        .await?;
    if !stored {
        return Ok(());
    }
    store
        .call(move |conn| {
            conn.execute("DELETE FROM playback_state", []).map(|_| ()).map_err(DbError::from)
        })
        .await
}
