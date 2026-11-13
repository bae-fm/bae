//! What a play fills the context lane with: the tracks it names, in play
//! order, where the lane starts, and what it named and left out. One bulk
//! write replaces the whole queue with them, shuffled when the play asks for
//! it or the queue's shuffle mode is on.

use super::queue::{context_from_row, queue_row_on};
use super::*;
use crate::{ContextSource, QueueEntry};

/// What a play fills the context lane with.
#[derive(Debug, Clone)]
pub enum ContextFill {
    /// The release `track_id` is on, in its order, from that track.
    ReleaseOfTrack(String),
    /// One release in its order, from where `start` says.
    Release { release_id: String, start: ReleaseStart },
    /// Releases one after another in the order given, each in its own order,
    /// from the first track. A release with no tracks is left out, of the lane
    /// and of the source the queue names; a release named twice plays twice.
    Releases(Vec<String>),
    /// Tracks in the order given, from where `start` says. A track the
    /// library does not hold is left out; a track named twice plays twice.
    Tracks { track_ids: Vec<String>, start: TracksStart },
    /// Every library track, shuffled.
    LibraryShuffled,
}

/// Where a release starts playing.
#[derive(Debug, Clone)]
pub enum ReleaseStart {
    First,
    Track(String),
    Shuffled,
}

/// Where a list of tracks starts playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracksStart {
    /// The entry at this index of the list as sent.
    Index(usize),
    /// Wherever a shuffle of the list puts first.
    Shuffled,
}

/// What a fill did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextFilled {
    /// The lane holds `track_count` tracks and plays `current` first; Up Next
    /// is empty. `left_out` is what the fill named and left out.
    Playing { current: QueueEntry, track_count: usize, left_out: LeftOut },
    /// The release, the releases or the library hold no tracks, or the list
    /// of tracks is empty. The queue is as it was.
    NoTracks,
    /// The track to start from is not on the release, not in the library, or
    /// not in the list of tracks at its start index. The queue is as it was.
    StartMissing,
}

/// What a fill named and left out of the lane, in the order named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeftOut {
    /// Everything named is in the lane. A fill of one release or the library
    /// names nothing it could leave out.
    Nothing,
    /// Releases among several that hold no tracks; never empty.
    Releases(Vec<String>),
    /// Tracks of a list that the library doesn't hold; never empty.
    Tracks(Vec<String>),
}

impl LeftOut {
    fn releases(ids: Vec<String>) -> Self {
        if ids.is_empty() {
            Self::Nothing
        } else {
            Self::Releases(ids)
        }
    }

    fn tracks(ids: Vec<String>) -> Self {
        if ids.is_empty() {
            Self::Nothing
        } else {
            Self::Tracks(ids)
        }
    }
}

/// The tracks a fill puts in the lane, as a `FROM … WHERE …` over `tracks t`
/// binding `:source` when it names one, and the source order that ranks them.
struct FillSource {
    from: &'static str,
    source_order: &'static str,
    source: Option<String>,
}

/// The lane row a fill plays first.
enum FillCursor {
    First,
    /// The row of this track, which the one release played holds once.
    Track(String),
    /// The row at this position.
    Position(i64),
}

impl FillCursor {
    /// The lane when shuffled: the row played first leads it and the rest
    /// follow in random order, so a shuffled play of one track still starts
    /// with it. SQL over a lane row's `track_id` and `ordered`, true for
    /// the row that leads, with the value it binds as `:lead`.
    fn lead(&self) -> Option<(&'static str, &dyn rusqlite::ToSql)> {
        match self {
            FillCursor::First => None,
            FillCursor::Track(track_id) => Some(("track_id = :lead", track_id)),
            FillCursor::Position(position) => Some(("ordered = :lead", position)),
        }
    }
}

pub(super) fn fill_on(
    sql: &coven::BulkSqlContext<'_>,
    fill: ContextFill,
) -> Result<ContextFilled, DbError> {
    let release_source = |release_id: String| FillSource {
        from: "tracks t WHERE t.release_id = :source",
        source_order: "t.position, t.id",
        source: Some(release_id),
    };
    let (source, rows, whole_shuffled, start, left_out) = match fill {
        ContextFill::ReleaseOfTrack(track_id) => {
            let release_id: Option<String> = sql
                .query_row("SELECT release_id FROM tracks WHERE id = ?", [&track_id], |row| {
                    row.get(0)
                })
                .optional()?;
            let Some(release_id) = release_id else {
                return Ok(ContextFilled::StartMissing);
            };
            (
                ContextSource::Release(release_id.clone()),
                release_source(release_id),
                false,
                FillCursor::Track(track_id),
                LeftOut::Nothing,
            )
        }
        ContextFill::Release { release_id, start } => {
            let has_tracks: bool = sql.query_row(
                "SELECT EXISTS (SELECT 1 FROM tracks WHERE release_id = ?)",
                [&release_id],
                |row| row.get(0),
            )?;
            if !has_tracks {
                return Ok(ContextFilled::NoTracks);
            }
            let (shuffled, start) = match start {
                ReleaseStart::First => (false, FillCursor::First),
                ReleaseStart::Shuffled => (true, FillCursor::First),
                ReleaseStart::Track(track_id) => {
                    let on_release: bool = sql.query_row(
                        "SELECT EXISTS (SELECT 1 FROM tracks WHERE id = ? AND release_id = ?)",
                        [&track_id, &release_id],
                        |row| row.get(0),
                    )?;
                    if !on_release {
                        return Ok(ContextFilled::StartMissing);
                    }
                    (false, FillCursor::Track(track_id))
                }
            };
            (
                ContextSource::Release(release_id.clone()),
                release_source(release_id),
                shuffled,
                start,
                LeftOut::Nothing,
            )
        }
        ContextFill::Releases(release_ids) => {
            let release_ids = serde_json::to_string(&release_ids)
                .expect("serializing a Vec<String> to JSON cannot fail");
            let playable: Vec<String> = sql.query(
                "SELECT j.value FROM json_each(?) j \
                 WHERE EXISTS (SELECT 1 FROM tracks WHERE release_id = j.value) \
                 ORDER BY j.key",
                [&release_ids],
                |row| row.get(0),
            )?;
            let left_out: Vec<String> = sql.query(
                "SELECT j.value FROM json_each(?) j \
                 WHERE NOT EXISTS (SELECT 1 FROM tracks WHERE release_id = j.value) \
                 ORDER BY j.key",
                [&release_ids],
                |row| row.get(0),
            )?;
            if playable.is_empty() {
                return Ok(ContextFilled::NoTracks);
            }
            let playable_ids = serde_json::to_string(&playable)
                .expect("serializing a Vec<String> to JSON cannot fail");
            (
                ContextSource::releases(playable),
                FillSource {
                    from: "json_each(:source) j JOIN tracks t ON t.release_id = j.value",
                    source_order: "j.key, t.position, t.id",
                    source: Some(playable_ids),
                },
                false,
                FillCursor::First,
                LeftOut::releases(left_out),
            )
        }
        ContextFill::Tracks { track_ids, start } => {
            if track_ids.is_empty() {
                return Ok(ContextFilled::NoTracks);
            }
            match start {
                TracksStart::Index(index) => {
                    let Some(start_track) = track_ids.get(index) else {
                        return Ok(ContextFilled::StartMissing);
                    };
                    let start_held: bool = sql.query_row(
                        "SELECT EXISTS (SELECT 1 FROM tracks WHERE id = ?)",
                        [start_track],
                        |row| row.get(0),
                    )?;
                    if !start_held {
                        return Ok(ContextFilled::StartMissing);
                    }
                }
                TracksStart::Shuffled => {
                    let any_held: bool = sql.query_row(
                        "SELECT EXISTS (SELECT 1 FROM json_each(?1) j \
                         JOIN tracks t ON t.id = j.value)",
                        [serde_json::to_string(&track_ids)
                            .expect("serializing a Vec<String> to JSON cannot fail")],
                        |row| row.get(0),
                    )?;
                    if !any_held {
                        return Ok(ContextFilled::NoTracks);
                    }
                }
            }
            let track_ids = serde_json::to_string(&track_ids)
                .expect("serializing a Vec<String> to JSON cannot fail");
            let left_out: Vec<String> = sql.query(
                "SELECT j.value FROM json_each(?) j \
                 WHERE NOT EXISTS (SELECT 1 FROM tracks WHERE id = j.value) \
                 ORDER BY j.key",
                [&track_ids],
                |row| row.get(0),
            )?;
            // The start entry's place among the entries the library holds,
            // which are the lane's rows in the list's order.
            let (whole_shuffled, cursor) = match start {
                TracksStart::Shuffled => (true, FillCursor::First),
                TracksStart::Index(index) => {
                    let position: i64 = sql.query_row(
                        "SELECT count(*) FROM json_each(?1) j JOIN tracks t ON t.id = j.value \
                         WHERE j.key <= ?2",
                        params![&track_ids, index as i64],
                        |row| row.get(0),
                    )?;
                    (false, FillCursor::Position(position))
                }
            };
            (
                ContextSource::Tracks,
                FillSource {
                    from: "json_each(:source) j JOIN tracks t ON t.id = j.value",
                    source_order: "j.key",
                    source: Some(track_ids),
                },
                whole_shuffled,
                cursor,
                LeftOut::tracks(left_out),
            )
        }
        ContextFill::LibraryShuffled => {
            let has_tracks: bool =
                sql.query_row("SELECT EXISTS (SELECT 1 FROM tracks)", [], |row| row.get(0))?;
            if !has_tracks {
                return Ok(ContextFilled::NoTracks);
            }
            (
                ContextSource::Library,
                FillSource {
                    from: "tracks t",
                    source_order: "t.release_id, t.position, t.id",
                    source: None,
                },
                true,
                FillCursor::First,
                LeftOut::Nothing,
            )
        }
    };

    // A play the person asked shuffled shuffles the whole lane, and turns the
    // mode on. Any other play shuffles while the mode is on, from its start
    // track when it names one.
    let shuffled = whole_shuffled || queue_row_on(sql)?.shuffled;
    let lead = if whole_shuffled { None } else { start.lead() }.filter(|_| shuffled);
    // Up Next empties and the context lane is replaced, with what it plays
    // from written below.
    if sql.execute("DELETE FROM playback_manual", [])? > 0 {
        sql.execute(super::queue::MANUAL_CHANGED, [])?;
    }
    sql.execute("DELETE FROM playback_context", [])?;
    let play_order = match (shuffled, &lead) {
        (false, _) => "ordered".to_string(),
        (true, None) => "random()".to_string(),
        (true, Some((leads, _))) => format!("({leads}) DESC, random()"),
    };
    // The lane's entries are numbered on from the next free entry number, in
    // play order, and inserted in that order: the table, its shuffled index
    // and its track index then grow at their end. A lane not shuffled reads in
    // the order it was numbered in, so its two orders agree.
    let base = queue_row_on(sql)?.next_entry;
    let mut bound: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":base", &base)];
    if let Some(source) = &rows.source {
        bound.push((":source", source));
    }
    if let Some((_, value)) = &lead {
        bound.push((":lead", *value));
    }
    let track_count = sql.execute(
        &format!(
            "INSERT INTO playback_context (entry, ordered, shuffled, track_id) \
             SELECT :base + shuffled - 1, ordered, shuffled, track_id FROM ( \
                 SELECT track_id, ordered, \
                        row_number() OVER (ORDER BY {play_order}) AS shuffled \
                 FROM (SELECT t.id AS track_id, \
                              row_number() OVER (ORDER BY {source_order}) AS ordered \
                       FROM {from})) \
             ORDER BY shuffled",
            source_order = rows.source_order,
            from = rows.from,
        ),
        bound.as_slice(),
    )?;
    // The row played first: a shuffled lane's first row, else the start's.
    let (ordered_cursor, shuffled_cursor): (i64, i64) = if shuffled {
        let ordered =
            sql.query_row("SELECT ordered FROM playback_context WHERE shuffled = 1", [], |row| {
                row.get(0)
            })?;
        (ordered, 1)
    } else {
        let ordered = match start {
            FillCursor::First => 1,
            FillCursor::Track(track_id) => sql.query_row(
                "SELECT ordered FROM playback_context WHERE track_id = ?",
                [track_id],
                |row| row.get(0),
            )?,
            FillCursor::Position(position) => position,
        };
        (ordered, ordered)
    };
    let (source_kind, source_release_id) = source.to_columns();
    sql.execute(
        "UPDATE playback_queue \
         SET source_kind = ?, source_release_id = ?, shuffled = ?, ordered_cursor = ?, \
             shuffled_cursor = ?, next_entry = ?, context_version = context_version + 1",
        params![
            source_kind,
            source_release_id,
            shuffled,
            ordered_cursor,
            shuffled_cursor,
            base + track_count as i64
        ],
    )?;
    let (current, _) = sql.query_row(
        "SELECT entry, track_id, shuffled AS position FROM playback_context WHERE shuffled = ?",
        [shuffled_cursor],
        context_from_row,
    )?;
    Ok(ContextFilled::Playing { current, track_count, left_out })
}
