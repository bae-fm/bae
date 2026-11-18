//! The network read that an apply of a catalog release waits on. The apply
//! writes the read to its row in `import_catalog_apply_step` while it waits,
//! and the screen that started the apply reads the row.

use std::sync::Arc;

use bae_catalogs::catalog::Catalog;
use bae_store::DbError;

/// A network read an apply of a catalog release waits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogApplyStep {
    /// Reading a release's documents from its catalog.
    ReadingRelease(Catalog),
    /// Fetching the cover the release is read with from the catalog that
    /// offers it.
    FetchingCover(Catalog),
}

/// The id of one apply of a catalog release that a screen shows: the key of
/// the apply's row. Each apply gets a new one as it starts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogApplyId(String);

impl CatalogApplyId {
    /// A new id from `ids`, which no other apply has.
    pub(crate) fn new(ids: &dyn coven::IdSource) -> Self {
        Self(ids.new_id().to_string())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where an apply of a catalog release writes the step it waits on: the
/// apply's row, which holds each step from its start to its end. The apply
/// reads a release or cover the library already holds without a step.
#[derive(Clone)]
pub struct CatalogApplySteps {
    written: StepsWritten,
}

/// Where an apply's steps go.
#[derive(Clone)]
enum StepsWritten {
    /// Nowhere: nothing reads the apply's steps.
    Unread,
    /// The apply's row, which a screen reads.
    Row(Arc<ShownApply>),
    /// A test's record of each step as it starts, and `None` as it ends, for
    /// a test of which steps an apply runs.
    #[cfg(any(test, feature = "test-utils"))]
    Recorded(Arc<dyn Fn(Option<CatalogApplyStep>) + Send + Sync>),
}

/// An apply a screen shows: its row's key, and the tables the apply writes
/// its row to.
struct ShownApply {
    id: CatalogApplyId,
    tables: crate::tables::ImportTables,
    diagnostics: bae_diagnostics::Diagnostics,
    runtime: tokio::runtime::Handle,
}

impl CatalogApplySteps {
    /// The steps of the apply `id` names, which the apply writes to its row
    /// in `tables`. When the caller drops the apply mid-step, a task on
    /// `runtime` deletes the row, and `diagnostics` reports a delete that
    /// fails.
    pub(crate) fn shown(
        id: CatalogApplyId,
        tables: crate::tables::ImportTables,
        diagnostics: bae_diagnostics::Diagnostics,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            written: StepsWritten::Row(Arc::new(ShownApply { id, tables, diagnostics, runtime })),
        }
    }

    /// The steps of an apply that nothing reads, so the apply writes none.
    pub fn unread() -> Self {
        Self { written: StepsWritten::Unread }
    }

    /// Steps told to `record` as each starts, and `None` as each ends, for a
    /// test of which steps an apply runs and in what order.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn recorded(record: impl Fn(Option<CatalogApplyStep>) + Send + Sync + 'static) -> Self {
        Self { written: StepsWritten::Recorded(Arc::new(record)) }
    }

    /// Run `work` while its apply's row holds `step`. The apply writes the
    /// row before `work` starts, replacing any step the row holds, and
    /// deletes it once `work` ends. When the caller drops this mid-step, a
    /// task deletes the row.
    ///
    /// A failed write of the row is a failed local write, and fails the
    /// apply with its error: a failed start before `work` runs, a failed
    /// delete in place of what `work` returned.
    ///
    /// `work` is awaited at one place only: each place a future is awaited
    /// holds a copy of it in this future.
    pub(crate) async fn during<T, E: From<DbError>>(
        &self,
        step: CatalogApplyStep,
        work: impl std::future::Future<Output = Result<T, E>>,
    ) -> Result<T, E> {
        let started = self.start(step).await?;
        let done = work.await;
        started.end().await?;
        done
    }

    /// Tell `step` where this apply's steps go, as its work starts.
    async fn start(&self, step: CatalogApplyStep) -> Result<StartedStep<'_>, DbError> {
        match &self.written {
            StepsWritten::Unread => Ok(StartedStep::Unread),
            StepsWritten::Row(shown) => {
                // The guard exists before the write starts, so a drop during
                // the write deletes the row after the write commits: coven
                // commits a write whose caller is gone, and runs the
                // library's writes in the order the callers asked for them.
                let mut running = RunningStep { shown, ended: false };
                if let Err(error) = shown.tables.start_catalog_apply_step(&shown.id, step).await {
                    // The failed write rolled back, so the row holds no step.
                    running.ended = true;
                    return Err(error);
                }
                Ok(StartedStep::Row(running))
            }
            #[cfg(any(test, feature = "test-utils"))]
            StepsWritten::Recorded(record) => {
                record(Some(step));
                Ok(StartedStep::Recorded(record))
            }
        }
    }
}

/// A step [`CatalogApplySteps::start`] told where the apply's steps go,
/// which its end tells there too.
enum StartedStep<'a> {
    Unread,
    Row(RunningStep<'a>),
    #[cfg(any(test, feature = "test-utils"))]
    Recorded(&'a Arc<dyn Fn(Option<CatalogApplyStep>) + Send + Sync>),
}

impl StartedStep<'_> {
    /// Tell that the step's work ended.
    async fn end(self) -> Result<(), DbError> {
        match self {
            StartedStep::Unread => Ok(()),
            StartedStep::Row(mut running) => {
                // Coven commits the delete even when the caller drops this
                // while it waits, so the guard has nothing left to delete.
                running.ended = true;
                running.shown.tables.end_catalog_apply_step(&running.shown.id).await
            }
            #[cfg(any(test, feature = "test-utils"))]
            StartedStep::Recorded(record) => {
                record(None);
                Ok(())
            }
        }
    }
}

/// A step whose row the apply has not deleted yet.
struct RunningStep<'a> {
    shown: &'a Arc<ShownApply>,
    ended: bool,
}

impl Drop for RunningStep<'_> {
    fn drop(&mut self) {
        if self.ended {
            return;
        }
        let shown = self.shown.clone();
        // No caller waits on the delete, so the panic callback has no
        // waiter to fail and does nothing; the panic hook reports the panic.
        let _task = self.shown.diagnostics.spawn_reported(
            &self.shown.runtime,
            "delete a dropped apply's step",
            async move {
                if let Err(error) = shown.tables.end_catalog_apply_step(&shown.id).await {
                    shown.diagnostics.report_bug("delete a dropped apply's step", &error);
                }
            },
            |_panic| {},
        );
    }
}
