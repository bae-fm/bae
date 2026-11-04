//! An apply of a catalog release, an import pick or the release editor's, as
//! the app follows it: the stream the apply reports the network read it waits
//! on in, which the apply writes. The app makes the object (its constructor
//! is the host's, whose runtime the stream is pulled on), starts reading its
//! steps, then hands it to the apply.

use std::sync::Arc;

use crate::keyed_values::{KeyedValueStream, ProgressStream};
use crate::operation_runtime::OperationRuntime;
use crate::types::{BridgeCatalogApplyStep, BridgeFastValue};

/// The app makes one for each apply it shows, starts reading its steps with
/// [`Self::feed_apply_steps`], then hands it to the apply.
#[derive(uniffi::Object)]
pub struct BridgeCatalogApply {
    /// Where the apply reports its steps: the stream
    /// [`Self::feed_apply_steps`] last handed out.
    steps: ProgressStream,
}

/// An apply whose stream is pulled on `runtime`.
pub(crate) fn catalog_apply(runtime: OperationRuntime) -> Arc<BridgeCatalogApply> {
    Arc::new(BridgeCatalogApply { steps: ProgressStream::new(runtime) })
}

#[uniffi::export]
impl BridgeCatalogApply {
    /// The stream the apply reports its steps in: its one key, `""`, while
    /// the apply waits on a read, and nothing between reads or once it ends.
    /// A cancelled stream holds nothing from then on.
    pub fn feed_apply_steps(&self) -> Arc<KeyedValueStream> {
        self.steps.feed()
    }
}

/// Where core tells an apply's steps: `apply`'s stream, which empties as
/// each step ends and as the apply drops it, or nowhere when the app doesn't
/// show the apply. An apply nobody reads writes nothing.
pub(crate) fn catalog_apply_steps(
    apply: Option<&BridgeCatalogApply>,
) -> bae_import::CatalogApplySteps {
    let Some(apply) = apply else {
        return bae_import::CatalogApplySteps::unread();
    };
    let writer = apply.steps.writer();
    bae_import::CatalogApplySteps::new(move |step| match step {
        Some(step) => writer.set(BridgeFastValue::CatalogApplyStep {
            step: BridgeCatalogApplyStep::from_core(step),
        }),
        None => writer.clear(),
    })
}
