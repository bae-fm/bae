//! One setup call, as the app follows it: the stream its steps are reported
//! in, which the call writes. A restore, a join, a creation, a first storage
//! connect and a dev reset each take their own.

use std::sync::{Arc, Mutex};

use crate::keyed_values::{KeyedValueStream, StreamValues, StreamWriter};
use crate::types::{
    BridgeBootstrapStatus, BridgeFastValue, BridgeSetupProgress, BridgeSetupStep,
    KeyActivityCallback,
};
use crate::BridgeDiagnostics;

/// The app makes one for each setup call, starts reading its steps with
/// [`Self::feed_setup_steps`], then hands it to that call.
#[derive(uniffi::Object)]
pub struct BridgeLibrarySetup {
    /// Where a throw from the app's activity callback is reported.
    diagnostics: Arc<BridgeDiagnostics>,
    /// Where the call reports its steps: the stream
    /// [`Self::feed_setup_steps`] last handed out.
    steps: Mutex<Option<Arc<StreamValues>>>,
    /// Every value a call wrote, and `None` where its writing ended, so a
    /// test reads steps that come and go faster than it could look.
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeFastValue>>>>,
}

#[uniffi::export]
impl BridgeLibrarySetup {
    /// A setup whose stream reports a throw from the app's activity callback
    /// to `diagnostics`, the process's sink.
    #[uniffi::constructor]
    pub fn new(diagnostics: Arc<BridgeDiagnostics>) -> Arc<Self> {
        Arc::new(Self {
            diagnostics,
            steps: Mutex::new(None),
            #[cfg(test)]
            written: Arc::default(),
        })
    }

    /// The stream the call reports its steps in: its one key, `""`, while
    /// the call runs, and nothing once it ends. A cancelled stream holds
    /// nothing from then on.
    pub fn feed_setup_steps(
        &self,
        activity: Box<dyn KeyActivityCallback>,
    ) -> Arc<KeyedValueStream> {
        let values = self.diagnostics.stream_values(activity);
        *self.steps.lock().unwrap() = Some(values.clone());
        crate::screens::setup_steps(values)
    }
}

impl BridgeLibrarySetup {
    /// Where the call writes its steps; dropping it, as the call ends,
    /// empties the stream. A setup nobody reads writes nothing.
    pub(crate) fn writer(&self) -> SetupSteps {
        SetupSteps {
            writer: self.steps.lock().unwrap().clone().map(StreamWriter::new),
            #[cfg(test)]
            written: self.written.clone(),
        }
    }

    /// What calls wrote, in order, `None` where each one's writing ended.
    #[cfg(test)]
    pub(crate) fn written(&self) -> Vec<Option<BridgeFastValue>> {
        self.written.lock().unwrap().clone()
    }
}

/// A setup call's writing of its stream.
pub(crate) struct SetupSteps {
    writer: Option<StreamWriter>,
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeFastValue>>>>,
}

impl SetupSteps {
    /// The call runs `step`, `done` of its `total` units; a total of 0 has
    /// nothing to count.
    pub(crate) fn step(&self, step: BridgeSetupStep, done: u64, total: u64) {
        self.set(BridgeFastValue::SetupProgress {
            progress: BridgeSetupProgress { step, done, total },
        });
    }

    /// The call runs `step`, which has nothing to count.
    pub(crate) fn uncounted(&self, step: BridgeSetupStep) {
        self.step(step, 0, 0);
    }

    /// Where a restore or join stands now.
    pub(crate) fn bootstrap(&self, status: coven::BootstrapStatus) {
        self.set(BridgeFastValue::BootstrapStatus {
            status: BridgeBootstrapStatus::from_core(status),
        });
    }

    fn set(&self, value: BridgeFastValue) {
        #[cfg(test)]
        self.written.lock().unwrap().push(Some(value.clone()));
        if let Some(writer) = &self.writer {
            writer.set(String::new(), value);
        }
    }
}

#[cfg(test)]
impl Drop for SetupSteps {
    fn drop(&mut self) {
        self.written.lock().unwrap().push(None);
    }
}

#[cfg(test)]
#[path = "library_setup_tests.rs"]
mod tests;
