//! The device's `playback_timeline` row: what it holds of each line, what the
//! schema refuses, and what starting playback leaves.

use super::super::*;
use crate::playback::{PlaybackTimeline, PreviewTarget, PreviewTimelinePoint, TimelinePoint};

fn timeline() -> PlaybackTimeline {
    PlaybackTimeline {
        library: Some(TimelinePoint {
            track_id: "track-1".to_string(),
            position_ms: 61_000,
            duration_ms: None,
            moving: true,
        }),
        seek_revision: 3,
        preview: Some(PreviewTimelinePoint {
            target: PreviewTarget::sample_range("/music/Album Title/01.flac".into(), 44_100, None),
            position_ms: 2_500,
            duration_ms: 30_000,
            moving: false,
        }),
    }
}

fn nothing() -> PlaybackTimeline {
    PlaybackTimeline {
        library: None,
        seek_revision: 0,
        preview: None,
    }
}

/// The row reads back each line as written, a length unknown and a clip
/// running to its file's end included, and a line taken away is gone.
#[tokio::test]
async fn the_row_holds_each_line_as_written() {
    let (db, _tmp) = super::temp_db().await;
    assert_eq!(db.playback_timeline().await.unwrap(), nothing());
    let row = db.playback_timeline_row();

    row.write(&timeline()).await.unwrap();
    assert_eq!(db.playback_timeline().await.unwrap(), timeline());

    let mut ended = timeline();
    ended.library = None;
    ended.preview = Some(PreviewTimelinePoint {
        target: PreviewTarget::sample_range("/music/Album Title/02.flac".into(), 0, Some(88_200)),
        position_ms: 0,
        duration_ms: 2_000,
        moving: true,
    });
    row.write(&ended).await.unwrap();
    assert_eq!(db.playback_timeline().await.unwrap(), ended);
}

/// Starting playback with the library leaves no line and no seek, whether it
/// resumes the queue or not.
#[tokio::test]
async fn starting_playback_leaves_no_line() {
    let (db, _tmp) = super::temp_db().await;
    for resume in [true, false] {
        db.playback_timeline_row().write(&timeline()).await.unwrap();
        db.settle_playback_queue(resume).await.unwrap();
        assert_eq!(
            db.playback_timeline().await.unwrap(),
            nothing(),
            "resume: {resume}"
        );
    }
}

/// Each line's columns are set together; a length or an end needs its line.
#[tokio::test]
async fn the_schema_refuses_half_a_line() {
    let (db, _tmp) = super::temp_db().await;
    for sql in [
        "UPDATE playback_timeline SET track_id = 'track-1'",
        "UPDATE playback_timeline SET track_id = 'track-1', position_ms = 0",
        "UPDATE playback_timeline SET duration_ms = 1000",
        "UPDATE playback_timeline SET preview_path = x'2f61', preview_start_sample = 0, \
             preview_position_ms = 0, preview_duration_ms = 1000",
        "UPDATE playback_timeline SET preview_end_sample = 10",
    ] {
        let statement = sql.to_string();
        let refused = db
            .call(move |conn| conn.execute_batch(&statement).map_err(DbError::from))
            .await
            .is_err();
        assert!(refused, "{sql}");
    }
}
