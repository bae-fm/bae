//! A library's open and each setup call (a restore, a join, a creation, a
//! first storage connect, a dev reset), as the app follows them: the stream
//! the call reports its steps in, which the call writes. The app makes the
//! object (its constructor is the host's, whose runtime the stream is pulled
//! on), starts reading its steps, then hands it to the call.

use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use crate::keyed_values::{KeyedValueStream, ProgressStream, ProgressWriter};
use crate::operation_runtime::OperationRuntime;
use crate::types::{
    BridgeBootstrapState, BridgeBootstrapStatus, BridgeFastValue, BridgeOpenProgress,
    BridgeSetupProgress, BridgeSetupStep,
};

/// The app makes one for each call that opens a library, starts reading its
/// steps with [`Self::feed_open_steps`], then hands it to that call.
#[derive(uniffi::Object)]
pub struct BridgeLibraryOpening {
    /// Where the open reports its steps: the stream
    /// [`Self::feed_open_steps`] last handed out.
    steps: ProgressStream,
}

/// An opening whose stream is pulled on `runtime`.
pub(crate) fn library_opening(runtime: OperationRuntime) -> Arc<BridgeLibraryOpening> {
    Arc::new(BridgeLibraryOpening { steps: ProgressStream::new(runtime) })
}

#[uniffi::export]
impl BridgeLibraryOpening {
    /// The stream the open reports its steps in: its one key, `""`, while
    /// the open runs a step, and nothing once the open ends. A cancelled
    /// stream holds nothing from then on.
    pub fn feed_open_steps(&self) -> Arc<KeyedValueStream> {
        self.steps.feed()
    }
}

impl BridgeLibraryOpening {
    /// Write each step the open reports into the stream; dropping what this
    /// returns, as the open ends, empties the stream. An opening nobody reads
    /// writes nothing.
    pub(crate) fn report(&self) -> impl Fn(coven::Progress<coven::OpenStep>) + Send + 'static {
        let writer = self.steps.writer();
        move |progress| {
            writer.set(BridgeFastValue::OpenProgress {
                progress: BridgeOpenProgress::from_core(progress),
            });
        }
    }
}

/// The app makes one for each setup call, starts reading its steps with
/// [`Self::feed_setup_steps`], then hands it to that call.
#[derive(uniffi::Object)]
pub struct BridgeLibrarySetup {
    /// Where the call reports its steps: the stream
    /// [`Self::feed_setup_steps`] last handed out.
    steps: ProgressStream,
    /// Every value a call wrote, and `None` where its writing ended, so a
    /// test reads steps that come and go faster than it could look.
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeFastValue>>>>,
}

/// A setup whose stream is pulled on `runtime`.
pub(crate) fn library_setup(runtime: OperationRuntime) -> Arc<BridgeLibrarySetup> {
    Arc::new(BridgeLibrarySetup {
        steps: ProgressStream::new(runtime),
        #[cfg(test)]
        written: Arc::default(),
    })
}

#[uniffi::export]
impl BridgeLibrarySetup {
    /// The stream the call reports its steps in: its one key, `""`, while
    /// the call runs, and nothing once it ends. A cancelled stream holds
    /// nothing from then on.
    pub fn feed_setup_steps(&self) -> Arc<KeyedValueStream> {
        self.steps.feed()
    }
}

impl BridgeLibrarySetup {
    /// Where the call writes its steps; dropping it, as the call ends,
    /// empties the stream. A setup nobody reads writes nothing.
    pub(crate) fn writer(&self) -> SetupSteps {
        SetupSteps {
            writer: self.steps.writer(),
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
    writer: ProgressWriter,
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeFastValue>>>>,
}

impl SetupSteps {
    /// Where a call writes its steps: `setup`'s stream, or nowhere when the
    /// app doesn't show the call.
    pub(crate) fn of(setup: Option<Arc<BridgeLibrarySetup>>) -> Self {
        match setup {
            Some(setup) => setup.writer(),
            None => Self {
                writer: ProgressWriter::unread(),
                #[cfg(test)]
                written: Arc::default(),
            },
        }
    }

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

    /// How far a storage setup has come, as coven reports it.
    pub(crate) fn storage(&self, progress: coven::Progress<coven::StorageSetupStep>) {
        self.set(BridgeFastValue::StorageSetupProgress {
            progress: crate::types::BridgeStorageSetupProgress::from_core(progress),
        });
    }

    /// Where a restore, join or reset stands now, and whether its Cancel can
    /// still stop it.
    pub(crate) fn bootstrap(&self, status: coven::BootstrapStatus, cancellable: bool) {
        self.set(BridgeFastValue::BootstrapState {
            state: BridgeBootstrapState {
                status: BridgeBootstrapStatus::from_core(status),
                cancellable,
            },
        });
    }

    fn set(&self, value: BridgeFastValue) {
        #[cfg(test)]
        self.written.lock().unwrap().push(Some(value.clone()));
        self.writer.set(value);
    }
}

#[cfg(test)]
impl Drop for SetupSteps {
    fn drop(&mut self) {
        self.written.lock().unwrap().push(None);
    }
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
