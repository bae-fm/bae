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

coven::write!(
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

coven::write!(
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

// The playing track's line, while the playing track has reported a position.
coven::fragment!(
    LibraryTimelineLine {
        track_id: String,
        position_ms: u64,
        duration_ms: Option<u64>,
        moving: bool,
        started_at_seek: bool
    },
    r#"
    SELECT line.singleton AS library_singleton, line.track_id, line.position_ms,
           line.duration_ms, line.moving, line.started_at_seek
    FROM playback_timeline AS line
    WHERE line.track_id IS NOT NULL AND line.position_ms IS NOT NULL
      AND line.moving IS NOT NULL AND line.started_at_seek IS NOT NULL
"#
);

// The preview clip's line, while a preview is loaded.
coven::fragment!(
    PreviewTimelineLine {
        preview_path: Vec<u8>,
        preview_start_sample: u64,
        preview_end_sample: Option<u64>,
        preview_position_ms: u64,
        preview_duration_ms: u64,
        preview_moving: bool
    },
    r#"
    SELECT line.singleton AS preview_singleton, line.preview_path, line.preview_start_sample,
           line.preview_end_sample, line.preview_position_ms, line.preview_duration_ms,
           line.preview_moving
    FROM playback_timeline AS line
    WHERE line.preview_path IS NOT NULL AND line.preview_start_sample IS NOT NULL
      AND line.preview_position_ms IS NOT NULL AND line.preview_duration_ms IS NOT NULL
      AND line.preview_moving IS NOT NULL
"#
);

// Where playback is: the playing track's line and the preview's, each absent
// while it is not set.
coven::read!(
    writer TimelineLines,
    r#"
    SELECT library.*, preview.*
    FROM playback_timeline AS line
    LEFT JOIN @crate::tables::timeline::LibraryTimelineLine AS library
           ON library.library_singleton = line.singleton
    LEFT JOIN @crate::tables::timeline::PreviewTimelineLine AS preview
           ON preview.preview_singleton = line.singleton
"#
);

/// Where playback is, as the row's lines hold it.
fn playback_timeline_of(row: TimelineLinesRow) -> Result<PlaybackTimeline, DbError> {
    let library = row.library.map(|line| TimelinePoint {
        track_id: line.track_id,
        position_ms: line.position_ms,
        duration_ms: line.duration_ms,
        moving: line.moving,
        started_at_seek: line.started_at_seek,
    });
    let preview = row
        .preview
        .map(|line| {
            Ok::<_, DbError>(PreviewTimelinePoint {
                target: PreviewTarget::sample_range(
                    bae_store::path_column::decode(line.preview_path)?,
                    line.preview_start_sample,
                    line.preview_end_sample,
                ),
                position_ms: line.preview_position_ms,
                duration_ms: line.preview_duration_ms,
                moving: line.preview_moving,
            })
        })
        .transpose()?;
    Ok(PlaybackTimeline { library, preview })
}

// Lines the schema refuses: each sets part of a line, or a length or an end
// without its line. No write of the domain writes one.
#[cfg(test)]
coven::write!(TimelineTrackWithoutPosition, r#"UPDATE playback_timeline SET track_id = 'track-1'"#);

#[cfg(test)]
coven::write!(
    TimelineLineWithoutMoving,
    r#"UPDATE playback_timeline SET track_id = 'track-1', position_ms = 0"#
);

#[cfg(test)]
coven::write!(
    TimelineLineWithoutSeekMark,
    r#"UPDATE playback_timeline SET track_id = 'track-1', position_ms = 0, moving = 1"#
);

#[cfg(test)]
coven::write!(TimelineSeekMarkWithoutLine, r#"UPDATE playback_timeline SET started_at_seek = 0"#);

#[cfg(test)]
coven::write!(TimelineLengthWithoutLine, r#"UPDATE playback_timeline SET duration_ms = 1000"#);

#[cfg(test)]
coven::write!(
    TimelinePreviewWithoutMoving,
    r#"
    UPDATE playback_timeline SET preview_path = x'2f61', preview_start_sample = 0,
        preview_position_ms = 0, preview_duration_ms = 1000
"#
);

#[cfg(test)]
coven::write!(
    TimelinePreviewEndWithoutPreview,
    r#"UPDATE playback_timeline SET preview_end_sample = 10"#
);

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
