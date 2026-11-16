//! What an apply of a catalog release waits on over the network, written to
//! the apply's row in `import_catalog_apply_step` while it waits, for the
//! screen that started it.

use std::sync::Arc;

use bae_catalogs::catalog::Catalog;

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
/// apply's row, which holds each step from its start to its end. A release or
/// cover the library already holds is read without a step.
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

/// An apply a screen shows: its row's key and where the row is written.
struct ShownApply {
    id: CatalogApplyId,
    tables: crate::tables::ImportTables,
    diagnostics: bae_diagnostics::Diagnostics,
    runtime: tokio::runtime::Handle,
}

impl CatalogApplySteps {
    /// The steps of the apply `id` names, written to its row in `tables`.
    /// When the apply is dropped mid-step, a task on `runtime`, run under
    /// `diagnostics`, deletes the row.
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

    /// Where nothing reads an apply's steps, so nothing is written.
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
    /// row before `work` starts and deletes it once `work` ends. When the
    /// caller drops this mid-step, a task deletes the row.
    ///
    /// The row only shows the step, and the apply's result doesn't depend on
    /// it, so a failed write of the row is logged and `work` runs all the
    /// same.
    pub(crate) async fn during<T>(
        &self,
        step: CatalogApplyStep,
        work: impl std::future::Future<Output = T>,
    ) -> T {
        let shown = match &self.written {
            StepsWritten::Unread => return work.await,
            StepsWritten::Row(shown) => shown,
            #[cfg(any(test, feature = "test-utils"))]
            StepsWritten::Recorded(record) => {
                record(Some(step));
                let done = work.await;
                record(None);
                return done;
            }
        };
        // The guard is made before the write, so a drop during the write
        // deletes the row after the write commits: coven commits a write
        // whose caller is gone, and runs the library's writes in the order
        // they were asked.
        let mut running = RunningStep { shown, ended: false };
        if let Err(error) = shown.tables.start_catalog_apply_step(&shown.id, step).await {
            tracing::error!("the apply's step {step:?} was not written: {error}");
        }
        let done = work.await;
        if let Err(error) = shown.tables.end_catalog_apply_step(&shown.id).await {
            tracing::error!("the apply's step {step:?} was not deleted as it ended: {error}");
        }
        running.ended = true;
        done
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
        // Nothing waits on the delete, so a panic in it has nothing to end;
        // the panic hook reports the panic.
        let _task = self.shown.diagnostics.spawn_reported(
            &self.shown.runtime,
            "delete a dropped apply's step",
            async move {
                if let Err(error) = shown.tables.end_catalog_apply_step(&shown.id).await {
                    tracing::error!("a dropped apply's step was not deleted: {error}");
                }
            },
            |_panic| {},
        );
    }
}
