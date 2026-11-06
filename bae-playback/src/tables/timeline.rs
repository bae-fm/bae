//! The device's `playback_timeline` row: where playback is, for the surfaces
//! that move the position on themselves, written only as playback leaves the
//! line it holds, and read by their live query. Its table is `timeline.sql`.

/// The row's part of the library's schema.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("timeline.sql")]);

use super::preview_target::{preview_target_from_row, preview_target_values};
use super::*;
use crate::{PlaybackTimeline, PreviewTimelinePoint, TimelinePoint};

/// The `playback_timeline` row, as playback writes it.
#[derive(Clone)]
pub struct PlaybackTimelineRow {
    store: Store,
}

impl PlaybackTimelineRow {
    /// The row over `store`.
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

/// Follow where playback is, as the surfaces that move the position on
/// themselves read it.
pub fn subscribe_playback_timeline(
    store: &Store,
) -> coven::LiveQuery<PlaybackTimeline, coven::CovenError> {
    store.subscribe(|sql| playback_timeline_on(&sql).map_err(CovenError::from))
}

/// Where playback is now, as `subscribe_playback_timeline` reads it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn playback_timeline(store: &Store) -> Result<PlaybackTimeline, DbError> {
    store.read(|sql| playback_timeline_on(&sql)).await
}

impl PlaybackTimelineRow {
    /// Write `timeline` as the row's whole value.
    pub async fn write(&self, timeline: &PlaybackTimeline) -> Result<(), DbError> {
        let timeline = timeline.clone();
        self.store.call(move |sql| write_on(sql, &timeline).map_err(DbError::from)).await
    }
}

fn write_on(sql: &CovenWrite<'_>, timeline: &PlaybackTimeline) -> rusqlite::Result<()> {
    let library = timeline.library.as_ref();
    let preview = timeline.preview.as_ref().map(|preview| {
        let (path, start_sample, end_sample) = preview_target_values(&preview.target);
        (path, start_sample, end_sample, preview)
    });
    sql.execute(
        "UPDATE playback_timeline SET track_id = ?, position_ms = ?, duration_ms = ?, \
             moving = ?, seek_revision = ?, preview_path = ?, preview_start_sample = ?, \
             preview_end_sample = ?, preview_position_ms = ?, preview_duration_ms = ?, \
             preview_moving = ?",
        params![
            library.map(|line| line.track_id.as_str()),
            library.map(|line| line.position_ms),
            library.and_then(|line| line.duration_ms),
            library.map(|line| line.moving),
            timeline.seek_revision,
            preview.as_ref().map(|clip| clip.0.as_slice()),
            preview.as_ref().map(|clip| clip.1),
            preview.as_ref().and_then(|clip| clip.2),
            preview.as_ref().map(|clip| clip.3.position_ms),
            preview.as_ref().map(|clip| clip.3.duration_ms),
            preview.as_ref().map(|clip| clip.3.moving),
        ],
    )?;
    Ok(())
}

/// Reset the row to what playback starts with: no line, no seek applied.
pub(super) fn reset_playback_timeline_on(sql: &coven::BulkSqlContext<'_>) -> rusqlite::Result<()> {
    sql.execute(
        "UPDATE playback_timeline SET track_id = NULL, position_ms = NULL, duration_ms = NULL, \
             moving = NULL, seek_revision = 0, preview_path = NULL, \
             preview_start_sample = NULL, preview_end_sample = NULL, \
             preview_position_ms = NULL, preview_duration_ms = NULL, preview_moving = NULL",
        [],
    )?;
    Ok(())
}

fn playback_timeline_on(sql: &SqlReadContext<'_>) -> Result<PlaybackTimeline, DbError> {
    Ok(sql.query_row("SELECT * FROM playback_timeline", [], |row| {
        // The table's CHECKs keep each line's columns set together.
        let library = match row.get::<_, Option<String>>("track_id")? {
            None => None,
            Some(track_id) => Some(TimelinePoint {
                track_id,
                position_ms: row.get("position_ms")?,
                duration_ms: row.get("duration_ms")?,
                moving: row.get("moving")?,
            }),
        };
        let preview = match preview_target_from_row(row)? {
            None => None,
            Some(target) => Some(PreviewTimelinePoint {
                target,
                position_ms: row.get("preview_position_ms")?,
                duration_ms: row.get("preview_duration_ms")?,
                moving: row.get("preview_moving")?,
            }),
        };
        Ok(PlaybackTimeline { library, seek_revision: row.get("seek_revision")?, preview })
    })?)
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
