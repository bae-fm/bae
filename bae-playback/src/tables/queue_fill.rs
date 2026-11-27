//! What a play fills the context lane with: the tracks it names, in play
//! order, where the lane starts, and what it named and left out. One bulk
//! write replaces the whole queue with them, shuffled when the play asks for
//! it or the queue's shuffle mode is on.

use super::queue::{context_from_row, queue_row_on, ClearContextLaneParams, ClearUpNextParams};
use super::*;
use crate::{ContextSource, QueueEntry};
use bae_store::WriteStatements;

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
    /// Tracks in the list's order, from where `start` says, counting the
    /// list in that order. A track a given list names twice plays twice, and
    /// one the library does not hold is left out and named. A picked list
    /// counts only the tracks the library holds and names none left out.
    Tracks { tracks: bae_records::TrackList, start: TracksStart },
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
    /// The entry at this index of the list, in the list's order.
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

/// The tracks a fill puts in the lane, in the source's order.
enum FillSource {
    /// The tracks of one release, by its id.
    Release(String),
    /// The tracks of each release a JSON array of ids names, release by
    /// release in the array's order.
    Releases(String),
    /// The library's tracks among those a JSON array of ids names, in the
    /// array's order.
    Tracks(String),
    /// Every library track, release by release.
    Library,
}

// The tracks of a source, each with its place `ordered` in the source's order,
// numbered from 1 in the order a play takes them. A play that shuffles takes
// the row that leads first, which `lead_track` or `lead_position` names by its
// track or its place, and the rest in random order; any other takes them in
// the source's order.
sqlive_compose::fragment!(
    PlayOrder(shuffled: bool, lead_track: Option<String>, lead_position: Option<i64>)
    with sourced(track_id TEXT, ordered INTEGER) { track_id: String, ordered: i64, played: i64 },
    r#"
    SELECT source.track_id, source.ordered,
           row_number() OVER (
               ORDER BY CASE WHEN :shuffled
                            THEN source.track_id IS :lead_track
                                 OR source.ordered IS :lead_position END DESC,
                        CASE WHEN :shuffled THEN random() ELSE source.ordered END) AS played
    FROM sourced AS source
"#
);

// The context lane holding a release's tracks. The lane's entries are numbered
// on from `base`, the next free entry number, in play order, and go in in that
// order: the table, its shuffled index and its track index then grow at their
// end. A lane not shuffled reads in the order it was numbered in, so its two
// orders agree.
sqlive_compose::write!(
    FillFromRelease(
        release: &'p str,
        base: i64,
        shuffled: bool,
        lead_track: Option<&'p str>,
        lead_position: Option<i64>,
    ),
    r#"
    INSERT INTO playback_context (entry, ordered, shuffled, track_id)
    SELECT :base + lane.played - 1, lane.ordered, lane.played, lane.track_id
    FROM @crate::tables::queue_fill::PlayOrder(
        shuffled: :shuffled, lead_track: :lead_track, lead_position: :lead_position,
        sourced: (
            SELECT track.track_id,
                   row_number() OVER (ORDER BY track.position, track.track_id)
            FROM @bae_records::records::spread_fragments::TrackPosition AS track
            WHERE track.release_id = :release)) AS lane
    ORDER BY lane.played
"#
);

// [`FillFromRelease`] with the tracks of several releases.
sqlive_compose::write!(
    FillFromReleases(
        releases: &'p str,
        base: i64,
        shuffled: bool,
        lead_track: Option<&'p str>,
        lead_position: Option<i64>,
    ),
    r#"
    INSERT INTO playback_context (entry, ordered, shuffled, track_id)
    SELECT :base + lane.played - 1, lane.ordered, lane.played, lane.track_id
    FROM @crate::tables::queue_fill::PlayOrder(
        shuffled: :shuffled, lead_track: :lead_track, lead_position: :lead_position,
        sourced: (
            SELECT track.track_id,
                   row_number() OVER (ORDER BY given.key, track.position, track.track_id)
            FROM json_each(:releases) AS given
            JOIN @bae_records::records::spread_fragments::TrackPosition AS track
                ON track.release_id = given.value)) AS lane
    ORDER BY lane.played
"#
);

// [`FillFromRelease`] with a list's tracks.
sqlive_compose::write!(
    FillFromTracks(
        tracks: &'p str,
        base: i64,
        shuffled: bool,
        lead_track: Option<&'p str>,
        lead_position: Option<i64>,
    ),
    r#"
    INSERT INTO playback_context (entry, ordered, shuffled, track_id)
    SELECT :base + lane.played - 1, lane.ordered, lane.played, lane.track_id
    FROM @crate::tables::queue_fill::PlayOrder(
        shuffled: :shuffled, lead_track: :lead_track, lead_position: :lead_position,
        sourced: (
            SELECT track.track_id, row_number() OVER (ORDER BY given.key)
            FROM json_each(:tracks) AS given
            JOIN @bae_records::records::spread_fragments::TrackPosition AS track
                ON track.track_id = given.value)) AS lane
    ORDER BY lane.played
"#
);

// [`FillFromRelease`] with every library track.
sqlive_compose::write!(
    FillFromLibrary(
        base: i64,
        shuffled: bool,
        lead_track: Option<&'p str>,
        lead_position: Option<i64>,
    ),
    r#"
    INSERT INTO playback_context (entry, ordered, shuffled, track_id)
    SELECT :base + lane.played - 1, lane.ordered, lane.played, lane.track_id
    FROM @crate::tables::queue_fill::PlayOrder(
        shuffled: :shuffled, lead_track: :lead_track, lead_position: :lead_position,
        sourced: (
            SELECT track.track_id,
                   row_number() OVER (
                       ORDER BY track.release_id, track.position, track.track_id)
            FROM @bae_records::records::spread_fragments::TrackPosition AS track)) AS lane
    ORDER BY lane.played
"#
);

// What the filled lane plays from, its playing row's places and the next free
// entry number.
sqlive_compose::write!(
    PlayFilledLane(
        source_kind: &'static str,
        source_release_id: Option<&'p str>,
        shuffled: bool,
        ordered_cursor: i64,
        shuffled_cursor: i64,
        next_entry: i64,
    ),
    r#"
    UPDATE playback_queue
    SET source_kind = :source_kind, source_release_id = :source_release_id,
        shuffled = :shuffled, ordered_cursor = :ordered_cursor,
        shuffled_cursor = :shuffled_cursor, next_entry = :next_entry
"#
);

/// The lane row a fill plays first.
enum FillCursor {
    First,
    /// The row of this track, which the one release played holds once.
    Track(String),
    /// The row at this position.
    Position(i64),
}

impl FillCursor {
    /// The row that leads the lane when it shuffles, so a shuffled play of one
    /// track still starts with it: its track, or its place in the source's
    /// order; neither when no row leads.
    fn lead(&self) -> (Option<&str>, Option<i64>) {
        match self {
            FillCursor::First => (None, None),
            FillCursor::Track(track_id) => (Some(track_id), None),
            FillCursor::Position(position) => (None, Some(*position)),
        }
    }
}

pub(super) fn fill_on(
    sql: &coven::BulkSqlContext<'_>,
    fill: ContextFill,
) -> Result<ContextFilled, DbError> {
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
                FillSource::Release(release_id),
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
                FillSource::Release(release_id),
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
                FillSource::Releases(playable_ids),
                false,
                FillCursor::First,
                LeftOut::releases(left_out),
            )
        }
        ContextFill::Tracks { tracks, start } => {
            // A given list's start index counts every track it names and the
            // fill names the ones the library lacks. A picked list counts only
            // the tracks the library holds, in the order their release lists
            // them, and names none.
            let left_out = match &tracks {
                bae_records::TrackList::Given(_) => sql.query(
                    "SELECT j.value FROM json_each(?) j \
                     WHERE NOT EXISTS (SELECT 1 FROM tracks WHERE id = j.value) \
                     ORDER BY j.key",
                    [tracks.ids_json()],
                    |row| row.get::<_, String>(0),
                )?,
                bae_records::TrackList::Picked(_) => Vec::new(),
            };
            let track_ids = tracks.ordered_leaving_out_missing_picks(sql)?;
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
                FillSource::Tracks(track_ids),
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
            (ContextSource::Library, FillSource::Library, true, FillCursor::First, LeftOut::Nothing)
        }
    };

    // A play the person asked shuffled shuffles the whole lane, and turns the
    // mode on. Any other play shuffles while the mode is on, from its start
    // track when it names one.
    let shuffled = whole_shuffled || queue_row_on(sql)?.shuffled;
    let (lead_track, lead_position) =
        if whole_shuffled || !shuffled { (None, None) } else { start.lead() };
    // Up Next empties and the context lane is replaced, with what it plays
    // from written below.
    sql.run(&ClearUpNextParams {})?;
    sql.run(&ClearContextLaneParams {})?;
    let base = queue_row_on(sql)?.next_entry;
    let track_count = match &rows {
        FillSource::Release(release) => {
            sql.run(&FillFromReleaseParams { release, base, shuffled, lead_track, lead_position })?
        }
        FillSource::Releases(releases) => sql.run(&FillFromReleasesParams {
            releases,
            base,
            shuffled,
            lead_track,
            lead_position,
        })?,
        FillSource::Tracks(tracks) => {
            sql.run(&FillFromTracksParams { tracks, base, shuffled, lead_track, lead_position })?
        }
        FillSource::Library => {
            sql.run(&FillFromLibraryParams { base, shuffled, lead_track, lead_position })?
        }
    };
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
    sql.run(&PlayFilledLaneParams {
        source_kind,
        source_release_id,
        shuffled,
        ordered_cursor,
        shuffled_cursor,
        next_entry: base + track_count as i64,
    })?;
    let (current, _) = sql.query_row(
        "SELECT entry, track_id, shuffled AS position FROM playback_context WHERE shuffled = ?",
        [shuffled_cursor],
        context_from_row,
    )?;
    Ok(ContextFilled::Playing { current, track_count, left_out })
}
