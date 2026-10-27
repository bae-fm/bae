//! The playback queue's tables. Each queued track is a row of
//! `playback_queue_entries`, in the manual lane (Up Next) or the context lane
//! (what a play filled), ordered by its position there; the one
//! `playback_queue` row says what the context lane plays from, which of its
//! rows is playing, and how the queue repeats. The playback service is the only
//! writer, each change one transaction; the queue screens read the same tables
//! through live queries.

use super::*;
use crate::playback::{ContextSource, QueueEntry, QueueEntryId, RepeatMode};

/// The lane a queue entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Manual,
    Context,
}

impl Lane {
    fn column(self) -> &'static str {
        match self {
            Lane::Manual => "manual",
            Lane::Context => "context",
        }
    }

    fn from_column(value: &str) -> rusqlite::Result<Lane> {
        match value {
            "manual" => Ok(Lane::Manual),
            "context" => Ok(Lane::Context),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("unknown queue lane {other:?}").into(),
            )),
        }
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
    /// and of the source the queue names.
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
            sql.execute("DELETE FROM playback_queue_entries", [])?;
            sql.execute(
                "UPDATE playback_queue SET repeat = 'off', revision = revision + 1",
                [],
            )?;
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
                let count: i64 = sql.query_row(
                    "SELECT count(*) FROM playback_queue_entries WHERE lane = 'context'",
                    [],
                    |row| row.get(0),
                )?;
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
                sql.execute(
                    "DELETE FROM playback_queue_entries WHERE lane = 'manual'",
                    [],
                )?;
                bump_revision_on(sql)
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
                sql.execute(
                    "DELETE FROM playback_queue_entries WHERE lane = 'context'",
                    [],
                )?;
                bump_revision_on(sql)?;
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
                bump_revision_on(sql)
            })
            .await
    }

    /// Take the entry out of whichever lane holds it; `None` when no lane does.
    pub async fn remove(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.db
            .call(move |sql| {
                let removed = sql
                    .query_row(
                        "DELETE FROM playback_queue_entries WHERE entry = ? RETURNING track_id",
                        [id.0],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let Some(track_id) = removed else {
                    return Ok(None);
                };
                bump_revision_on(sql)?;
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
                            "DELETE FROM playback_queue_entries \
                             WHERE lane = 'manual' AND position <= ?",
                            [located.position],
                        )?;
                    }
                    Lane::Context => set_cursor_on(sql, located.position)?,
                }
                bump_revision_on(sql)?;
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
                        "SELECT track_id FROM playback_queue_entries WHERE lane = 'manual' \
                         ORDER BY position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?;
                if manual.is_some() {
                    return Ok(manual);
                }
                Ok(sql
                    .query_row(
                        "SELECT track_id FROM playback_queue_entries \
                         WHERE lane = 'context' \
                           AND position > (SELECT cursor FROM playback_queue) \
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
                        "SELECT e.track_id FROM playback_queue q \
                         JOIN playback_queue_entries e \
                           ON e.lane = 'context' AND e.position > q.cursor \
                         WHERE q.shuffled = 0 \
                           AND NOT EXISTS ( \
                               SELECT 1 FROM playback_queue_entries WHERE lane = 'manual') \
                         ORDER BY e.position LIMIT 1",
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
                    "SELECT track_id FROM playback_queue_entries WHERE lane = 'context' \
                     ORDER BY position",
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

fn bump_revision_on(sql: &SqlContext<'_, '_>) -> Result<(), DbError> {
    sql.execute("UPDATE playback_queue SET revision = revision + 1", [])?;
    Ok(())
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
    sql.query_row(
        "SELECT lane, position, track_id FROM playback_queue_entries WHERE entry = ?",
        [id.0],
        |row| {
            Ok(Located {
                lane: Lane::from_column(&row.get::<_, String>("lane")?)?,
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

fn entry_from_row(row: &Row<'_>) -> rusqlite::Result<(QueueEntry, i64)> {
    Ok((
        QueueEntry {
            id: QueueEntryId(row.get("entry")?),
            track_id: row.get("track_id")?,
        },
        row.get("position")?,
    ))
}

/// The highest position in `lane`, `None` when it is empty.
fn lane_top_on(sql: &SqlContext<'_, '_>, lane: Lane) -> rusqlite::Result<Option<i64>> {
    sql.query_row(
        "SELECT max(position) FROM playback_queue_entries WHERE lane = ?",
        [lane.column()],
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
        "UPDATE playback_queue_entries SET position = position + ?1 \
         WHERE lane = ?2 AND position BETWEEN ?3 AND ?4",
        params![lift, lane.column(), low, high],
    )?;
    sql.execute(
        "UPDATE playback_queue_entries SET position = position - ?1 + ?2 \
         WHERE lane = ?3 AND position >= ?4",
        params![lift, delta, lane.column(), low + lift],
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
            "UPDATE playback_queue_entries SET position = ?1 + placed.k \
             FROM (SELECT entry, row_number() OVER (ORDER BY {order}) AS k \
                   FROM playback_queue_entries \
                   WHERE lane = 'context' AND position > ?2) AS placed \
             WHERE playback_queue_entries.entry = placed.entry"
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
            let playable: Vec<String> = sql.query(
                "SELECT j.value FROM json_each(?) j \
                 WHERE EXISTS (SELECT 1 FROM tracks WHERE release_id = j.value) \
                 ORDER BY j.key",
                [&release_ids],
                |row| row.get(0),
            )?;
            if playable.is_empty() {
                return Ok(ContextFilled::NoTracks);
            }
            (
                ContextSource::releases(playable),
                FillSource {
                    from: "json_each(:source) j JOIN tracks t ON t.release_id = j.value",
                    source_order: "j.key, t.position, t.id",
                    source: Some(release_ids),
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
    sql.execute("DELETE FROM playback_queue_entries", [])?;
    let base = queue_row_on(sql)?.next_entry;
    let play_order = if shuffled { "random()" } else { "unshuffled" };
    // Entries `base`, `base + 1`, … in play order.
    let first_entry_less_one = base - 1;
    let mut bound: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":base", &first_entry_less_one)];
    if let Some(source) = &rows.source {
        bound.push((":source", source));
    }
    let track_count = sql.execute(
        &format!(
            "INSERT INTO playback_queue_entries (entry, lane, position, track_id, unshuffled) \
             SELECT :base + position, 'context', position, track_id, unshuffled FROM ( \
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
            "SELECT position FROM playback_queue_entries \
             WHERE lane = 'context' AND track_id = ? ORDER BY position LIMIT 1",
            [track_id],
            |row| row.get(0),
        )?,
        None => 1,
    };
    sql.execute(
        "UPDATE playback_queue SET source = ?, shuffled = ?, cursor = ?, \
         next_entry = ?, revision = revision + 1",
        params![
            source.to_column(),
            shuffled,
            cursor,
            base + track_count as i64
        ],
    )?;
    let (current, _) = sql.query_row(
        "SELECT entry, track_id, position FROM playback_queue_entries \
         WHERE lane = 'context' AND position = ?",
        [cursor],
        entry_from_row,
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
    let bottom: Option<i64> = sql.query_row(
        "SELECT min(position) FROM playback_queue_entries WHERE lane = 'manual'",
        [],
        |row| row.get(0),
    )?;
    let top = lane_top_on(sql, Lane::Manual)?;
    let end = top.map_or(1, |top| top + 1);
    let start = match place {
        ManualPlace::End => end,
        ManualPlace::Front => bottom.map_or(1, |bottom| bottom - count),
        ManualPlace::At(index) => {
            let at: Option<i64> = sql
                .query_row(
                    "SELECT position FROM playback_queue_entries WHERE lane = 'manual' \
                     ORDER BY position LIMIT 1 OFFSET ?",
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
        "INSERT INTO playback_queue_entries (entry, lane, position, track_id) \
         SELECT ?2 + k, 'manual', ?3 + k, track_id FROM ( \
             SELECT t.id AS track_id, row_number() OVER (ORDER BY j.key) - 1 AS k \
             FROM json_each(?1) j JOIN tracks t ON t.id = j.value)",
        params![track_ids, base, start],
    )?;
    sql.execute(
        "UPDATE playback_queue SET next_entry = ?, revision = revision + 1",
        [base + count],
    )?;
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
        sql.execute(
            "UPDATE playback_queue_entries SET unshuffled = position WHERE lane = 'context'",
            [],
        )?;
        reorder_context_after_on(sql, cursor, "random()")?;
    } else {
        reorder_context_after_on(sql, cursor, "unshuffled, entry")?;
    }
    sql.execute(
        "UPDATE playback_queue SET shuffled = ?, revision = revision + 1",
        [on],
    )?;
    Ok(ShuffleChange::Changed)
}

fn reorder_on(
    sql: &SqlContext<'_, '_>,
    id: QueueEntryId,
    before: Option<QueueEntryId>,
) -> Result<bool, DbError> {
    let Some(moving) = locate_on(sql, id)? else {
        warn!("reorder: queue entry id not found: {}", id.0);
        return Ok(false);
    };
    let to = match before {
        None => None,
        Some(before) => match locate_on(sql, before)? {
            Some(target) if target.lane == moving.lane => Some(target.position),
            Some(_) => {
                warn!(
                    "reorder: entry {} and the entry {} it goes before are in different lanes",
                    id.0, before.0
                );
                return Ok(false);
            }
            None => {
                warn!("reorder: before id not found: {}", before.0);
                return Ok(false);
            }
        },
    };
    let lane = moving.lane;
    let from = moving.position;
    let top = lane_top_on(sql, lane)?.expect("the lane holds the entry being moved");
    let park = |position: i64| {
        sql.execute(
            "UPDATE playback_queue_entries SET position = ? WHERE entry = ?",
            [position, id.0],
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
    bump_revision_on(sql)?;
    Ok(true)
}

fn advance_on(sql: &SqlContext<'_, '_>) -> Result<Option<QueueEntry>, DbError> {
    let manual_front = sql
        .query_row(
            "SELECT entry, track_id, position FROM playback_queue_entries \
             WHERE lane = 'manual' ORDER BY position LIMIT 1",
            [],
            entry_from_row,
        )
        .optional()?;
    if let Some((entry, _)) = manual_front {
        sql.execute(
            "DELETE FROM playback_queue_entries WHERE entry = ?",
            [entry.id.0],
        )?;
        bump_revision_on(sql)?;
        return Ok(Some(entry));
    }
    let next = sql
        .query_row(
            "SELECT entry, track_id, position FROM playback_queue_entries \
             WHERE lane = 'context' AND position > (SELECT cursor FROM playback_queue) \
             ORDER BY position LIMIT 1",
            [],
            entry_from_row,
        )
        .optional()?;
    let Some((entry, position)) = next else {
        return Ok(None);
    };
    set_cursor_on(sql, position)?;
    bump_revision_on(sql)?;
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
        "SELECT entry, track_id, position FROM playback_queue_entries \
         WHERE lane = 'context' ORDER BY position LIMIT 1",
        [],
        entry_from_row,
    )?;
    set_cursor_on(sql, position)?;
    bump_revision_on(sql)?;
    Ok(Some(first))
}

fn step_back_on(
    sql: &SqlContext<'_, '_>,
    current: Option<QueueEntryId>,
) -> Result<Option<QueueEntry>, DbError> {
    let Some(cursor) = queue_row_on(sql)?.cursor else {
        return Ok(None);
    };
    let at_cursor: Option<i64> = sql
        .query_row(
            "SELECT entry FROM playback_queue_entries WHERE lane = 'context' AND position = ?",
            [cursor],
            |row| row.get(0),
        )
        .optional()?;
    let playing_cursor = current.is_some_and(|current| at_cursor == Some(current.0));
    let comparison = if playing_cursor { "<" } else { "<=" };
    let target = sql
        .query_row(
            &format!(
                "SELECT entry, track_id, position FROM playback_queue_entries \
                 WHERE lane = 'context' AND position {comparison} ? \
                 ORDER BY position DESC LIMIT 1"
            ),
            [cursor],
            entry_from_row,
        )
        .optional()?;
    let Some((entry, position)) = target else {
        return Ok(None);
    };
    set_cursor_on(sql, position)?;
    bump_revision_on(sql)?;
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
