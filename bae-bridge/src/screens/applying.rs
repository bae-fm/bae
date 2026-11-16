//! An apply of a catalog release, an import pick or the release editor's, as
//! the app follows it. The library's handle makes the apply with a new id,
//! the apply writes each network read it waits on to its row under that id,
//! and the screen reads that row through the apply's subscription.

use std::sync::Arc;

use crate::operation_runtime::OperationRuntime;

/// The app asks the library's handle for one for each apply it shows,
/// subscribes to its step with [`Self::subscribe_step`], then hands it to the
/// apply.
#[derive(uniffi::Object)]
pub struct BridgeCatalogApply {
    /// The id of the apply's row.
    id: bae_import::CatalogApplyId,
    /// Where the apply writes its steps: the row `id` names.
    steps: bae_import::CatalogApplySteps,
    services: bae_app::AppServices,
    /// Where the subscription's pulls run.
    runtime: OperationRuntime,
}

/// A new apply over `services`' library, its subscription pulled on
/// `runtime`.
pub(crate) fn catalog_apply(
    services: bae_app::AppServices,
    runtime: OperationRuntime,
) -> Arc<BridgeCatalogApply> {
    let id = services.new_catalog_apply_id();
    let steps = services.catalog_apply_steps(id.clone());
    Arc::new(BridgeCatalogApply { id, steps, services, runtime })
}

#[uniffi::export]
impl BridgeCatalogApply {
    /// The network read the apply waits on, now and as it changes; `None`
    /// before it starts one, between reads, and once it ends.
    pub fn subscribe_step(&self) -> Arc<crate::CatalogApplyStepSubscription> {
        crate::screens::catalog_apply_step(
            self.services.subscribe_catalog_apply_step(&self.id),
            self.runtime.clone(),
        )
    }
}

/// Where core writes an apply's steps: `apply`'s row, or nowhere when the
/// app doesn't show the apply.
pub(crate) fn catalog_apply_steps(
    apply: Option<&BridgeCatalogApply>,
) -> bae_import::CatalogApplySteps {
    apply.map_or_else(bae_import::CatalogApplySteps::unread, |apply| apply.steps.clone())
}
