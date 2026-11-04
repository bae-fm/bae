//! A library's close or removal as the app follows it: the stream the call
//! reports its steps in, which the call writes. The app makes the object (its
//! constructor is the host's, whose runtime outlives the library's and pulls
//! the stream), starts reading its steps, then hands it to the call.

use std::sync::{Arc, Mutex};

use crate::keyed_values::{KeyedValueStream, StreamValues, StreamWriter};
use crate::operation_runtime::OperationRuntime;
use crate::types::{BridgeClosingStep, BridgeFastValue};

/// The app makes one for each close or removal it shows, starts reading its
/// steps with [`Self::feed_closing_steps`], then hands it to the call.
#[derive(uniffi::Object)]
pub struct BridgeLibraryClosing {
    /// Where the stream's pulls run: the host's runtime.
    runtime: OperationRuntime,
    /// Where the close reports its steps: the stream
    /// [`Self::feed_closing_steps`] last handed out.
    steps: Mutex<Option<Arc<StreamValues>>>,
    /// Every step the close wrote, and `None` where its writing ended, so a
    /// test reads steps that come and go faster than it could look.
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeClosingStep>>>>,
}

/// A closing whose stream is pulled on `runtime`.
pub(crate) fn library_closing(runtime: OperationRuntime) -> Arc<BridgeLibraryClosing> {
    Arc::new(BridgeLibraryClosing {
        runtime,
        steps: Mutex::new(None),
        #[cfg(test)]
        written: Arc::default(),
    })
}

#[uniffi::export]
impl BridgeLibraryClosing {
    /// The stream the close reports its steps in: its one key, `""`, while
    /// the close runs a step, and nothing once it ends. A cancelled stream
    /// holds nothing from then on.
    pub fn feed_closing_steps(&self) -> Arc<KeyedValueStream> {
        let values = StreamValues::new();
        *self.steps.lock().unwrap() = Some(values.clone());
        Arc::new(KeyedValueStream::new(
            values,
            bae_library::CancellationToken::new(),
            self.runtime.clone(),
        ))
    }
}

impl BridgeLibraryClosing {
    /// Where the close writes its steps; dropping it, as the close ends,
    /// empties the stream. A closing nobody reads writes nothing.
    pub(crate) fn writer(&self) -> ClosingSteps {
        ClosingSteps {
            writer: self.steps.lock().unwrap().clone().map(StreamWriter::new),
            #[cfg(test)]
            written: Some(self.written.clone()),
        }
    }

    /// What the close wrote, in order, `None` where its writing ended.
    #[cfg(test)]
    pub(crate) fn written(&self) -> Vec<Option<BridgeClosingStep>> {
        self.written.lock().unwrap().clone()
    }
}

/// A close's writing of its stream; a close no app follows writes nowhere.
pub(crate) struct ClosingSteps {
    writer: Option<StreamWriter>,
    #[cfg(test)]
    written: Option<Arc<Mutex<Vec<Option<BridgeClosingStep>>>>>,
}

impl ClosingSteps {
    /// Where nothing reads a close's steps.
    pub(crate) fn unread() -> Self {
        Self {
            writer: None,
            #[cfg(test)]
            written: None,
        }
    }

    /// Where `closing`, the close the app shows, reads the steps; where
    /// nothing does without it.
    pub(crate) fn of(closing: Option<Arc<BridgeLibraryClosing>>) -> Self {
        closing.map_or_else(Self::unread, |closing| closing.writer())
    }

    /// Stop the desktop services, the step every close starts with.
    #[cfg(feature = "desktop")]
    pub(crate) async fn stop_servers(&self, desktop: &bae_desktop::DesktopServices) {
        self.step(BridgeClosingStep::StoppingServers);
        desktop.shutdown().await;
    }

    /// The close runs core's `step`.
    pub(crate) fn core(&self, step: bae_library::ClosingStep) {
        self.step(BridgeClosingStep::from_core(step));
    }

    /// The close runs `step`.
    pub(crate) fn step(&self, step: BridgeClosingStep) {
        #[cfg(test)]
        if let Some(written) = &self.written {
            written.lock().unwrap().push(Some(step));
        }
        if let Some(writer) = &self.writer {
            writer.set(String::new(), BridgeFastValue::ClosingStep { step });
        }
    }
}

#[cfg(test)]
impl Drop for ClosingSteps {
    fn drop(&mut self) {
        if let Some(written) = &self.written {
            written.lock().unwrap().push(None);
        }
    }
}

#[cfg(test)]
#[path = "closing_tests.rs"]
mod tests;
