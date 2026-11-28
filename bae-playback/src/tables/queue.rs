//! The playback queue's tables. Up Next is `playback_manual`, the context lane
//! a play fills is `playback_context`; in both each row is an entry of its own,
//! named by its entry number and ordered by its position, so a track queued
//! twice is two rows. The one `playback_queue` row says what the context lane
//! plays from, which of its rows is playing, how the queue repeats and
//! whether it shuffles. The
//! playback service is the only writer, each change one transaction; the queue
//! screens read the same tables through live queries. Its tables, `playback_state`
//! included, are `queue.sql`.

use super::queue_fill::{fill_on, ContextFill, ContextFilled};
use super::queue_order::{reorder_on, shift_on, LaneOrder};
use super::queue_steps;
use super::*;
use crate::{QueueEntry, QueueEntryId, QueueLane, RepeatMode};
use bae_store::{statement_optional, statement_row, WriteStatements, WriterRead};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};

/// The lane an entry id names and the entry number of its row there.
pub(super) fn row_key(id: &QueueEntryId) -> (QueueLane, i64) {
    match id {
        QueueEntryId::Manual(entry) => (QueueLane::Manual, *entry),
        QueueEntryId::Context(entry) => (QueueLane::Context, *entry),
    }
}

/// Where manual-lane additions go.
#[derive(Debug, Clone, Copy)]
pub enum ManualPlace {
    End,
    Front,
    /// Before the entry at this index of the lane; past its end is the end.
    At(usize),
}

/// The playback queue's tables, as the playback service changes them.
#[derive(Clone)]
pub struct PlaybackQueueTables {
    store: Store,
}

impl PlaybackQueueTables {
    /// The tables over `store`.
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

coven::write!(ClearUpNext, r#"DELETE FROM playback_manual"#);

coven::write!(ClearContextLane, r#"DELETE FROM playback_context"#);

// Where the lane is empty, what it plays from cleared: its source and its
// cursor, which the schema keeps NULL together exactly while the lane is empty.
// The shuffle mode stays. The schema's `tracks_leave_playback_queue` trigger
// writes the same clear when a deleted track was the lane's last.
coven::write!(
    ClearEmptiedContextSource,
    r#"
    UPDATE playback_queue
    SET source_kind = NULL, source_release_id = NULL, ordered_cursor = NULL,
        shuffled_cursor = NULL
    WHERE source_kind IS NOT NULL AND NOT EXISTS (SELECT 1 FROM playback_context)
"#
);

coven::write!(ResetQueueModes, r#"UPDATE playback_queue SET repeat = 'off', shuffled = 0"#);

coven::write!(SetRepeat(repeat: &'static str), r#"UPDATE playback_queue SET repeat = :repeat"#);

coven::write!(SetShuffle(shuffled: bool), r#"UPDATE playback_queue SET shuffled = :shuffled"#);

coven::write!(
    SetNextEntry(next_entry: i64),
    r#"UPDATE playback_queue SET next_entry = :next_entry"#
);

coven::write!(
    RemoveFromUpNext(entry: i64),
    r#"DELETE FROM playback_manual WHERE entry = :entry RETURNING track_id"#
);

coven::write!(
    RemoveFromContextLane(entry: i64),
    r#"DELETE FROM playback_context WHERE entry = :entry RETURNING track_id"#
);

coven::write!(
    DeleteUpNextAt(position: i64),
    r#"DELETE FROM playback_manual WHERE position = :position"#
);

coven::write!(
    DeleteUpNextThrough(position: i64),
    r#"DELETE FROM playback_manual WHERE position <= :position"#
);

coven::write!(
    PlayContextEntry(entry: i64),
    r#"
    UPDATE playback_queue
    SET ordered_cursor = (SELECT ordered FROM playback_context WHERE entry = :entry),
        shuffled_cursor = (SELECT shuffled FROM playback_context WHERE entry = :entry)
"#
);

// The playing row's place in the shuffled order whose places start above `top`,
// where the rows up to `cursor` in the lane's own order come first.
coven::write!(
    PlayShuffledAfter(top: i64, cursor: i64),
    r#"
    UPDATE playback_queue
    SET shuffled_cursor =
        :top + (SELECT count(*) FROM playback_context WHERE ordered <= :cursor)
"#
);

coven::write!(
    ShuffleWholeLane(top: i64),
    r#"
    UPDATE playback_context SET shuffled = :top + placed.k
    FROM (SELECT entry, row_number() OVER (ORDER BY random()) AS k
          FROM playback_context) AS placed
    WHERE playback_context.entry = placed.entry
"#
);

coven::write!(
    ShuffleLaneAfter(top: i64, cursor: i64),
    r#"
    UPDATE playback_context SET shuffled = :top + placed.k
    FROM (SELECT entry,
                 row_number() OVER (
                     ORDER BY ordered > :cursor,
                         CASE WHEN ordered > :cursor THEN random() ELSE ordered END) AS k
          FROM playback_context) AS placed
    WHERE playback_context.entry = placed.entry
"#
);

// The tracks of the JSON array `tracks` that the library holds, in the order
// a given list puts them, as Up Next's entries numbered from `base` at places
// from `start`.
coven::write!(
    QueueGivenTracks(tracks: &'p str, base: i64, start: i64),
    r#"
    INSERT INTO playback_manual (entry, position, track_id)
    SELECT :base + added.place - 1, :start + added.place - 1, added.track_id
    FROM @bae_records::records::track_list::GivenTracks(tracks: :tracks) AS added
"#
);

// [`QueueGivenTracks`] in the order a picked list puts them.
coven::write!(
    QueuePickedTracks(tracks: &'p str, base: i64, start: i64),
    r#"
    INSERT INTO playback_manual (entry, position, track_id)
    SELECT :base + added.place - 1, :start + added.place - 1, added.track_id
    FROM @bae_records::records::track_list::PickedTracks(tracks: :tracks) AS added
"#
);

#[cfg(any(test, feature = "test-utils"))]
coven::write!(
    SeedContextLane(tracks: &'p str, start: u64),
    r#"
    INSERT INTO playback_context (entry, ordered, shuffled, track_id)
    SELECT :start + seeded.key, :start + seeded.key, :start + seeded.key, seeded.value
    FROM json_each(:tracks) AS seeded
"#
);

#[cfg(any(test, feature = "test-utils"))]
coven::write!(
    SeedLibraryPlayback,
    r#"
    UPDATE playback_queue
    SET source_kind = 'library', shuffled = 0, ordered_cursor = -1, shuffled_cursor = -1
"#
);

// How many of the JSON array `tracks`' ids the library holds, an id named
// twice counted twice.
coven::read!(
    writer HeldTrackCount(tracks: String) { held: i64 },
    r#"
    SELECT count(*) AS held
    FROM @bae_records::records::track_list::GivenTracks(tracks: :tracks) AS listed
"#
);

// Up Next's lowest place, NULL when it is empty.
coven::read!(
    writer UpNextBottom { bottom: Option<i64> },
    r#"SELECT min(manual.position) AS bottom FROM playback_manual AS manual"#
);

// The place of Up Next's entry at `offset`, counted from 0.
coven::read!(
    writer UpNextPositionAt(offset: i64),
    r#"
    SELECT manual.position
    FROM playback_manual AS manual
    ORDER BY manual.position
    LIMIT 1 OFFSET :offset
"#
);

// Up Next's first entry.
coven::read!(
    writer UpNextFront,
    r#"
    SELECT manual.entry, manual.track_id, manual.position
    FROM playback_manual AS manual
    ORDER BY manual.position
    LIMIT 1
"#
);

/// Empty the queue and the resume row unless the resume row is kept: what
/// starting playback with a library does when it will not resume what it
/// played. A library with nothing to resume opens with an empty queue, and
/// one opened with "restore on launch" off forgets what it played. Either
/// way `playback_now` starts stopped, at the resumed volume or at full
/// volume, and `playback_timeline` holds no line. The function writes in
/// bulk, so emptying a library-sized lane takes one step.
pub async fn settle_playback_queue(store: &Store, resume: bool) -> Result<(), DbError> {
    store
        .call_bulk(move |sql| {
            let stored = super::resume::resume_stored_on(sql)?;
            super::now::reset_playback_now_on(sql, resume && stored)?;
            super::timeline::reset_playback_timeline_on(sql)?;
            if resume && stored {
                return Ok(());
            }
            sql.run(&super::resume::DeleteResumeParams {})?;
            sql.run(&ClearUpNextParams {})?;
            clear_context_on(sql)?;
            sql.run(&ResetQueueModesParams {})?;
            Ok(())
        })
        .await
}

impl PlaybackQueueTables {
    /// Replace the whole queue with what `fill` names: Up Next empties, and the
    /// context lane holds the tracks in play order, playing the first. A bulk
    /// write: a play takes out and puts in up to a library's worth of rows.
    pub async fn fill(&self, fill: ContextFill) -> Result<ContextFilled, DbError> {
        self.store.call_bulk(move |sql| fill_on(sql, fill)).await
    }

    /// Add tracks to Up Next where `place` says, in the list's order, leaving
    /// out any the library does not hold, and say how many went in. Putting
    /// any in counts one addition on the queue row, with how many.
    pub async fn add_manual(
        &self,
        tracks: impl Into<bae_records::TrackList>,
        place: ManualPlace,
    ) -> Result<usize, DbError> {
        let tracks = tracks.into();
        self.store.call(move |sql| add_manual_on(sql, &tracks, place)).await
    }

    /// Empty Up Next, as a bulk write.
    pub async fn clear_manual(&self) -> Result<(), DbError> {
        self.store
            .call_bulk(|sql| {
                sql.run(&ClearUpNextParams {})?;
                Ok(())
            })
            .await
    }

    /// Empty the context lane, as a bulk write; `false` when there was none.
    pub async fn clear_context(&self) -> Result<bool, DbError> {
        self.store
            .call_bulk(|sql| {
                if queue_row_on(sql)?.cursor().is_none() {
                    return Ok(false);
                }
                clear_context_on(sql)?;
                Ok(true)
            })
            .await
    }

    /// Turn shuffle on or off: the mode is kept with nothing playing. Turning
    /// it on gives a context lane's tracks after the playing one new places in
    /// the shuffled order, the playing track and the ones before it staying
    /// where they are; turning it off changes only the mode, so the lane reads
    /// in its own order again with the same track playing.
    pub async fn set_shuffle(&self, on: bool) -> Result<(), DbError> {
        self.store.call(move |sql| set_shuffle_on(sql, on)).await
    }

    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                sql.run(&SetRepeatParams { repeat: repeat_column(mode) })?;
                Ok(())
            })
            .await
    }

    /// Take the entry out of whichever lane holds it; `None` when no lane does.
    pub async fn remove(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.store
            .call(move |sql| {
                let (lane, entry) = row_key(&id);
                let removed = match lane {
                    QueueLane::Manual => sql
                        .returning_optional(&RemoveFromUpNextParams { entry })?
                        .map(|row| row.track_id),
                    QueueLane::Context => sql
                        .returning_optional(&RemoveFromContextLaneParams { entry })?
                        .map(|row| row.track_id),
                };
                let Some(track_id) = removed else {
                    return Ok(None);
                };
                if lane == QueueLane::Context {
                    // The lane's last row takes what it played from with it,
                    // as a deleted track does (`tracks_leave_playback_queue`).
                    sql.run(&ClearEmptiedContextSourceParams {})?;
                }
                Ok(Some(QueueEntry { id, track_id }))
            })
            .await
    }

    /// Move the entry to `position` among `lane`'s rows as the screens show
    /// them, or to the lane's end when `position` is past the last row, and
    /// answer whether it moved. Nothing moves when the entry is not in `lane`. The
    /// context lane's playing track stays the playing track wherever it
    /// moves.
    pub async fn reorder(
        &self,
        id: QueueEntryId,
        lane: QueueLane,
        position: u64,
    ) -> Result<bool, DbError> {
        self.store.call(move |sql| reorder_on(sql, id, lane, position)).await
    }

    /// Make the entry the one playing: an Up Next entry leaves the lane with
    /// the ones ahead of it, a context entry becomes the lane's playing track.
    /// `None` when no lane holds it.
    pub async fn skip_to(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.store
            .call(move |sql| {
                let Some(located) = locate_on(sql, id)? else {
                    return Ok(None);
                };
                match located.lane {
                    QueueLane::Manual => {
                        sql.run(&DeleteUpNextThroughParams { position: located.position })?;
                    }
                    QueueLane::Context => set_cursor_on(sql, row_key(&located.entry.id).1)?,
                }
                Ok(Some(located.entry))
            })
            .await
    }

    /// Play on: the first Up Next entry leaves the lane, or the context lane's
    /// next track becomes its playing one. `None` when neither lane has a
    /// track after the playing one.
    pub async fn advance(&self) -> Result<Option<QueueEntry>, DbError> {
        self.store.call(advance_on).await
    }

    /// Play the context lane again from its first track, in a new shuffled
    /// order when it is shuffled. `None` without a context lane.
    pub async fn wrap(&self) -> Result<Option<QueueEntry>, DbError> {
        self.store.call(wrap_on).await
    }

    /// Step the context lane back: to the track before its playing one when
    /// `current` is that playing track, otherwise to the playing track itself,
    /// or the one before it when it has left the lane. `None` when there is
    /// nothing to step back to.
    pub async fn step_back(
        &self,
        current: Option<QueueEntryId>,
    ) -> Result<Option<QueueEntry>, DbError> {
        self.store.call(move |sql| step_back_on(sql, current)).await
    }

    /// The track that plays next: Up Next's first, or else the context lane's
    /// track after the playing one.
    pub async fn front(&self) -> Result<Option<String>, DbError> {
        self.store.read(|sql| super::queue_reads::queue_front_on(&sql)).await
    }

    /// The context lane's next track when it plays straight on from the
    /// playing one: Up Next empty and the lane in its own order. The only
    /// track a side or disc pause can fall before.
    pub async fn next_in_order(&self) -> Result<Option<String>, DbError> {
        self.store.read(|sql| super::queue_reads::next_in_order_on(&sql)).await
    }

    /// Every track of the context lane, played or not, in lane order.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn context_order(&self) -> Result<Vec<String>, DbError> {
        self.store
            .read(|sql| {
                let column = if queue_row_on(&sql)?.shuffled { "shuffled" } else { "ordered" };
                Ok(sql.query(
                    &format!("SELECT track_id FROM playback_context ORDER BY {column}"),
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

// The playing row's place in each of the context lane's orders, while it has
// one; the schema keeps the two NULL together, exactly while the lane is
// empty.
coven::fragment!(
    QueueCursors,
    r#"
    SELECT queue.ordered_cursor AS ordered, queue.shuffled_cursor AS shuffled
    FROM playback_queue AS queue
    WHERE queue.ordered_cursor IS NOT NULL AND queue.shuffled_cursor IS NOT NULL
"#
);

// The `playback_queue` row's columns the lane changes read.
coven::read!(
    writer QueueState { shuffled: bool },
    r#"
    SELECT queue.shuffled, queue.next_entry, cursors.*
    FROM playback_queue AS queue
    LEFT JOIN @crate::tables::queue::QueueCursors AS cursors ON 1
"#
);

impl QueueStateRow {
    /// The playing row's place in the order the lane reads in.
    fn cursor(&self) -> Option<i64> {
        let cursors = self.cursors.as_ref()?;
        Some(if self.shuffled { cursors.shuffled } else { cursors.ordered })
    }
}

pub(super) fn queue_row_on(sql: &impl WriterRead) -> Result<QueueStateRow, DbError> {
    statement_row(sql, &QueueStateParams {})
}

/// Make the context row `entry` the playing one, in both of the lane's orders.
fn set_cursor_on(sql: &CovenWrite<'_>, entry: i64) -> Result<(), DbError> {
    sql.run(&PlayContextEntryParams { entry })?;
    Ok(())
}

/// Empty the context lane and clear what it played from, in the same write.
fn clear_context_on(sql: &coven::BulkSqlContext<'_>) -> Result<(), DbError> {
    sql.run(&ClearContextLaneParams {})?;
    sql.run(&ClearEmptiedContextSourceParams {})?;
    Ok(())
}

/// An entry and where it sits.
pub(super) struct Located {
    pub(super) entry: QueueEntry,
    pub(super) lane: QueueLane,
    position: i64,
}

pub(super) fn locate_on(
    sql: &CovenWrite<'_>,
    id: QueueEntryId,
) -> Result<Option<Located>, DbError> {
    let (lane, entry) = row_key(&id);
    let order = LaneOrder::of(lane, queue_row_on(sql)?.shuffled);
    Ok(order.entry_on(sql, entry)?.map(|(position, track_id)| Located {
        lane,
        position,
        entry: QueueEntry { id, track_id },
    }))
}

/// Give every row of the context lane a new place in the shuffled order, above
/// every place it holds, in random order, and answer the highest place the
/// lane held before. With `keep_through`, a place in the lane's own order, the
/// rows up to it keep their order and come first.
fn place_shuffled_on(sql: &CovenWrite<'_>, keep_through: Option<i64>) -> Result<i64, DbError> {
    let top = LaneOrder::Shuffled.top_on(sql)?.unwrap_or(0);
    match keep_through {
        Some(cursor) => sql.run(&ShuffleLaneAfterParams { top, cursor })?,
        None => sql.run(&ShuffleWholeLaneParams { top })?,
    };
    Ok(top)
}

fn add_manual_on(
    sql: &CovenWrite<'_>,
    tracks: &bae_records::TrackList,
    place: ManualPlace,
) -> Result<usize, DbError> {
    let track_ids = tracks.ids_json();
    let count = statement_row(sql, &HeldTrackCountParams { tracks: track_ids.clone() })?.held;
    if count == 0 {
        return Ok(0);
    }
    let bottom = statement_row(sql, &UpNextBottomParams {})?.bottom;
    let top = LaneOrder::UpNext.top_on(sql)?;
    let end = top.map_or(1, |top| top + 1);
    let start = match place {
        ManualPlace::End => end,
        ManualPlace::Front => bottom.map_or(1, |bottom| bottom - count),
        ManualPlace::At(index) => {
            let at = statement_optional(sql, &UpNextPositionAtParams { offset: index as i64 })?
                .map(|row| row.position);
            match at {
                Some(at) => {
                    shift_on(sql, LaneOrder::UpNext, at, end - 1, count)?;
                    at
                }
                None => end,
            }
        }
    };
    let base = queue_row_on(sql)?.next_entry;
    match tracks {
        bae_records::TrackList::Given(_) => {
            sql.run(&QueueGivenTracksParams { tracks: &track_ids, base, start })?
        }
        bae_records::TrackList::Picked(_) => {
            sql.run(&QueuePickedTracksParams { tracks: &track_ids, base, start })?
        }
    };
    sql.run(&SetNextEntryParams { next_entry: base + count })?;
    Ok(count as usize)
}

fn set_shuffle_on(sql: &CovenWrite<'_>, on: bool) -> Result<(), DbError> {
    let row = queue_row_on(sql)?;
    if row.shuffled == on {
        return Ok(());
    }
    // With nothing playing the mode is all there is to change.
    let Some(cursor) = row.cursor() else {
        sql.run(&SetShuffleParams { shuffled: on })?;
        return Ok(());
    };
    if on {
        // The rows up to the playing one keep their order and come first; the
        // rows after it follow in random order. The lane was reading in its
        // own order, so `cursor` is the playing row's place there.
        let top = place_shuffled_on(sql, Some(cursor))?;
        sql.run(&PlayShuffledAfterParams { top, cursor })?;
    }
    sql.run(&SetShuffleParams { shuffled: on })?;
    Ok(())
}

fn advance_on(sql: &CovenWrite<'_>) -> Result<Option<QueueEntry>, DbError> {
    if let Some(front) = statement_optional(sql, &UpNextFrontParams {})? {
        sql.run(&DeleteUpNextAtParams { position: front.position })?;
        let id = QueueEntryId::Manual(front.entry);
        return Ok(Some(QueueEntry { id, track_id: front.track_id }));
    }
    let row = queue_row_on(sql)?;
    let Some(next) = queue_steps::after_on(sql, row.shuffled, row.cursor())? else {
        return Ok(None);
    };
    set_cursor_on(sql, row_key(&next.id).1)?;
    Ok(Some(next))
}

fn wrap_on(sql: &CovenWrite<'_>) -> Result<Option<QueueEntry>, DbError> {
    let row = queue_row_on(sql)?;
    if row.cursor().is_none() {
        return Ok(None);
    }
    if row.shuffled {
        // A new pass in a new order over the rows the lane holds now. The
        // lane's own order is as it was.
        place_shuffled_on(sql, None)?;
    }
    let first = queue_steps::first_on(sql, row.shuffled)?
        .ok_or_else(|| DbError::unreadable("the first entry of a playing context lane", "none"))?;
    set_cursor_on(sql, row_key(&first.id).1)?;
    Ok(Some(first))
}

fn step_back_on(
    sql: &CovenWrite<'_>,
    current: Option<QueueEntryId>,
) -> Result<Option<QueueEntry>, DbError> {
    let row = queue_row_on(sql)?;
    let Some(cursor) = row.cursor() else {
        return Ok(None);
    };
    let at_cursor = queue_steps::at_on(sql, row.shuffled, cursor)?.map(|entry| entry.id);
    let playing_cursor = matches!(
        (current, at_cursor),
        (Some(QueueEntryId::Context(current)), Some(QueueEntryId::Context(at_cursor)))
            if current == at_cursor
    );
    let Some(target) = queue_steps::before_on(sql, row.shuffled, cursor, !playing_cursor)? else {
        return Ok(None);
    };
    set_cursor_on(sql, row_key(&target.id).1)?;
    Ok(Some(target))
}

pub(crate) fn repeat_column(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Context => "context",
    }
}

/// The repeat mode a `repeat` column stores.
impl FromSql for RepeatMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "off" => Ok(RepeatMode::Off),
            "track" => Ok(RepeatMode::Track),
            "context" => Ok(RepeatMode::Context),
            other => Err(FromSqlError::Other(format!("unknown repeat mode {other:?}").into())),
        }
    }
}

/// The context lane holding `tracks`, the one at index `i` at place
/// `start + i`, and the queue playing the library from before the lane's first
/// track. The lane goes in by one set-based statement, at the sizes the lists'
/// window costs are measured at.
#[cfg(any(test, feature = "test-utils"))]
pub fn fill_context_lane_on(
    sql: &CovenWrite<'_>,
    start: u64,
    tracks: &[String],
) -> Result<(), DbError> {
    let tracks = serde_json::to_string(tracks).expect("serializing track ids cannot fail");
    sql.run(&SeedContextLaneParams { tracks: &tracks, start })?;
    sql.run(&SeedLibraryPlaybackParams {})?;
    Ok(())
}

impl PlaybackQueueTables {
    /// Write the resume row. Never synced.
    pub async fn save_playback_state(&self, state: &PlaybackResumeRow) -> Result<(), DbError> {
        let state = state.clone();
        self.store.call(move |sql| super::resume::save_resume_on(sql, &state)).await
    }

    /// Delete the device-local `playback_state` row (playback stopped).
    ///
    /// Stopping is persisted on every queue change and every stop, so most
    /// calls arrive with the row already gone. That case is read, not written:
    /// a delete over an empty table is a transaction that changes nothing, and
    /// those belong off the sync journal.
    pub async fn clear_playback_state(&self) -> Result<(), DbError> {
        let stored = self.store.read(|sql| super::resume::resume_stored_on(&sql)).await?;
        if !stored {
            return Ok(());
        }
        self.store
            .call(|sql| {
                sql.run(&super::resume::DeleteResumeParams {})?;
                Ok(())
            })
            .await
    }
}

/// Plant damage for a test: every write of the queue row of the library
/// stored under `directory` fails, as a full disk fails it. The queue row
/// counts every change to either lane and holds the repeat mode and the
/// context lane's cursor, so each queue command that writes fails.
#[cfg(any(test, feature = "test-utils"))]
pub fn fail_queue_writes_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_queue",
        "CREATE TRIGGER fail_playback_queue_write BEFORE UPDATE ON playback_queue \
         BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;",
    );
}

/// Plant damage for a test: the table of Up Next is gone from the library
/// stored under `directory`, so the queue can't say which track follows.
#[cfg(any(test, feature = "test-utils"))]
pub fn lose_up_next_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_manual",
        "DROP TABLE playback_manual;",
    );
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
