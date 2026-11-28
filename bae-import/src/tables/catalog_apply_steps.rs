//! Each row of `import_catalog_apply_step` holds the network read that an
//! apply of a catalog release waits on now, keyed by the apply's id. The
//! apply writes the row as the read starts and deletes it as the read ends,
//! and the screen that shows the apply reads it. No apply outlives the
//! process, so bae empties the table when it opens the library. Its table
//! is `catalog_apply_steps.sql`.

use super::*;
use crate::{CatalogApplyId, CatalogApplyStep};
use bae_store::WriteStatements;

/// The stored `step` value of `step`, and its catalog.
fn step_columns(step: CatalogApplyStep) -> (&'static str, Catalog) {
    match step {
        CatalogApplyStep::ReadingRelease(catalog) => ("reading_release", catalog),
        CatalogApplyStep::FetchingCover(catalog) => ("fetching_cover", catalog),
    }
}

// The step of the apply `:apply`, while it waits on a read.
coven::read!(
    writer StepOfApply(apply: String),
    r#"
    SELECT waiting.step, waiting.catalog
    FROM import_catalog_apply_step AS waiting
    WHERE waiting.apply_id = :apply
"#
);

/// The read `apply` waits on as its row holds it, or `None` while it waits
/// on none.
fn step_of(
    apply: &CatalogApplyId,
    row: Option<StepOfApplyRow>,
) -> Result<Option<CatalogApplyStep>, DbError> {
    let Some(StepOfApplyRow { step, catalog }) = row else {
        return Ok(None);
    };
    let broken = |what: &str| {
        DbError::unreadable(format!("import_catalog_apply_step of {}", apply.as_str()), what)
    };
    let catalog = Catalog::from_str(&catalog).map_err(|_unknown| broken(&catalog))?;
    match step.as_str() {
        "reading_release" => Ok(Some(CatalogApplyStep::ReadingRelease(catalog))),
        "fetching_cover" => Ok(Some(CatalogApplyStep::FetchingCover(catalog))),
        other => Err(broken(other)),
    }
}

coven::write!(
    KeepApplyStep(apply: &'p str, step: &'static str, catalog: &'static str),
    r#"
    INSERT INTO import_catalog_apply_step (apply_id, step, catalog)
    VALUES (:apply, :step, :catalog)
    ON CONFLICT (apply_id) DO UPDATE SET step = excluded.step, catalog = excluded.catalog
"#
);

coven::write!(
    DeleteApplyStep(apply: &'p str),
    r#"DELETE FROM import_catalog_apply_step WHERE apply_id = :apply"#
);

coven::write!(DeleteAllApplySteps, r#"DELETE FROM import_catalog_apply_step"#);

impl ImportTables {
    /// Write that `apply` waits on `step` now, in place of any step its row
    /// holds.
    pub(crate) async fn start_catalog_apply_step(
        &self,
        apply: &CatalogApplyId,
        step: CatalogApplyStep,
    ) -> Result<(), DbError> {
        let apply = apply.clone();
        let (step, catalog) = step_columns(step);
        self.store
            .call(move |sql| {
                sql.run(&KeepApplyStepParams {
                    apply: apply.as_str(),
                    step,
                    catalog: catalog.as_str(),
                })?;
                Ok(())
            })
            .await
    }

    /// Write that `apply` waits on no read now.
    pub(crate) async fn end_catalog_apply_step(
        &self,
        apply: &CatalogApplyId,
    ) -> Result<(), DbError> {
        let apply = apply.clone();
        self.store
            .call(move |sql| {
                sql.run(&DeleteApplyStepParams { apply: apply.as_str() })?;
                Ok(())
            })
            .await
    }

    /// Delete every apply's step: what the library opens with, since no
    /// apply outlives the process that wrote its step.
    pub(crate) async fn clear_catalog_apply_steps(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.run(&DeleteAllApplyStepsParams {})?;
                Ok(())
            })
            .await
    }

    /// The read `apply` waits on, live: `None` while it waits on none.
    pub(crate) fn subscribe_catalog_apply_step(
        &self,
        apply: &CatalogApplyId,
    ) -> coven::LiveQuery<Option<CatalogApplyStep>, DbError> {
        let statement = StepOfApplyParams { apply: apply.as_str().to_string() };
        let apply = apply.clone();
        self.store.subscribe_optional(statement).process(move |row| step_of(&apply, row))
    }
}

#[cfg(test)]
#[path = "catalog_apply_steps_tests.rs"]
mod tests;
