//! coven's sync state beside a read of the library's tables, the one place
//! the two are put together.
//!
//! coven computes the sync state from what it knows and follows it on its
//! own (`subscribe_sync_status`): it reruns as a file starts or ends a
//! transfer, which no write to a table the library reads would wake. The
//! library's rows (the pending records it names, why a start failed, the
//! queues' counts) are read in one read of their own.
//!
//! The rows are never older than the state they are put with. A start of
//! sync ends a failed start in the transaction that starts it (the
//! `sync_start_failure_ended` trigger), so rows read before the state that
//! start brings would show the failed start beside it.
//! Each value's rows are therefore read after the state it carries arrived;
//! rows newer than the state are as they should be, such as a pending record
//! listed before the state names it.

use coven::CovenResult;
use std::sync::Arc;

/// A read of the library's tables the sync status puts beside coven's sync
/// state.
pub(crate) type RowsRead<R> =
    Arc<dyn Fn(coven::SqlReadContext<'_>) -> CovenResult<R> + Send + Sync>;

/// coven's sync state and a read of the library's tables made after it, put
/// into one value each time either changes.
pub struct WithSyncState<R, T> {
    store: crate::Store,
    state: coven::LiveQuery<coven::SyncStatus, coven::CovenError>,
    /// The rows' own live query, which says when they change; each value's
    /// rows are read afresh, after the latest state.
    changes: coven::LiveQuery<R, coven::CovenError>,
    rows: RowsRead<R>,
    make: fn(coven::SyncStatus, R) -> T,
    latest_state: Option<coven::SyncStatus>,
    /// The value last handed out, so one equal to it isn't handed out again.
    told: Option<T>,
}

impl<R, T> WithSyncState<R, T>
where
    R: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
{
    pub(crate) fn new(
        store: crate::Store,
        state: coven::LiveQuery<coven::SyncStatus, coven::CovenError>,
        changes: coven::LiveQuery<R, coven::CovenError>,
        rows: RowsRead<R>,
        make: fn(coven::SyncStatus, R) -> T,
    ) -> Self {
        Self { store, state, changes, rows, make, latest_state: None, told: None }
    }

    /// The first value once coven's sync state has one, then one each time
    /// the state or the rows change the value. A failed read is handed on;
    /// the next value comes once the reads succeed again. `None` once either
    /// source has ended with its store.
    pub async fn next(&mut self) -> Option<CovenResult<T>> {
        loop {
            tokio::select! {
                state = self.state.next() => match state? {
                    Ok(state) => self.latest_state = Some(state),
                    Err(error) => return Some(Err(error)),
                },
                changed = self.changes.next() => {
                    if let Err(error) = changed? {
                        return Some(Err(error));
                    }
                }
            }
            let Some(state) = &self.latest_state else {
                continue;
            };
            let rows = Arc::clone(&self.rows);
            let rows = match self.store.read_coven(move |sql| rows(sql)).await {
                Ok(rows) => rows,
                Err(error) => return Some(Err(error)),
            };
            let value = (self.make)(state.clone(), rows);
            if self.told.as_ref() == Some(&value) {
                continue;
            }
            self.told = Some(value.clone());
            return Some(Ok(value));
        }
    }

    /// How often the rows' live query had run when it last said they
    /// changed, for a test that asks how often a write reran it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn runs(&self) -> coven::LiveQueryRuns {
        self.changes.runs()
    }
}
