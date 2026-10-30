//! The transfer queues' tables: `download_queue` and `output_queue`, each
//! entry a release in the order queued with its phase, and whether each queue
//! is paused (`release_queues`). The queue owners
//! ([`crate::library::QueuedReleases`]) are the only writers; the panes, the
//! detail views and the sync activity read the same tables through live
//! queries.

use super::*;
use crate::library::output_snapshot::{OutputKind, OutputRequest};
#[cfg(any(desktop, test, feature = "test-utils"))]
use crate::library::release_queue::ReleaseQueueSnapshot;
use crate::library::release_queue::{
    QueuedRelease, ReleaseQueueCounts, ReleaseQueueEntry, ReleaseQueueOp, ReleaseQueueProgress,
    ReleaseQueueRow, ReleaseQueueState, ReleaseQueueWindows,
};
use crate::library::{LibraryBrowseWindow, LibraryPageWindows};
use rusqlite::types::{Type, Value};

/// What a queue's entries carry beyond their release, and the table that
/// holds them.
pub(crate) trait QueuedPayload: Clone + PartialEq + Send + Sync + 'static {
    /// The queue's row in `release_queues`.
    const QUEUE: &'static str;
    const TABLE: &'static str;
    /// The payload's columns, in the order [`Self::values`] gives them.
    const COLUMNS: &'static [&'static str];
    fn values(&self) -> Result<Vec<Value>, DbError>;
    /// The payload from a row read with [`Self::COLUMNS`] from column `at`.
    fn from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<Self>;
}

/// A download carries nothing beyond its release.
impl QueuedPayload for () {
    const QUEUE: &'static str = "download";
    const TABLE: &'static str = "download_queue";
    const COLUMNS: &'static [&'static str] = &[];

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

    fn values(&self) -> Result<Vec<Value>, DbError> {
        let target_dir = self.target_dir.to_str().ok_or_else(|| {
            DbError::Message(format!(
                "export target directory is not valid UTF-8: {}",
                self.target_dir.display()
            ))
        })?;
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
            Value::Text(target_dir.to_string()),
            Value::Text(kind.to_string()),
            preset.map_or(Value::Null, Value::Text),
        ])
    }

    fn from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<Self> {
        let target_dir: String = row.get(at)?;
        let kind: String = row.get(at + 1)?;
        let preset: Option<String> = row.get(at + 2)?;
        let kind = match (kind.as_str(), preset) {
            ("export", None) => OutputKind::Export,
            ("save", Some(preset)) => OutputKind::Save {
                preset: serde_json::from_str(&preset).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(at + 2, Type::Text, Box::new(error))
                })?,
            },
            (kind, preset) => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    at + 1,
                    Type::Text,
                    format!("output kind {kind:?} with preset {preset:?}").into(),
                ))
            }
        };
        Ok(OutputRequest {
            target_dir: target_dir.into(),
            kind,
        })
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
        (state, error) => Err(rusqlite::Error::FromSqlConversionFailure(
            at,
            Type::Text,
            format!("queue state {state:?} with error {error:?}").into(),
        )),
    }
}

/// A queue's entries beside their releases as the library holds them, the
/// ones `tail` picks: a `WHERE`, `ORDER BY` or `LIMIT` over the queue `q`.
pub(super) fn queue_rows_sql<P: QueuedPayload>(tail: &str) -> String {
    format!(
        "SELECT {columns}, q.state, q.error, r.id IS NOT NULL, a.title, \
            COALESCE((SELECT COUNT(*) FROM release_files rf WHERE rf.release_id = q.release_id), 0), \
            COALESCE((SELECT SUM(rf.file_size) FROM release_files rf WHERE rf.release_id = q.release_id), 0) \
         FROM {table} q \
         LEFT JOIN releases r ON r.id = q.release_id \
         LEFT JOIN albums a ON a.id = r.album_id \
         {tail}",
        columns = entry_columns::<P>("q."),
        table = P::TABLE,
    )
}

/// The rows [`queue_rows_sql`] reads for `tail`, bound to `params`.
fn queue_rows_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
    tail: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<ReleaseQueueRow<P, ()>>, DbError> {
    let at = 2 + P::COLUMNS.len();
    Ok(sql.query(&queue_rows_sql::<P>(tail), params, |row| {
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
    })?)
}

/// A window's offset or length as SQLite takes it.
fn sql_bound(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// How many entries the queue holds, and the windows `windows` names of
/// them in the order queued, each beside its release.
fn release_queue_windows_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
    windows: &LibraryPageWindows,
) -> Result<ReleaseQueueWindows<P>, DbError> {
    let total_count: u64 =
        sql.query_row(&format!("SELECT COUNT(*) FROM {}", P::TABLE), [], |row| {
            row.get(0)
        })?;
    let windows = windows
        .iter()
        .map(|window| {
            Ok(LibraryBrowseWindow {
                window: window.clone(),
                rows: queue_rows_on::<P>(
                    sql,
                    "ORDER BY q.position LIMIT ?1 OFFSET ?2",
                    params![sql_bound(window.limit), sql_bound(window.offset)],
                )?,
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(ReleaseQueueWindows {
        windows,
        total_count,
    })
}

fn paused_on(sql: &SqlReadContext<'_>, queue: &str) -> rusqlite::Result<bool> {
    sql.query_row(
        "SELECT paused FROM release_queues WHERE queue = ?",
        [queue],
        |row| row.get(0),
    )
}

#[cfg(any(desktop, test, feature = "test-utils"))]
fn release_queue_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
) -> Result<ReleaseQueueSnapshot<P, ()>, DbError> {
    let rows = queue_rows_on::<P>(sql, "ORDER BY q.position", [])?;
    Ok(ReleaseQueueSnapshot {
        rows,
        total: release_queue_counts_on::<P>(sql)?.total,
        paused: paused_on(sql, P::QUEUE)?,
    })
}

fn release_queue_counts_on<P: QueuedPayload>(
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
        total: ReleaseQueueProgress {
            queued,
            active,
            failed,
        },
        paused: paused_on(sql, P::QUEUE)?,
    })
}

/// `release_id`'s entries in the download and export queues.
#[cfg(desktop)]
pub(super) fn release_queue_entries_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<crate::library::ReleaseQueueEntries, DbError> {
    Ok(crate::library::ReleaseQueueEntries {
        download: queue_rows_on::<()>(sql, "WHERE q.release_id = ?1", [release_id])?.pop(),
        output: queue_rows_on::<OutputRequest>(sql, "WHERE q.release_id = ?1", [release_id])?.pop(),
    })
}

/// The download queue's phase for `release_id`, `None` when it is not
/// queued: what a release's detail shows of its download.
pub(super) fn download_phase_on(
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

impl Database {
    /// Queue `entries` in order, passing over a release already in the queue;
    /// `true` when any went in.
    pub(crate) async fn enqueue_releases<P: QueuedPayload>(
        &self,
        entries: Vec<ReleaseQueueEntry<P>>,
    ) -> Result<bool, DbError> {
        self.call(move |sql| {
            let placeholders = vec!["?"; 2 + P::COLUMNS.len()].join(", ");
            let insert = format!(
                "INSERT INTO {table} ({columns}, state) VALUES ({placeholders}, 'queued') \
                 ON CONFLICT (release_id) DO NOTHING",
                table = P::TABLE,
                columns = entry_columns::<P>(""),
            );
            let mut added = false;
            for entry in &entries {
                let mut values = vec![
                    Value::Text(entry.release_id.clone()),
                    Value::Integer(entry.created_at),
                ];
                values.extend(entry.payload.values()?);
                added |= sql.execute(&insert, rusqlite::params_from_iter(values))? > 0;
            }
            Ok(added)
        })
        .await
    }

    /// Queue to pin, in the order given, each of `release_ids` that is
    /// uploaded and not wholly pinned on this device, as its listing says,
    /// passing over one waiting or downloading, one the library no longer
    /// holds, and one local: decided in the write that queues them. One whose
    /// download failed is queued again in its place, as a retry queues it.
    /// `true` when any went in or was queued again.
    pub(crate) async fn enqueue_pinnable_releases(
        &self,
        release_ids: Vec<String>,
        created_at: i64,
    ) -> Result<bool, DbError> {
        let release_ids = serde_json::Value::from(release_ids).to_string();
        self.call(move |sql| {
            Ok(sql.execute(
                &format!(
                    "INSERT INTO download_queue (release_id, created_at, state) \
                     SELECT requested.value, :created_at, 'queued' \
                     FROM json_each(:release_ids) AS requested \
                     JOIN release_listing AS listing ON listing.release_id = requested.value \
                     WHERE listing.stored = 1 AND NOT {pinned} \
                     ORDER BY requested.key \
                     ON CONFLICT (release_id) DO UPDATE SET state = 'queued', error = NULL \
                     WHERE download_queue.state = 'failed'",
                    pinned = super::query::RELEASE_LISTING_PINNED,
                ),
                rusqlite::named_params! {
                    ":created_at": created_at,
                    ":release_ids": release_ids,
                },
            )? > 0)
        })
        .await
    }

    /// Whether the queue holds `release_id`, in any phase.
    pub(crate) async fn release_queued<P: QueuedPayload>(
        &self,
        release_id: &str,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            Ok(sql.query_row(
                &format!(
                    "SELECT EXISTS (SELECT 1 FROM {} WHERE release_id = ?)",
                    P::TABLE
                ),
                [release_id],
                |row| row.get(0),
            )?)
        })
        .await
    }

    /// The first queued entry, unless the queue is paused.
    pub(crate) async fn next_queued_release<P: QueuedPayload>(
        &self,
    ) -> Result<Option<ReleaseQueueEntry<P>>, DbError> {
        self.read(|sql| {
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
        })
        .await
    }

    /// Start `release_id`'s entry; `false` when it is not queued, having
    /// been cancelled or never queued.
    pub(crate) async fn activate_queued_release<P: QueuedPayload>(
        &self,
        release_id: &str,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.call(move |sql| {
            Ok(sql.execute(
                &format!(
                    "UPDATE {} SET state = 'active' WHERE release_id = ? AND state = 'queued'",
                    P::TABLE
                ),
                [release_id],
            )? > 0)
        })
        .await
    }

    /// The active entry for `release_id` finished: it leaves the queue.
    /// `false` when it had already left, cancelled while it ran.
    pub(crate) async fn finish_queued_release<P: QueuedPayload>(
        &self,
        release_id: &str,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.call(move |sql| {
            Ok(sql.execute(
                &format!(
                    "DELETE FROM {} WHERE release_id = ? AND state = 'active'",
                    P::TABLE
                ),
                [release_id],
            )? > 0)
        })
        .await
    }

    /// `release_id`'s entry failed, starting or running: it stays listed
    /// with `error` until retried. `false` when it had already left.
    pub(crate) async fn fail_queued_release<P: QueuedPayload>(
        &self,
        release_id: &str,
        error: String,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.call(move |sql| {
            Ok(sql.execute(
                &format!(
                    "UPDATE {} SET state = 'failed', error = ?2 \
                     WHERE release_id = ?1 AND state IN ('queued', 'active')",
                    P::TABLE
                ),
                params![release_id, error],
            )? > 0)
        })
        .await
    }

    /// Queue every failed entry again, in its place; `true` when any had
    /// failed.
    pub(crate) async fn retry_failed_releases<P: QueuedPayload>(&self) -> Result<bool, DbError> {
        self.call(|sql| {
            Ok(sql.execute(
                &format!(
                    "UPDATE {} SET state = 'queued', error = NULL WHERE state = 'failed'",
                    P::TABLE
                ),
                [],
            )? > 0)
        })
        .await
    }

    /// Pause or resume the queue, saying whether it was paused.
    pub(crate) async fn set_release_queue_paused<P: QueuedPayload>(
        &self,
        paused: bool,
    ) -> Result<bool, DbError> {
        self.call(move |sql| {
            let was: bool = sql.query_row(
                "SELECT paused FROM release_queues WHERE queue = ?",
                [P::QUEUE],
                |row| row.get(0),
            )?;
            sql.execute(
                "UPDATE release_queues SET paused = ?2 WHERE queue = ?1",
                params![P::QUEUE, paused],
            )?;
            Ok(was)
        })
        .await
    }

    /// Take `release_id`'s entry out, in any phase; `false` when the queue
    /// does not hold it.
    pub(crate) async fn cancel_queued_release<P: QueuedPayload>(
        &self,
        release_id: &str,
    ) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.call(move |sql| {
            Ok(sql.execute(
                &format!("DELETE FROM {} WHERE release_id = ?", P::TABLE),
                [release_id],
            )? > 0)
        })
        .await
    }

    /// Every entry of the queue now, each beside its release: what the
    /// automation surface's export status reads.
    #[cfg(any(desktop, test, feature = "test-utils"))]
    pub(crate) async fn release_queue<P: QueuedPayload>(
        &self,
    ) -> Result<ReleaseQueueSnapshot<P, ()>, DbError> {
        self.read(|sql| release_queue_on::<P>(&sql)).await
    }

    /// Follow every entry of the queue, for a test that waits on the whole
    /// queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn subscribe_release_queue<P: QueuedPayload>(
        &self,
    ) -> coven::LiveQuery<ReleaseQueueSnapshot<P, ()>> {
        self.inner
            .handle
            .subscribe(|sql| release_queue_on::<P>(&sql).map_err(CovenError::from))
    }

    /// Follow how many entries the queue holds and the windows `initial`
    /// names of them, as its pane draws them: an entry coming, going,
    /// starting or failing, or a change to a release a window shows. The
    /// windows move by pointing the same query at new ones.
    pub(crate) fn subscribe_release_queue_windows<P: QueuedPayload>(
        &self,
        initial: LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<LibraryPageWindows, ReleaseQueueWindows<P>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |windows, sql| {
                release_queue_windows_on::<P>(&sql, windows).map_err(CovenError::from)
            })
            .process(Ok)
    }

    /// How many entries the queue holds in each phase now, and whether it is
    /// paused.
    pub(crate) async fn release_queue_counts<P: QueuedPayload>(
        &self,
    ) -> Result<ReleaseQueueCounts, DbError> {
        self.read(|sql| release_queue_counts_on::<P>(&sql)).await
    }

    /// Follow how many entries the queue holds in each phase, and whether it
    /// is paused.
    pub(crate) fn subscribe_release_queue_counts<P: QueuedPayload>(
        &self,
    ) -> coven::LiveQuery<ReleaseQueueCounts> {
        self.inner
            .handle
            .subscribe(|sql| release_queue_counts_on::<P>(&sql).map_err(CovenError::from))
    }

    /// Empty both queues and let them run: what the library opens with, since
    /// no transfer outlives the process that ran it.
    pub(crate) async fn clear_release_queues(&self) -> Result<(), DbError> {
        self.call(|sql| {
            sql.execute("DELETE FROM download_queue", [])?;
            sql.execute("DELETE FROM output_queue", [])?;
            sql.execute("UPDATE release_queues SET paused = 0", [])?;
            Ok(())
        })
        .await
    }
}
