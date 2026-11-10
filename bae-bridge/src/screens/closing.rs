//! A library's close or removal as the app follows it: the stream the call
//! reports its steps in, which the call writes. The app makes the object (its
//! constructor is the host's, whose runtime outlives the library's and pulls
//! the stream), starts reading its steps, then hands it to the call.

use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use crate::keyed_values::{KeyedValueStream, ProgressStream, ProgressWriter};
use crate::operation_runtime::OperationRuntime;
use crate::types::{BridgeClosingStep, BridgeFastValue};

/// The app makes one for each close or removal it shows, starts reading its
/// steps with [`Self::feed_closing_steps`], then hands it to the call.
#[derive(uniffi::Object)]
pub struct BridgeLibraryClosing {
    /// Where the close reports its steps: the stream
    /// [`Self::feed_closing_steps`] last handed out.
    steps: ProgressStream,
    /// Every step the close wrote, and `None` where its writing ended, so a
    /// test reads steps that come and go faster than it could look.
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeClosingStep>>>>,
    /// A step a test holds the close at, until it lets the close go on.
    #[cfg(test)]
    held: Arc<Mutex<Option<HeldStep>>>,
}

/// The step a test holds a close at: as the close reaches it, `reached` hears
/// so, and the close waits there until `release` hears from the test.
#[cfg(test)]
struct HeldStep {
    step: BridgeClosingStep,
    reached: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

/// A closing whose stream is pulled on `runtime`.
pub(crate) fn library_closing(runtime: OperationRuntime) -> Arc<BridgeLibraryClosing> {
    Arc::new(BridgeLibraryClosing {
        steps: ProgressStream::new(runtime),
        #[cfg(test)]
        written: Arc::default(),
        #[cfg(test)]
        held: Arc::default(),
    })
}

#[uniffi::export]
impl BridgeLibraryClosing {
    /// The stream the close reports its steps in: its one key, `""`, while
    /// the close runs a step, and nothing once it ends. A cancelled stream
    /// holds nothing from then on.
    pub fn feed_closing_steps(&self) -> Arc<KeyedValueStream> {
        self.steps.feed()
    }
}

impl BridgeLibraryClosing {
    /// What the close wrote, in order, `None` where its writing ended.
    #[cfg(test)]
    pub(crate) fn written(&self) -> Vec<Option<BridgeClosingStep>> {
        self.written.lock().unwrap().clone()
    }

    /// Hold the close at `step`: the first receiver hears when the close
    /// reaches it, and the close goes on once the sender sends.
    #[cfg(test)]
    pub(crate) fn hold_at(
        &self,
        step: BridgeClosingStep,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (reached, reaching) = std::sync::mpsc::channel();
        let (releasing, release) = std::sync::mpsc::channel();
        *self.held.lock().unwrap() = Some(HeldStep { step, reached, release });
        (reaching, releasing)
    }
}

/// The close the app shows, if it shows one: what the app passed to the call.
/// A close the app doesn't show is the empty feed, and writes nowhere.
pub(crate) struct ClosingFeed(Option<Arc<BridgeLibraryClosing>>);

impl ClosingFeed {
    /// The feed of `BridgeLibraryClosing`, the object the app passed, or the empty feed.
    pub(crate) fn new(shown: Option<Arc<BridgeLibraryClosing>>) -> Self {
        Self(shown)
    }

    /// Where the close writes its steps; dropping it, as the close ends,
    /// empties the stream. A closing nobody reads writes nothing.
    pub(crate) fn steps(&self) -> ClosingSteps {
        match &self.0 {
            Some(closing) => ClosingSteps {
                writer: Some(closing.steps.writer()),
                #[cfg(test)]
                written: Some(closing.written.clone()),
                #[cfg(test)]
                held: Some(closing.held.clone()),
            },
            None => ClosingSteps {
                writer: None,
                #[cfg(test)]
                written: None,
                #[cfg(test)]
                held: None,
            },
        }
    }
}

/// A close's writing of its stream; a close no app follows writes nowhere.
pub(crate) struct ClosingSteps {
    writer: Option<ProgressWriter>,
    #[cfg(test)]
    written: Option<Arc<Mutex<Vec<Option<BridgeClosingStep>>>>>,
    #[cfg(test)]
    held: Option<Arc<Mutex<Option<HeldStep>>>>,
}

impl ClosingSteps {
    /// Stop the desktop services, the step every close starts with.
    #[cfg(desktop)]
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
            writer.set(BridgeFastValue::ClosingStep { step });
        }
        #[cfg(test)]
        if let Some(held) = &self.held {
            let mut held = held.lock().unwrap();
            if held.as_ref().is_some_and(|held| held.step == step) {
                let held = held.take().expect("the held step is there");
                held.reached.send(()).expect("the test waits for the held step");
                held.release.recv().expect("the test lets the close go on");
            }
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
