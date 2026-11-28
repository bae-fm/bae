//! The device's `playback_timeline` row: where playback is, for the surfaces
//! that move the position on themselves, written only as playback leaves the
//! line it holds, and read by their live query. Its table is `timeline.sql`.

use super::preview_target::preview_target_values;
use super::*;
use crate::{PlaybackTimeline, PreviewTimelinePoint, TimelinePoint};
use bae_store::WriteStatements;
use bae_track_audio::PreviewTarget;

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
) -> coven::LiveQuery<PlaybackTimeline, bae_store::DbError> {
    store.subscribe_row(TimelineLinesParams {}).process(playback_timeline_of)
}

/// Where playback is now, as `subscribe_playback_timeline` reads it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn playback_timeline(store: &Store) -> Result<PlaybackTimeline, DbError> {
    playback_timeline_of(store.read_row(TimelineLinesParams {}).await?)
}

sqlive_compose::write!(
    SetPlaybackTimeline(
        track_id: Option<&'p str>,
        position_ms: Option<u64>,
        duration_ms: Option<u64>,
        moving: Option<bool>,
        started_at_seek: Option<bool>,
        preview_path: Option<&'p [u8]>,
        preview_start_sample: Option<u64>,
        preview_end_sample: Option<u64>,
        preview_position_ms: Option<u64>,
        preview_duration_ms: Option<u64>,
        preview_moving: Option<bool>,
    ),
    r#"
    UPDATE playback_timeline
    SET track_id = :track_id, position_ms = :position_ms, duration_ms = :duration_ms,
        moving = :moving, started_at_seek = :started_at_seek, preview_path = :preview_path,
        preview_start_sample = :preview_start_sample, preview_end_sample = :preview_end_sample,
        preview_position_ms = :preview_position_ms, preview_duration_ms = :preview_duration_ms,
        preview_moving = :preview_moving
"#
);

sqlive_compose::write!(
    ResetPlaybackTimeline,
    r#"
    UPDATE playback_timeline
    SET track_id = NULL, position_ms = NULL, duration_ms = NULL, moving = NULL,
        started_at_seek = NULL, preview_path = NULL, preview_start_sample = NULL,
        preview_end_sample = NULL, preview_position_ms = NULL, preview_duration_ms = NULL,
        preview_moving = NULL
"#
);

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
    sql.run(&SetPlaybackTimelineParams {
        track_id: library.map(|line| line.track_id.as_str()),
        position_ms: library.map(|line| line.position_ms),
        duration_ms: library.and_then(|line| line.duration_ms),
        moving: library.map(|line| line.moving),
        started_at_seek: library.map(|line| line.started_at_seek),
        preview_path: preview.as_ref().map(|clip| clip.0.as_slice()),
        preview_start_sample: preview.as_ref().map(|clip| clip.1),
        preview_end_sample: preview.as_ref().and_then(|clip| clip.2),
        preview_position_ms: preview.as_ref().map(|clip| clip.3.position_ms),
        preview_duration_ms: preview.as_ref().map(|clip| clip.3.duration_ms),
        preview_moving: preview.as_ref().map(|clip| clip.3.moving),
    })?;
    Ok(())
}

/// Reset the row to what playback starts with: no line.
pub(super) fn reset_playback_timeline_on(sql: &coven::BulkSqlContext<'_>) -> rusqlite::Result<()> {
    sql.run(&ResetPlaybackTimelineParams {})?;
    Ok(())
}

// Where playback is: the playing track's line and the preview's.
sqlive_compose::read!(
    writer TimelineLines {
        track_id: Option<String>,
        position_ms: Option<u64>,
        duration_ms: Option<u64>,
        moving: Option<bool>,
        started_at_seek: Option<bool>,
        preview_path: Option<Vec<u8>>,
        preview_start_sample: Option<u64>,
        preview_end_sample: Option<u64>,
        preview_position_ms: Option<u64>,
        preview_duration_ms: Option<u64>,
        preview_moving: Option<bool>
    },
    r#"
    SELECT line.track_id, line.position_ms, line.duration_ms, line.moving, line.started_at_seek,
           line.preview_path, line.preview_start_sample, line.preview_end_sample,
           line.preview_position_ms, line.preview_duration_ms, line.preview_moving
    FROM playback_timeline AS line
"#
);

/// Where playback is, as the row's columns hold it. The table's CHECKs keep
/// each line's columns set together.
fn playback_timeline_of(row: TimelineLinesRow) -> Result<PlaybackTimeline, DbError> {
    let library = match row.track_id {
        None => None,
        Some(track_id) => Some(TimelinePoint {
            track_id,
            position_ms: set(row.position_ms, "position_ms")?,
            duration_ms: row.duration_ms,
            moving: set(row.moving, "moving")?,
            started_at_seek: set(row.started_at_seek, "started_at_seek")?,
        }),
    };
    let preview = match row.preview_path {
        None => None,
        Some(path) => Some(PreviewTimelinePoint {
            target: PreviewTarget::sample_range(
                bae_store::path_column::decode(path)?,
                set(row.preview_start_sample, "preview_start_sample")?,
                row.preview_end_sample,
            ),
            position_ms: set(row.preview_position_ms, "preview_position_ms")?,
            duration_ms: set(row.preview_duration_ms, "preview_duration_ms")?,
            moving: set(row.preview_moving, "preview_moving")?,
        }),
    };
    Ok(PlaybackTimeline { library, preview })
}

/// `value`, the column `column` of a line the row holds, which the line sets.
fn set<T>(value: Option<T>, column: &str) -> Result<T, DbError> {
    value.ok_or_else(|| DbError::unreadable(format!("playback_timeline.{column}"), None::<String>))
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
