//! The queue screens' reads of the playback queue tables: the queue value,
//! with Up Next whole and what the context lane plays from, and the windows
//! of the context lane's upcoming tracks with their length.

use super::playback::{track_display_columns, track_queue_meta_from_row, TRACK_DISPLAY_JOINS};
use super::playback_queue::repeat_from_column;
use super::*;
use crate::playback::{ContextSource, RepeatMode};

impl Database {
    /// Follow the queue as its screens show it: Up Next whole, what the
    /// context lane plays from, and whether the transport can step either way.
    pub(crate) fn subscribe_queue_value(
        &self,
    ) -> coven::LiveQuery<crate::queue::ResolvedQueueSnapshot> {
        self.inner
            .handle
            .subscribe(|sql| queue_value_on(&sql).map_err(CovenError::from))
    }

    /// The queue as its screens show it now.
    pub async fn queue_value(&self) -> Result<crate::queue::ResolvedQueueSnapshot, DbError> {
        self.read(|sql| queue_value_on(&sql)).await
    }

    /// Every one of the context lane's tracks after the playing one, as the
    /// queue rows show them.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn queue_upcoming(&self) -> Result<Vec<QueueItem>, DbError> {
        let all = crate::library::LibraryPageWindows::from([crate::library::LibraryPageWindow {
            offset: 0,
            limit: u64::MAX,
        }]);
        let read = self.read(move |sql| queue_upcoming_on(&sql, &all)).await?;
        Ok(read
            .windows
            .into_iter()
            .flat_map(|window| window.items)
            .collect())
    }

    /// Follow the windows `initial` names of the context lane's tracks after
    /// the playing one. The windows change by pointing the same query at new
    /// ones through its request handle.
    pub(crate) fn subscribe_queue_upcoming(
        &self,
        initial: crate::library::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<crate::library::LibraryPageWindows, QueueUpcomingRead> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |windows, sql| {
                queue_upcoming_on(&sql, windows).map_err(CovenError::from)
            })
            .process(Ok)
    }
}

/// The queue entries `filter` picks, each with its track as the queue rows
/// show it, in lane order, cut to `page` (a `LIMIT …` clause) when given.
/// The page is cut from the entries before the join, so the rows an offset
/// passes over are never joined to their tracks.
fn queue_items_on(
    sql: &SqlReadContext<'_>,
    filter: &str,
    page: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<QueueItem>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT e.entry, {columns} \
             FROM (SELECT entry, track_id, position FROM playback_queue_entries e \
                   WHERE {filter} ORDER BY position {page}) e \
             JOIN tracks t ON t.id = e.track_id \
             {joins} \
             ORDER BY e.position",
            columns = track_display_columns(),
            joins = TRACK_DISPLAY_JOINS,
        ),
        params,
        |row| {
            let meta = track_queue_meta_from_row(row)?;
            Ok(QueueItem {
                entry_id: row.get::<_, i64>("entry")?.to_string(),
                track_id: row.get("track_id")?,
                display: meta.display,
                duration_ms: meta.duration_ms,
            })
        },
    )?)
}

/// The `playback_queue` row as the queue screens read it.
struct QueueHead {
    repeat: RepeatMode,
    context: Option<(ContextSource, bool, i64)>,
}

fn queue_head_on(sql: &SqlReadContext<'_>) -> Result<QueueHead, DbError> {
    let (repeat, source, shuffled, cursor): (String, Option<String>, Option<bool>, Option<i64>) =
        sql.query_row(
            "SELECT repeat, source, shuffled, cursor FROM playback_queue",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let context = match (source, shuffled, cursor) {
        (Some(source), Some(shuffled), Some(cursor)) => Some((
            ContextSource::from_column(&source).map_err(DbError::Message)?,
            shuffled,
            cursor,
        )),
        // The table's CHECK keeps the three NULL together.
        _ => None,
    };
    Ok(QueueHead {
        repeat: repeat_from_column(&repeat)?,
        context,
    })
}

fn queue_value_on(
    sql: &SqlReadContext<'_>,
) -> Result<crate::queue::ResolvedQueueSnapshot, DbError> {
    let head = queue_head_on(sql)?;
    let manual = queue_items_on(sql, "e.lane = 'manual'", "", [])?;
    let (context, has_upcoming, has_previous) = match head.context {
        None => (None, false, false),
        Some((source, shuffled, cursor)) => {
            let (has_upcoming, has_previous): (bool, bool) = sql.query_row(
                "SELECT EXISTS (SELECT 1 FROM playback_queue_entries \
                                WHERE lane = 'context' AND position > ?1), \
                        EXISTS (SELECT 1 FROM playback_queue_entries \
                                WHERE lane = 'context' AND position < ?1)",
                [cursor],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let source_title = match &source {
                ContextSource::Release(release_id) => sql
                    .query_row(
                        "SELECT a.title FROM releases r JOIN albums a ON a.id = r.album_id \
                         WHERE r.id = ?",
                        [release_id],
                        |row| row.get(0),
                    )
                    .optional()?,
                ContextSource::Releases(_) | ContextSource::Library => None,
            };
            (
                Some(crate::queue::ResolvedContext {
                    source,
                    source_title,
                    shuffled,
                }),
                has_upcoming,
                has_previous,
            )
        }
    };
    let has_next = !manual.is_empty() || has_upcoming || head.repeat != RepeatMode::Off;
    Ok(crate::queue::ResolvedQueueSnapshot {
        manual,
        context,
        has_next,
        has_previous,
    })
}

fn queue_upcoming_on(
    sql: &SqlReadContext<'_>,
    windows: &crate::library::LibraryPageWindows,
) -> Result<QueueUpcomingRead, DbError> {
    let head = queue_head_on(sql)?;
    let windows = windows
        .iter()
        .map(|window| {
            let items = match head.context {
                None => Vec::new(),
                Some((_, _, cursor)) => queue_items_on(
                    sql,
                    "e.lane = 'context' AND e.position > ?1",
                    "LIMIT ?2 OFFSET ?3",
                    params![
                        cursor,
                        i64::try_from(window.limit).unwrap_or(i64::MAX),
                        i64::try_from(window.offset).unwrap_or(i64::MAX)
                    ],
                )?,
            };
            Ok(crate::library::QueueUpcomingWindow {
                window: window.clone(),
                items,
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(QueueUpcomingRead {
        upcoming_total: match head.context {
            None => 0,
            Some((_, _, cursor)) => upcoming_total_on(sql, cursor)?,
        },
        windows,
    })
}

/// How many of the context lane's tracks come after its playing one.
fn upcoming_total_on(sql: &SqlReadContext<'_>, cursor: i64) -> Result<u64, DbError> {
    let total: i64 = sql.query_row(
        "SELECT count(*) FROM playback_queue_entries WHERE lane = 'context' AND position > ?",
        [cursor],
        |row| row.get(0),
    )?;
    Ok(total as u64)
}

/// One read of the context lane's upcoming windows: the tail's length then,
/// and each window's entries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueueUpcomingRead {
    pub upcoming_total: u64,
    pub windows: Vec<crate::library::QueueUpcomingWindow>,
}
