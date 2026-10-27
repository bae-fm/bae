//! The playback queue's tables. Up Next is `playback_manual`, one row per
//! entry, so a track queued twice is two rows; the context lane a play fills is
//! `playback_context`, keyed by its position in play order, one row per track.
//! The one `playback_queue` row says what the context lane plays from, which of
//! its rows is playing, and how the queue repeats. The playback service is the
//! only writer, each change one transaction; the queue screens read the same
//! tables through live queries.

use super::*;
use crate::playback::{ContextSource, QueueEntry, QueueEntryId, RepeatMode};

/// The lane a queue entry is in, and the table holding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Manual,
    Context,
}

impl Lane {
    fn table(self) -> &'static str {
        match self {
            Lane::Manual => "playback_manual",
            Lane::Context => "playback_context",
        }
    }
}

/// The lane an entry id names, the column naming the row in it, and the
/// value there.
fn row_key(id: &QueueEntryId) -> (Lane, &'static str, rusqlite::types::Value) {
    match id {
        QueueEntryId::Manual(entry) => (Lane::Manual, "entry", (*entry).into()),
        QueueEntryId::Context(track_id) => (Lane::Context, "track_id", track_id.clone().into()),
    }
}

/// What a play fills the context lane with.
#[derive(Debug, Clone)]
pub enum ContextFill {
    /// The release `track_id` is on, in its order, from that track.
    ReleaseOfTrack(String),
    /// One release in its order, from where `start` says.
    Release {
        release_id: String,
        start: ReleaseStart,
    },
    /// Releases one after another in the order given, each in its own order,
    /// from the first track. A release with no tracks is left out, of the lane
    /// and of the source the queue names; a release named twice plays once,
    /// where it is first named.
    Releases(Vec<String>),
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

/// What a fill did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextFilled {
    /// The lane holds `track_count` tracks and plays `current` first; Up Next
    /// is empty.
    Playing {
        current: QueueEntry,
        track_count: usize,
    },
    /// The release, the releases or the library hold no tracks. The queue is
    /// as it was.
    NoTracks,
    /// The track to start from is not on the release, or not in the library.
    /// The queue is as it was.
    StartMissing,
}

/// Where manual-lane additions go.
#[derive(Debug, Clone, Copy)]
pub enum ManualPlace {
    End,
    Front,
    /// Before the entry at this index of the lane; past its end is the end.
    At(usize),
}

/// What turning shuffle on or off did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShuffleChange {
    Changed,
    /// The context lane was already in that order.
    Unchanged,
    /// No context lane to order.
    NoContext,
}

/// The playback queue's tables, as the playback service changes them.
#[derive(Clone)]
pub struct PlaybackQueueTables {
    db: Database,
}

impl Database {
    pub fn playback_queue_tables(&self) -> PlaybackQueueTables {
        PlaybackQueueTables { db: self.clone() }
    }

    /// Empty the queue and the resume row unless the resume row is kept: what
    /// opening a library does when it will not resume what it played. A
    /// library with nothing to resume opens with an empty queue, and one
    /// opened with "restore on launch" off forgets what it played.
    pub async fn settle_playback_queue(&self, resume: bool) -> Result<(), DbError> {
        self.call(move |sql| {
            let stored = sql
                .query_row("SELECT 1 FROM playback_state", [], |_| Ok(()))
                .optional()?
                .is_some();
            if resume && stored {
                return Ok(());
            }
            sql.execute("DELETE FROM playback_state", [])?;
            sql.execute("DELETE FROM playback_manual", [])?;
            sql.execute("DELETE FROM playback_context", [])?;
            sql.execute("UPDATE playback_queue SET repeat = 'off'", [])?;
            Ok(())
        })
        .await
    }
}

impl PlaybackQueueTables {
    /// How the queue repeats.
    pub async fn repeat(&self) -> Result<RepeatMode, DbError> {
        self.db
            .read(|sql| {
                let repeat: String =
                    sql.query_row("SELECT repeat FROM playback_queue", [], |row| row.get(0))?;
                Ok(repeat_from_column(&repeat)?)
            })
            .await
    }

    /// How many tracks the context lane holds, played or not.
    pub async fn context_track_count(&self) -> Result<usize, DbError> {
        self.db
            .read(|sql| {
                let count: i64 =
                    sql.query_row("SELECT count(*) FROM playback_context", [], |row| {
                        row.get(0)
                    })?;
                Ok(count as usize)
            })
            .await
    }

    /// Replace the whole queue with what `fill` names: Up Next empties, and the
    /// context lane holds the tracks in play order, playing the first.
    pub async fn fill(&self, fill: ContextFill) -> Result<ContextFilled, DbError> {
        self.db.call(move |sql| fill_on(sql, fill)).await
    }

    /// Add tracks to Up Next where `place` says, leaving out any the library
    /// does not hold, and say how many went in.
    pub async fn add_manual(
        &self,
        track_ids: Vec<String>,
        place: ManualPlace,
    ) -> Result<usize, DbError> {
        self.db
            .call(move |sql| add_manual_on(sql, &track_ids, place))
            .await
    }

    /// Empty Up Next.
    pub async fn clear_manual(&self) -> Result<(), DbError> {
        self.db
            .call(|sql| {
                sql.execute("DELETE FROM playback_manual", [])?;
                Ok(())
            })
            .await
    }

    /// Empty the context lane; `false` when there was none.
    pub async fn clear_context(&self) -> Result<bool, DbError> {
        self.db
            .call(|sql| {
                if queue_row_on(sql)?.cursor.is_none() {
                    return Ok(false);
                }
                // Emptying the lane clears what it played from
                // (`playback_context_emptied`).
                sql.execute("DELETE FROM playback_context", [])?;
                Ok(true)
            })
            .await
    }

    /// Put the context lane's tracks after the playing one in shuffled order,
    /// or back in the order they had before it was shuffled. The playing track
    /// and the ones before it stay where they are.
    pub async fn set_shuffle(&self, on: bool) -> Result<ShuffleChange, DbError> {
        self.db.call(move |sql| set_shuffle_on(sql, on)).await
    }

    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), DbError> {
        self.db
            .call(move |sql| {
                sql.execute(
                    "UPDATE playback_queue SET repeat = ?",
                    [repeat_column(mode)],
                )?;
                Ok(())
            })
            .await
    }

    /// Take the entry out of whichever lane holds it; `None` when no lane does.
    pub async fn remove(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.db
            .call(move |sql| {
                let (lane, column, key) = row_key(&id);
                let removed = sql
                    .query_row(
                        &format!(
                            "DELETE FROM {table} WHERE {column} = ? RETURNING track_id",
                            table = lane.table()
                        ),
                        [key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let Some(track_id) = removed else {
                    return Ok(None);
                };
                Ok(Some(QueueEntry { id, track_id }))
            })
            .await
    }

    /// Move the entry to sit just before `before`, or to the end of its lane
    /// without one. Both must be in the same lane; `false` when either is in
    /// none or they are in different ones. The context lane's playing track
    /// stays the playing track wherever it moves.
    pub async fn reorder(
        &self,
        id: QueueEntryId,
        before: Option<QueueEntryId>,
    ) -> Result<bool, DbError> {
        self.db.call(move |sql| reorder_on(sql, id, before)).await
    }

    /// Make the entry the one playing: an Up Next entry leaves the lane with
    /// the ones ahead of it, a context entry becomes the lane's playing track.
    /// `None` when no lane holds it.
    pub async fn skip_to(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.db
            .call(move |sql| {
                let Some(located) = locate_on(sql, id)? else {
                    return Ok(None);
                };
                match located.lane {
                    Lane::Manual => {
                        sql.execute(
                            "DELETE FROM playback_manual WHERE position <= ?",
                            [located.position],
                        )?;
                    }
                    Lane::Context => set_cursor_on(sql, located.position)?,
                }
                Ok(Some(located.entry))
            })
            .await
    }

    /// Play on: the first Up Next entry leaves the lane, or the context lane's
    /// next track becomes its playing one. `None` when neither lane has a
    /// track after the playing one.
    pub async fn advance(&self) -> Result<Option<QueueEntry>, DbError> {
        self.db.call(advance_on).await
    }

    /// Play the context lane again from its first track, in a new shuffled
    /// order when it is shuffled. `None` without a context lane.
    pub async fn wrap(&self) -> Result<Option<QueueEntry>, DbError> {
        self.db.call(wrap_on).await
    }

    /// Step the context lane back: to the track before its playing one when
    /// `current` is that playing track, otherwise to the playing track itself,
    /// or the one before it when it has left the lane. `None` when there is
    /// nothing to step back to.
    pub async fn step_back(
        &self,
        current: Option<QueueEntryId>,
    ) -> Result<Option<QueueEntry>, DbError> {
        self.db.call(move |sql| step_back_on(sql, current)).await
    }

    /// The track that plays next: Up Next's first, or else the context lane's
    /// track after the playing one.
    pub async fn front(&self) -> Result<Option<String>, DbError> {
        self.db
            .read(|sql| {
                let manual = sql
                    .query_row(
                        "SELECT track_id FROM playback_manual ORDER BY position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?;
                if manual.is_some() {
                    return Ok(manual);
                }
                Ok(sql
                    .query_row(
                        "SELECT track_id FROM playback_context \
                         WHERE position > (SELECT cursor FROM playback_queue) \
                         ORDER BY position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
    }

    /// The context lane's next track when it plays straight on from the
    /// playing one: Up Next empty and the lane in its own order. The only
    /// track a side or disc pause can fall before.
    pub async fn next_in_order(&self) -> Result<Option<String>, DbError> {
        self.db
            .read(|sql| {
                Ok(sql
                    .query_row(
                        "SELECT c.track_id FROM playback_queue q \
                         JOIN playback_context c ON c.position > q.cursor \
                         WHERE q.shuffled = 0 \
                           AND NOT EXISTS (SELECT 1 FROM playback_manual) \
                         ORDER BY c.position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
    }

    /// Every track of the context lane, played or not, in lane order.
    #[cfg(test)]
    pub async fn context_order(&self) -> Result<Vec<String>, DbError> {
        self.db
            .read(|sql| {
                Ok(sql.query(
                    "SELECT track_id FROM playback_context ORDER BY position",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

/// The `playback_queue` row's columns the lane changes read.
struct QueueRow {
    cursor: Option<i64>,
    shuffled: Option<bool>,
    next_entry: i64,
}

fn queue_row_on(sql: &SqlContext<'_, '_>) -> rusqlite::Result<QueueRow> {
    sql.query_row(
        "SELECT cursor, shuffled, next_entry FROM playback_queue",
        [],
        |row| {
            Ok(QueueRow {
                cursor: row.get("cursor")?,
                shuffled: row.get("shuffled")?,
                next_entry: row.get("next_entry")?,
            })
        },
    )
}

fn set_cursor_on(sql: &SqlContext<'_, '_>, position: i64) -> rusqlite::Result<()> {
    sql.execute("UPDATE playback_queue SET cursor = ?", [position])?;
    Ok(())
}

/// An entry and where it sits.
struct Located {
    entry: QueueEntry,
    lane: Lane,
    position: i64,
}

fn locate_on(sql: &SqlContext<'_, '_>, id: QueueEntryId) -> rusqlite::Result<Option<Located>> {
    let (lane, column, key) = row_key(&id);
    sql.query_row(
        &format!(
            "SELECT position, track_id FROM {table} WHERE {column} = ?",
            table = lane.table()
        ),
        [key],
        |row| {
            Ok(Located {
                lane,
                position: row.get("position")?,
                entry: QueueEntry {
                    id,
                    track_id: row.get("track_id")?,
                },
            })
        },
    )
    .optional()
}

/// An Up Next row read with its `entry`, `track_id` and `position`.
fn manual_from_row(row: &Row<'_>) -> rusqlite::Result<(QueueEntry, i64)> {
    Ok((
        QueueEntry {
            id: QueueEntryId::Manual(row.get("entry")?),
            track_id: row.get("track_id")?,
        },
        row.get("position")?,
    ))
}

/// A context row read with its `track_id` and `position`.
fn context_from_row(row: &Row<'_>) -> rusqlite::Result<(QueueEntry, i64)> {
    let track_id: String = row.get("track_id")?;
    Ok((
        QueueEntry {
            id: QueueEntryId::Context(track_id.clone()),
            track_id,
        },
        row.get("position")?,
    ))
}

/// The highest position in `lane`, `None` when it is empty.
fn lane_top_on(sql: &SqlContext<'_, '_>, lane: Lane) -> rusqlite::Result<Option<i64>> {
    sql.query_row(
        &format!("SELECT max(position) FROM {}", lane.table()),
        [],
        |row| row.get(0),
    )
}

/// Move `lane`'s rows at positions `low..=high` by `delta`. Positions are
/// unique in a lane at every statement, so the rows first go above every
/// position the lane holds, then come down to where they belong. The caller
/// has left the positions they come down to free.
fn shift_on(
    sql: &SqlContext<'_, '_>,
    lane: Lane,
    low: i64,
    high: i64,
    delta: i64,
) -> rusqlite::Result<()> {
    if low > high {
        return Ok(());
    }
    let Some(top) = lane_top_on(sql, lane)? else {
        return Ok(());
    };
    let lift = top - low + delta.abs() + 1;
    sql.execute(
        &format!(
            "UPDATE {} SET position = position + ?1 WHERE position BETWEEN ?2 AND ?3",
            lane.table()
        ),
        params![lift, low, high],
    )?;
    sql.execute(
        &format!(
            "UPDATE {} SET position = position - ?1 + ?2 WHERE position >= ?3",
            lane.table()
        ),
        params![lift, delta, low + lift],
    )?;
    Ok(())
}

/// Give the context lane's rows after `after` new positions above every one
/// it holds, in the order `order` sorts them by.
fn reorder_context_after_on(
    sql: &SqlContext<'_, '_>,
    after: i64,
    order: &str,
) -> rusqlite::Result<()> {
    let Some(top) = lane_top_on(sql, Lane::Context)? else {
        return Ok(());
    };
    sql.execute(
        &format!(
            "UPDATE playback_context SET position = ?1 + placed.k \
             FROM (SELECT track_id, row_number() OVER (ORDER BY {order}) AS k \
                   FROM playback_context WHERE position > ?2) AS placed \
             WHERE playback_context.track_id = placed.track_id"
        ),
        params![top, after],
    )?;
    Ok(())
}

/// The tracks a fill puts in the lane, as a `FROM … WHERE …` over `tracks t`
/// binding `:source` when it names one, and the source order that ranks them.
struct FillSource {
    from: &'static str,
    source_order: &'static str,
    source: Option<String>,
}

fn fill_on(sql: &SqlContext<'_, '_>, fill: ContextFill) -> Result<ContextFilled, DbError> {
    let release_source = |release_id: String| FillSource {
        from: "tracks t WHERE t.release_id = :source",
        source_order: "t.position, t.id",
        source: Some(release_id),
    };
    let (source, rows, shuffled, start_track) = match fill {
        ContextFill::ReleaseOfTrack(track_id) => {
            let release_id: Option<String> = sql
                .query_row(
                    "SELECT release_id FROM tracks WHERE id = ?",
                    [&track_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(release_id) = release_id else {
                return Ok(ContextFilled::StartMissing);
            };
            (
                ContextSource::Release(release_id.clone()),
                release_source(release_id),
                false,
                Some(track_id),
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
            let (shuffled, start_track) = match start {
                ReleaseStart::First => (false, None),
                ReleaseStart::Shuffled => (true, None),
                ReleaseStart::Track(track_id) => {
                    let on_release: bool = sql.query_row(
                        "SELECT EXISTS (SELECT 1 FROM tracks WHERE id = ? AND release_id = ?)",
                        [&track_id, &release_id],
                        |row| row.get(0),
                    )?;
                    if !on_release {
                        return Ok(ContextFilled::StartMissing);
                    }
                    (false, Some(track_id))
                }
            };
            (
                ContextSource::Release(release_id.clone()),
                release_source(release_id),
                shuffled,
                start_track,
            )
        }
        ContextFill::Releases(release_ids) => {
            let release_ids = serde_json::to_string(&release_ids)
                .expect("serializing a Vec<String> to JSON cannot fail");
            // A release named twice plays once, where it is first named.
            let playable: Vec<String> = sql.query(
                "SELECT j.value FROM json_each(?) j \
                 WHERE EXISTS (SELECT 1 FROM tracks WHERE release_id = j.value) \
                 GROUP BY j.value ORDER BY min(j.key)",
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
                None,
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
                None,
            )
        }
    };

    // Up Next empties and the context lane is replaced; emptying it clears
    // what it played from (`playback_context_emptied`).
    sql.execute("DELETE FROM playback_manual", [])?;
    sql.execute("DELETE FROM playback_context", [])?;
    let play_order = if shuffled { "random()" } else { "unshuffled" };
    let bound: Vec<(&str, &dyn rusqlite::ToSql)> = match &rows.source {
        Some(source) => vec![(":source", source)],
        None => Vec::new(),
    };
    // Inserted in position order, so the table grows at its end.
    let track_count = sql.execute(
        &format!(
            "INSERT INTO playback_context (position, track_id, unshuffled) \
             SELECT position, track_id, unshuffled FROM ( \
                 SELECT track_id, unshuffled, \
                        row_number() OVER (ORDER BY {play_order}) AS position \
                 FROM (SELECT t.id AS track_id, \
                              row_number() OVER (ORDER BY {source_order}) AS unshuffled \
                       FROM {from})) \
             ORDER BY position",
            source_order = rows.source_order,
            from = rows.from,
        ),
        bound.as_slice(),
    )?;
    let cursor: i64 = match start_track {
        Some(track_id) => sql.query_row(
            "SELECT position FROM playback_context WHERE track_id = ?",
            [track_id],
            |row| row.get(0),
        )?,
        None => 1,
    };
    sql.execute(
        "UPDATE playback_queue SET source = ?, shuffled = ?, cursor = ?",
        params![source.to_column(), shuffled, cursor],
    )?;
    let (current, _) = sql.query_row(
        "SELECT track_id, position FROM playback_context WHERE position = ?",
        [cursor],
        context_from_row,
    )?;
    Ok(ContextFilled::Playing {
        current,
        track_count,
    })
}

fn add_manual_on(
    sql: &SqlContext<'_, '_>,
    track_ids: &[String],
    place: ManualPlace,
) -> Result<usize, DbError> {
    let track_ids =
        serde_json::to_string(track_ids).expect("serializing a Vec<String> to JSON cannot fail");
    let count: i64 = sql.query_row(
        "SELECT count(*) FROM json_each(?) j JOIN tracks t ON t.id = j.value",
        [&track_ids],
        |row| row.get(0),
    )?;
    if count == 0 {
        return Ok(0);
    }
    let bottom: Option<i64> =
        sql.query_row("SELECT min(position) FROM playback_manual", [], |row| {
            row.get(0)
        })?;
    let top = lane_top_on(sql, Lane::Manual)?;
    let end = top.map_or(1, |top| top + 1);
    let start = match place {
        ManualPlace::End => end,
        ManualPlace::Front => bottom.map_or(1, |bottom| bottom - count),
        ManualPlace::At(index) => {
            let at: Option<i64> = sql
                .query_row(
                    "SELECT position FROM playback_manual ORDER BY position LIMIT 1 OFFSET ?",
                    [index as i64],
                    |row| row.get(0),
                )
                .optional()?;
            match at {
                Some(at) => {
                    shift_on(sql, Lane::Manual, at, end - 1, count)?;
                    at
                }
                None => end,
            }
        }
    };
    let base = queue_row_on(sql)?.next_entry;
    sql.execute(
        "INSERT INTO playback_manual (entry, position, track_id) \
         SELECT ?2 + k, ?3 + k, track_id FROM ( \
             SELECT t.id AS track_id, row_number() OVER (ORDER BY j.key) - 1 AS k \
             FROM json_each(?1) j JOIN tracks t ON t.id = j.value)",
        params![track_ids, base, start],
    )?;
    sql.execute("UPDATE playback_queue SET next_entry = ?", [base + count])?;
    Ok(count as usize)
}

fn set_shuffle_on(sql: &SqlContext<'_, '_>, on: bool) -> Result<ShuffleChange, DbError> {
    let row = queue_row_on(sql)?;
    let (Some(cursor), Some(shuffled)) = (row.cursor, row.shuffled) else {
        return Ok(ShuffleChange::NoContext);
    };
    if shuffled == on {
        return Ok(ShuffleChange::Unchanged);
    }
    if on {
        // The order shuffle returns to is the lane's order now.
        sql.execute("UPDATE playback_context SET unshuffled = position", [])?;
        reorder_context_after_on(sql, cursor, "random()")?;
    } else {
        reorder_context_after_on(sql, cursor, "unshuffled, position")?;
    }
    sql.execute("UPDATE playback_queue SET shuffled = ?", [on])?;
    Ok(ShuffleChange::Changed)
}

fn reorder_on(
    sql: &SqlContext<'_, '_>,
    id: QueueEntryId,
    before: Option<QueueEntryId>,
) -> Result<bool, DbError> {
    let Some(moving) = locate_on(sql, id.clone())? else {
        warn!("reorder: queue entry id not found: {id}");
        return Ok(false);
    };
    let to = match before {
        None => None,
        Some(before) => match locate_on(sql, before.clone())? {
            Some(target) if target.lane == moving.lane => Some(target.position),
            Some(_) => {
                warn!(
                    "reorder: entry {id} and the entry {before} it goes before are in different lanes"
                );
                return Ok(false);
            }
            None => {
                warn!("reorder: before id not found: {before}");
                return Ok(false);
            }
        },
    };
    let lane = moving.lane;
    let from = moving.position;
    let top = lane_top_on(sql, lane)?.expect("the lane holds the entry being moved");
    let (_, column, key) = row_key(&id);
    let park = |position: i64| {
        sql.execute(
            &format!(
                "UPDATE {table} SET position = ? WHERE {column} = ?",
                table = lane.table()
            ),
            params![position, key],
        )
    };
    // Where the entry lands, and how a context cursor between the two places
    // moves so it stays on the same track or between the same two.
    let (landing, cursor_shift): (i64, Option<(i64, i64, i64)>) = match to {
        None => {
            park(top + 1)?;
            (top + 1, None)
        }
        Some(to) if to == from => (from, None),
        Some(to) if to < from => {
            park(top + 1)?;
            shift_on(sql, lane, to, from - 1, 1)?;
            park(to)?;
            (to, Some((to, from - 1, 1)))
        }
        Some(to) => {
            park(top + 1)?;
            shift_on(sql, lane, from + 1, to - 1, -1)?;
            park(to - 1)?;
            (to - 1, Some((from + 1, to - 1, -1)))
        }
    };
    if lane == Lane::Context {
        let cursor = queue_row_on(sql)?
            .cursor
            .expect("a context lane with an entry has a cursor");
        let moved_cursor = if cursor == from {
            landing
        } else {
            match cursor_shift {
                Some((low, high, delta)) if (low..=high).contains(&cursor) => cursor + delta,
                _ => cursor,
            }
        };
        if moved_cursor != cursor {
            set_cursor_on(sql, moved_cursor)?;
        }
    }
    Ok(true)
}

fn advance_on(sql: &SqlContext<'_, '_>) -> Result<Option<QueueEntry>, DbError> {
    let manual_front = sql
        .query_row(
            "SELECT entry, track_id, position FROM playback_manual ORDER BY position LIMIT 1",
            [],
            manual_from_row,
        )
        .optional()?;
    if let Some((entry, position)) = manual_front {
        sql.execute("DELETE FROM playback_manual WHERE position = ?", [position])?;
        return Ok(Some(entry));
    }
    let next = sql
        .query_row(
            "SELECT track_id, position FROM playback_context \
             WHERE position > (SELECT cursor FROM playback_queue) \
             ORDER BY position LIMIT 1",
            [],
            context_from_row,
        )
        .optional()?;
    let Some((entry, position)) = next else {
        return Ok(None);
    };
    set_cursor_on(sql, position)?;
    Ok(Some(entry))
}

fn wrap_on(sql: &SqlContext<'_, '_>) -> Result<Option<QueueEntry>, DbError> {
    let row = queue_row_on(sql)?;
    let Some(shuffled) = row.shuffled else {
        return Ok(None);
    };
    if shuffled {
        // A new pass in a new order over the rows the lane holds now. The
        // order unshuffling returns to stays the one the lane had when it was
        // shuffled.
        reorder_context_after_on(sql, i64::MIN, "random()")?;
    }
    let (first, position) = sql.query_row(
        "SELECT track_id, position FROM playback_context ORDER BY position LIMIT 1",
        [],
        context_from_row,
    )?;
    set_cursor_on(sql, position)?;
    Ok(Some(first))
}

fn step_back_on(
    sql: &SqlContext<'_, '_>,
    current: Option<QueueEntryId>,
) -> Result<Option<QueueEntry>, DbError> {
    let Some(cursor) = queue_row_on(sql)?.cursor else {
        return Ok(None);
    };
    let at_cursor: Option<String> = sql
        .query_row(
            "SELECT track_id FROM playback_context WHERE position = ?",
            [cursor],
            |row| row.get(0),
        )
        .optional()?;
    let playing_cursor = matches!(
        (current, at_cursor),
        (Some(QueueEntryId::Context(current)), Some(at_cursor)) if current == at_cursor
    );
    let comparison = if playing_cursor { "<" } else { "<=" };
    let target = sql
        .query_row(
            &format!(
                "SELECT track_id, position FROM playback_context \
                 WHERE position {comparison} ? ORDER BY position DESC LIMIT 1"
            ),
            [cursor],
            context_from_row,
        )
        .optional()?;
    let Some((entry, position)) = target else {
        return Ok(None);
    };
    set_cursor_on(sql, position)?;
    Ok(Some(entry))
}

pub(crate) fn repeat_column(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Context => "context",
    }
}

pub(super) fn repeat_from_column(value: &str) -> rusqlite::Result<RepeatMode> {
    match value {
        "off" => Ok(RepeatMode::Off),
        "track" => Ok(RepeatMode::Track),
        "context" => Ok(RepeatMode::Context),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown repeat mode {other:?}").into(),
        )),
    }
}
