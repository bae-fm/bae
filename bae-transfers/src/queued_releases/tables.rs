//! The transfer queues' tables: `download_queue` and `output_queue`, each
//! entry a release in the order queued with its phase, and whether each queue
//! is paused (`release_queues`), as functions over coven's SQL contexts. The
//! queue owners ([`crate::QueuedReleases`]) are the only writers; the panes,
//! the detail views and the sync activity read the same tables through live
//! queries.
//!
//! Its tables, and the versions of their order, are `tables.sql`.

/// The SQL creating this part's tables and triggers.
pub(crate) const SQL: &str = include_str!("tables.sql");

use crate::output_snapshot::{OutputKind, OutputRequest};
#[cfg(any(desktop, test, feature = "test-utils"))]
use crate::release_queue::ReleaseQueueSnapshot;
use crate::release_queue::{
    QueuedRelease, ReleaseQueueCounts, ReleaseQueueEntry, ReleaseQueueOp, ReleaseQueueProgress,
    ReleaseQueueRow, ReleaseQueueState, ReleaseQueueWindows,
};
use bae_db::client::list_windows::{
    list_versions_on, read_windows, Keyed, KnownPlaces, ListOrder, Seek, SortTerm, WindowsRead,
};
use bae_db::client::read::column_conversion_error;
use bae_db::{DbError, LibraryBrowseWindow, LibraryPageWindows};
use coven::{CovenWrite, SqlReadContext};
#[cfg(desktop)]
use rusqlite::types::Value;
use rusqlite::{params, OptionalExtension, Row};

/// What a queue's entries carry beyond their release, and the table that
/// holds them.
pub trait QueuedPayload: Clone + PartialEq + Send + Sync + 'static {
    /// The queue's row in `release_queues`.
    const QUEUE: &'static str;
    const TABLE: &'static str;
    /// The payload's columns, in the order [`Self::values`] gives them.
    const COLUMNS: &'static [&'static str];
    /// Only the queues desktop fills by entry read it.
    #[cfg(desktop)]
    fn values(&self) -> Result<Vec<Value>, DbError>;
    /// The payload from a row read with [`Self::COLUMNS`] from column `at`.
    fn from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<Self>;
}

/// A download carries nothing beyond its release.
impl QueuedPayload for () {
    const QUEUE: &'static str = "download";
    const TABLE: &'static str = "download_queue";
    const COLUMNS: &'static [&'static str] = &[];

    #[cfg(desktop)]
    fn values(&self) -> Result<Vec<Value>, DbError> {
        Ok(Vec::new())
    }

    fn from_row(_row: &Row<'_>, _at: usize) -> rusqlite::Result<Self> {
        Ok(())
    }
}

/// An output carries its target folder and what it writes there, a save its
/// preset whole.
impl QueuedPayload for OutputRequest {
    const QUEUE: &'static str = "output";
    const TABLE: &'static str = "output_queue";
    const COLUMNS: &'static [&'static str] = &["target_dir", "kind", "preset"];

    #[cfg(desktop)]
    fn values(&self) -> Result<Vec<Value>, DbError> {
        let (kind, preset) = match &self.kind {
            OutputKind::Export => ("export", None),
            OutputKind::Save { preset } => (
                "save",
                Some(serde_json::to_string(preset).map_err(|error| {
                    DbError::Message(format!("save preset {}: {error}", preset.id))
                })?),
            ),
        };
        Ok(vec![
            Value::Blob(coven::path_bytes(&self.target_dir)),
            Value::Text(kind.to_string()),
            preset.map_or(Value::Null, Value::Text),
        ])
    }

    fn from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<Self> {
        let target_dir = bae_db::client::path_column::get(row, at)?;
        let kind: String = row.get(at + 1)?;
        let preset: Option<String> = row.get(at + 2)?;
        let kind = match (kind.as_str(), preset) {
            ("export", None) => OutputKind::Export,
            ("save", Some(preset)) => OutputKind::Save {
                preset: serde_json::from_str(&preset)
                    .map_err(|error| column_conversion_error(row, at + 2, error))?,
            },
            (kind, preset) => {
                return Err(column_conversion_error(
                    row,
                    at + 1,
                    format!("output kind {kind:?} with preset {preset:?}"),
                ))
            }
        };
        Ok(OutputRequest { target_dir, kind })
    }
}

/// `release_id, created_at` and the payload's columns, each prefixed by
/// `prefix`.
fn entry_columns<P: QueuedPayload>(prefix: &str) -> String {
    ["release_id", "created_at"]
        .iter()
        .chain(P::COLUMNS)
        .map(|column| format!("{prefix}{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// An entry read with [`entry_columns`] from column 0.
fn entry_from_row<P: QueuedPayload>(row: &Row<'_>) -> rusqlite::Result<ReleaseQueueEntry<P>> {
    Ok(ReleaseQueueEntry {
        release_id: row.get(0)?,
        created_at: row.get(1)?,
        payload: P::from_row(row, 2)?,
    })
}

/// An entry's phase from its `state` and `error` columns at `at`.
fn phase_from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<ReleaseQueueState<()>> {
    let state: String = row.get(at)?;
    let error: Option<String> = row.get(at + 1)?;
    match (state.as_str(), error) {
        ("queued", None) => Ok(ReleaseQueueState::Queued),
        ("active", None) => Ok(ReleaseQueueState::Active { progress: () }),
        ("failed", Some(error)) => Ok(ReleaseQueueState::Failed { error }),
        (state, error) => Err(column_conversion_error(
            row,
            at,
            format!("queue state {state:?} with error {error:?}"),
        )),
    }
}

/// A queue's entries beside their releases as the library holds them, the
/// ones `tail` picks: a `WHERE`, `ORDER BY` or `LIMIT` over the queue `q`;
/// and each entry's key in `keyed`'s order when one is given.
pub fn queue_rows_sql<P: QueuedPayload>(keyed: Option<&ListOrder>, tail: &str) -> String {
    format!(
        "SELECT {columns}, q.state, q.error, r.id IS NOT NULL, a.title, \
            COALESCE((SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = q.release_id), 0), \
            COALESCE((SELECT SUM(coven.size) FROM release_files rf \
                JOIN coven_files coven ON coven.id = rf.file \
                WHERE rf.release_id = q.release_id), 0){keys} \
         FROM {table} q \
         LEFT JOIN releases r ON r.id = q.release_id \
         LEFT JOIN albums a ON a.id = r.album_id \
         {tail}",
        columns = entry_columns::<P>("q."),
        keys = keyed.map_or_else(String::new, |order| format!(", {}", order.key_columns())),
        table = P::TABLE,
    )
}

/// The rows [`queue_rows_sql`] reads for `tail`, bound to `params`.
#[cfg(any(desktop, test, feature = "test-utils"))]
fn queue_rows_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
    tail: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<ReleaseQueueRow<P, ()>>, DbError> {
    Ok(sql.query(&queue_rows_sql::<P>(None, tail), params, queue_row::<P>)?)
}

/// One entry as [`queue_rows_sql`] reads it.
fn queue_row<P: QueuedPayload>(row: &Row<'_>) -> rusqlite::Result<ReleaseQueueRow<P, ()>> {
    let at = 2 + P::COLUMNS.len();
    let entry = entry_from_row::<P>(row)?;
    let held: bool = row.get(at + 2)?;
    let release = if held {
        Some(QueuedRelease {
            title: row.get(at + 3)?,
            file_count: row.get(at + 4)?,
            total_size: row.get(at + 5)?,
        })
    } else {
        None
    };
    Ok(ReleaseQueueRow {
        op: ReleaseQueueOp {
            release_id: entry.release_id,
            created_at: entry.created_at,
            payload: entry.payload,
            state: phase_from_row(row, at)?,
        },
        release,
    })
}

/// A queue's order: the order queued.
pub fn queue_order() -> ListOrder {
    ListOrder::new(vec![SortTerm::ascending("q.position")])
}

/// The queue's entries `seek` reads in `order`, at most `limit` from
/// `offset`, each with its key.
pub fn queue_window_rows_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<ReleaseQueueRow<P, ()>>>, DbError> {
    let statement = queue_rows_sql::<P>(
        Some(order),
        &format!(
            "WHERE {} ORDER BY {} LIMIT :limit OFFSET :offset",
            order.condition(seek),
            order.ordering(seek)
        ),
    );
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| Ok((queue_row::<P>(row)?, order.key_of(row)?)))
    })?)
}

/// How many entries the queue holds, and the windows `windows` names of
/// them in the order queued, each beside its release, each read from the
/// entry beside it when `places` knows one (`list_windows`).
pub(crate) fn release_queue_windows_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
    windows: &LibraryPageWindows,
    places: &KnownPlaces<Vec<i64>>,
) -> Result<ReleaseQueueWindows<P>, DbError> {
    let versions = list_versions_on(sql, "release_queue_versions", &[P::TABLE])?;
    let total_count: u64 =
        sql.query_row(&format!("SELECT COUNT(*) FROM {}", P::TABLE), [], |row| row.get(0))?;
    let order = queue_order();
    let WindowsRead { windows, found } =
        read_windows(&order, windows, places.under(&versions), |seek, limit, offset| {
            queue_window_rows_on::<P>(sql, &order, seek, limit, offset)
        })?;
    places.keep(versions, found);
    Ok(ReleaseQueueWindows {
        windows: windows
            .into_iter()
            .map(|(window, rows)| LibraryBrowseWindow { window, rows })
            .collect(),
        total_count,
    })
}

fn paused_on(sql: &SqlReadContext<'_>, queue: &str) -> rusqlite::Result<bool> {
    sql.query_row("SELECT paused FROM release_queues WHERE queue = ?", [queue], |row| row.get(0))
}

#[cfg(any(desktop, test, feature = "test-utils"))]
pub(crate) fn release_queue_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
) -> Result<ReleaseQueueSnapshot<P, ()>, DbError> {
    let rows = queue_rows_on::<P>(sql, "ORDER BY q.position", [])?;
    Ok(ReleaseQueueSnapshot {
        rows,
        total: release_queue_counts_on::<P>(sql)?.total,
        paused: paused_on(sql, P::QUEUE)?,
    })
}

pub fn release_queue_counts_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
) -> Result<ReleaseQueueCounts, DbError> {
    let (queued, active, failed): (u32, u32, u32) = sql.query_row(
        &format!(
            "SELECT COALESCE(SUM(state = 'queued'), 0), COALESCE(SUM(state = 'active'), 0), \
                    COALESCE(SUM(state = 'failed'), 0) \
             FROM {}",
            P::TABLE
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(ReleaseQueueCounts {
        total: ReleaseQueueProgress { queued, active, failed },
        paused: paused_on(sql, P::QUEUE)?,
    })
}

/// `release_id`'s entries in the download and export queues.
#[cfg(desktop)]
pub fn release_queue_entries_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<crate::release_queue::ReleaseQueueEntries, DbError> {
    Ok(crate::release_queue::ReleaseQueueEntries {
        download: queue_rows_on::<()>(sql, "WHERE q.release_id = ?1", [release_id])?.pop(),
        output: queue_rows_on::<OutputRequest>(sql, "WHERE q.release_id = ?1", [release_id])?.pop(),
    })
}

/// The download queue's phase for `release_id`, `None` when it is not
/// queued: what a release's detail shows of its download.
pub(crate) fn download_phase_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseQueueState<()>>, DbError> {
    Ok(sql
        .query_row(
            "SELECT state, error FROM download_queue WHERE release_id = ?",
            [release_id],
            |row| phase_from_row(row, 0),
        )
        .optional()?)
}

/// Queue `entries` in order, passing over a release already in the queue;
/// `true` when any went in.
#[cfg(desktop)]
pub(crate) fn enqueue_releases_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    entries: &[ReleaseQueueEntry<P>],
) -> Result<bool, DbError> {
    let placeholders = vec!["?"; 2 + P::COLUMNS.len()].join(", ");
    let insert = format!(
        "INSERT INTO {table} ({columns}, state) VALUES ({placeholders}, 'queued') \
         ON CONFLICT (release_id) DO NOTHING",
        table = P::TABLE,
        columns = entry_columns::<P>(""),
    );
    let mut added = false;
    for entry in entries {
        let mut values =
            vec![Value::Text(entry.release_id.clone()), Value::Integer(entry.created_at)];
        values.extend(entry.payload.values()?);
        added |= sql.execute(&insert, rusqlite::params_from_iter(values))? > 0;
    }
    Ok(added)
}

/// Queue to pin, in the order given, each of `release_ids` that is
/// uploaded and not wholly pinned on this device, as its storage row says,
/// passing over one waiting or downloading, one the library no longer
/// holds, and one local: decided in the write that queues them. One whose
/// download failed is queued again in its place, as a retry queues it.
/// `true` when any went in or was queued again.
pub(crate) fn enqueue_pinnable_releases_on(
    sql: &CovenWrite<'_>,
    release_ids: &[String],
    created_at: i64,
) -> Result<bool, DbError> {
    let release_ids = serde_json::Value::from(release_ids.to_vec()).to_string();
    Ok(sql.execute(
        &format!(
            "INSERT INTO download_queue (release_id, created_at, state) \
             SELECT requested.value, :created_at, 'queued' \
             FROM json_each(:release_ids) AS requested \
             JOIN release_storage AS storage ON storage.release_id = requested.value \
             WHERE storage.stored = 1 AND NOT {pinned} \
             ORDER BY requested.key \
             ON CONFLICT (release_id) DO UPDATE SET state = 'queued', error = NULL \
             WHERE download_queue.state = 'failed'",
            pinned = bae_db::client::query::RELEASE_STORAGE_PINNED,
        ),
        rusqlite::named_params! {
            ":created_at": created_at,
            ":release_ids": release_ids,
        },
    )? > 0)
}

/// Whether the queue holds `release_id`, in any phase.
pub(crate) fn release_queued_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(sql.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM {} WHERE release_id = ?)", P::TABLE),
        [release_id],
        |row| row.get(0),
    )?)
}

/// The first queued entry, unless the queue is paused.
pub(crate) fn next_queued_release_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
) -> Result<Option<ReleaseQueueEntry<P>>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "SELECT {columns} FROM {table} \
                 WHERE state = 'queued' \
                   AND NOT (SELECT paused FROM release_queues WHERE queue = ?) \
                 ORDER BY position LIMIT 1",
                columns = entry_columns::<P>(""),
                table = P::TABLE,
            ),
            [P::QUEUE],
            entry_from_row::<P>,
        )
        .optional()?)
}

/// Start `release_id`'s entry; `false` when it is not queued, having
/// been cancelled or never queued.
pub(crate) fn activate_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(sql.execute(
        &format!(
            "UPDATE {} SET state = 'active' WHERE release_id = ? AND state = 'queued'",
            P::TABLE
        ),
        [release_id],
    )? > 0)
}

/// The active entry for `release_id` finished: it leaves the queue.
/// `false` when it had already left, cancelled while it ran.
pub(crate) fn finish_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(sql.execute(
        &format!("DELETE FROM {} WHERE release_id = ? AND state = 'active'", P::TABLE),
        [release_id],
    )? > 0)
}

/// `release_id`'s entry failed, starting or running: it stays listed
/// with `error` until retried. `false` when it had already left.
pub(crate) fn fail_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
    error: &str,
) -> Result<bool, DbError> {
    Ok(sql.execute(
        &format!(
            "UPDATE {} SET state = 'failed', error = ?2 \
             WHERE release_id = ?1 AND state IN ('queued', 'active')",
            P::TABLE
        ),
        params![release_id, error],
    )? > 0)
}

/// Queue every failed entry again, in its place; `true` when any had
/// failed.
pub(crate) fn retry_failed_releases_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
) -> Result<bool, DbError> {
    Ok(sql.execute(
        &format!("UPDATE {} SET state = 'queued', error = NULL WHERE state = 'failed'", P::TABLE),
        [],
    )? > 0)
}

/// Pause or resume the queue, saying whether it was paused.
pub(crate) fn set_release_queue_paused_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    paused: bool,
) -> Result<bool, DbError> {
    let was: bool =
        sql.query_row("SELECT paused FROM release_queues WHERE queue = ?", [P::QUEUE], |row| {
            row.get(0)
        })?;
    sql.execute(
        "UPDATE release_queues SET paused = ?2 WHERE queue = ?1",
        params![P::QUEUE, paused],
    )?;
    Ok(was)
}

/// Take `release_id`'s entry out, in any phase; `false` when the queue
/// does not hold it.
pub(crate) fn cancel_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(sql.execute(&format!("DELETE FROM {} WHERE release_id = ?", P::TABLE), [release_id])? > 0)
}

/// Empty both queues and let them run, and end every storage transition:
/// what the library opens with, since no transfer outlives the process
/// that ran it.
pub fn clear_transfers_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.execute("DELETE FROM download_queue", [])?;
    sql.execute("DELETE FROM output_queue", [])?;
    sql.execute("DELETE FROM release_storage_action", [])?;
    sql.execute("UPDATE release_queues SET paused = 0", [])?;
    Ok(())
}

/// The worker's moves of a queue's entries, for a test that moves entries in
/// the table without a worker running them.
#[cfg(any(test, feature = "test-utils"))]
pub mod worker_moves {
    use super::*;

    /// Start `release_id`'s entry, as the worker does once its task runs.
    pub fn activate_on<P: QueuedPayload>(
        sql: &CovenWrite<'_>,
        release_id: &str,
    ) -> Result<bool, DbError> {
        activate_queued_release_on::<P>(sql, release_id)
    }

    /// Finish `release_id`'s active entry, as the worker does on success.
    pub fn finish_on<P: QueuedPayload>(
        sql: &CovenWrite<'_>,
        release_id: &str,
    ) -> Result<bool, DbError> {
        finish_queued_release_on::<P>(sql, release_id)
    }

    /// Fail `release_id`'s entry with `error`, as the worker does on failure.
    pub fn fail_on<P: QueuedPayload>(
        sql: &CovenWrite<'_>,
        release_id: &str,
        error: &str,
    ) -> Result<bool, DbError> {
        fail_queued_release_on::<P>(sql, release_id, error)
    }
}

#[cfg(test)]
#[path = "tables_tests.rs"]
mod tests;
