//! A library's open and each setup call (a restore, a join, a creation, a
//! first storage connect, a dev reset), as the app follows them: the stream
//! the call reports its steps in, which the call writes, and, for a restore,
//! a join or a reset, where it stands, and for a storage setup, whether it
//! is past its point of no return, which the app pulls as it pulls every
//! screen value. The app makes the object (its constructor is the host's,
//! whose runtime the stream and the states are pulled on), starts reading
//! its steps, then hands it to the call.

use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use bae_library::CancellationToken;
use tokio::sync::watch;

use crate::keyed_values::{KeyedValueStream, ProgressStream, ProgressWriter};
use crate::operation_runtime::OperationRuntime;
use crate::types::{
    BridgeBootstrapState, BridgeBootstrapStatus, BridgeError, BridgeFastValue, BridgeOpenProgress,
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
    /// Where a restore, a join or a reset says where it stands.
    states: SetupStateWatch<BridgeBootstrapState>,
    /// Where a storage setup says whether it is past its point of no return.
    storage_finishing: SetupStateWatch<bool>,
    /// Every value a call wrote, and `None` where its writing ended, so a
    /// test reads steps that come and go faster than it could look.
    #[cfg(test)]
    written: Arc<Mutex<Vec<Option<BridgeFastValue>>>>,
}

/// A setup whose stream is pulled on `runtime`.
pub(crate) fn library_setup(runtime: OperationRuntime) -> Arc<BridgeLibrarySetup> {
    Arc::new(BridgeLibrarySetup {
        steps: ProgressStream::new(runtime.clone()),
        states: SetupStateWatch::new(runtime.clone()),
        storage_finishing: SetupStateWatch::new(runtime),
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

    /// Where a restore, a join or a reset stands, and whether its Cancel can
    /// still stop it: the latest once the call has said, then each change.
    /// A step's counts are in the steps' stream instead, so they change
    /// nothing here.
    pub fn subscribe_states(&self) -> Arc<SetupStatesSubscription> {
        setup_states(self.states.read(), self.states.runtime.clone())
    }

    /// Whether a storage setup is past its point of no return, creating the
    /// library in storage or recording changed access, where it finishes
    /// rather than undoing and its screen offers no Cancel: the latest once
    /// the call has reported a step, then each change.
    pub fn subscribe_storage_finishing(&self) -> Arc<StorageSetupFinishingSubscription> {
        storage_setup_finishing(
            self.storage_finishing.read(),
            self.storage_finishing.runtime.clone(),
        )
    }
}

/// Where a call stands, as it says it and its screens pull it: `None` until
/// the call says.
struct SetupStateWatch<S> {
    states: Arc<watch::Sender<Option<S>>>,
    /// Where the states are pulled.
    runtime: OperationRuntime,
}

impl<S> SetupStateWatch<S> {
    fn new(runtime: OperationRuntime) -> Self {
        Self { states: Arc::new(watch::channel(None).0), runtime }
    }

    /// A read answering with the latest state once the call has said one,
    /// then each change.
    fn read(&self) -> SetupStates<S> {
        let mut states = self.states.subscribe();
        states.mark_changed();
        SetupStates { states: tokio::sync::Mutex::new(states), cancelled: CancellationToken::new() }
    }

    /// Where one call says where it stands.
    fn writer(&self) -> SetupStateWriter<S> {
        SetupStateWriter { states: self.states.clone() }
    }
}

/// One call's saying where it stands.
struct SetupStateWriter<S> {
    states: Arc<watch::Sender<Option<S>>>,
}

impl<S: PartialEq> SetupStateWriter<S> {
    /// The call stands at `state`; a state that changed wakes the
    /// subscriptions, one that didn't wakes nothing.
    fn say(&self, state: S) {
        self.states.send_if_modified(|current| {
            let changed = current.as_ref() != Some(&state);
            if changed {
                *current = Some(state);
            }
            changed
        });
    }
}

/// The states one subscription pulls: each that it has not answered with.
pub(crate) struct SetupStates<S> {
    states: tokio::sync::Mutex<watch::Receiver<Option<S>>>,
    cancelled: CancellationToken,
}

impl<S: Clone + Send + Sync + 'static> crate::screens::PulledRead for SetupStates<S> {
    type Value = S;

    async fn next(&self) -> Result<S, BridgeError> {
        tokio::select! {
            biased;
            () = self.cancelled.cancelled() => Err(BridgeError::Cancelled),
            state = async {
                let mut states = self.states.lock().await;
                loop {
                    // The setup is gone once every sender is: nothing more
                    // will be said.
                    if states.changed().await.is_err() {
                        return Err(BridgeError::Cancelled);
                    }
                    if let Some(state) = states.borrow_and_update().clone() {
                        return Ok(state);
                    }
                }
            } => state,
        }
    }

    async fn cancel(&self) -> Result<(), BridgeError> {
        self.cancelled.cancel();
        Ok(())
    }
}

pulled_read! {
    /// Where a restore, a join or a reset stands, as its screen shows it.
    SetupStatesSubscription: SetupStates<BridgeBootstrapState> => BridgeBootstrapState,
    converted by |state: BridgeBootstrapState| state,
    built by setup_states
}

pulled_read! {
    /// Whether a storage setup is past its point of no return, as its
    /// screen shows Cancel or not.
    StorageSetupFinishingSubscription: SetupStates<bool> => bool,
    converted by |finishing: bool| finishing,
    built by storage_setup_finishing
}

impl BridgeLibrarySetup {
    /// Where the call writes its steps; dropping it, as the call ends,
    /// empties the stream. A setup nobody reads writes nothing.
    pub(crate) fn writer(&self) -> SetupSteps {
        SetupSteps {
            writer: self.steps.writer(),
            states: Some(self.states.writer()),
            storage_finishing: Some(self.storage_finishing.writer()),
            #[cfg(test)]
            written: self.written.clone(),
        }
    }

    /// What calls wrote, in order, `None` where each one's writing ended.
    #[cfg(test)]
    pub(crate) fn written(&self) -> Vec<Option<BridgeFastValue>> {
        self.written.lock().unwrap().clone()
    }

    /// Where a call writes its steps: `setup`'s stream, or nowhere when the
    /// app doesn't show the call.
    pub(crate) fn steps(setup: Option<Arc<Self>>) -> SetupSteps {
        match setup {
            Some(setup) => setup.writer(),
            None => SetupSteps {
                writer: ProgressStream::unread(),
                states: None,
                storage_finishing: None,
                #[cfg(test)]
                written: Arc::default(),
            },
        }
    }
}

/// A setup call's writing of its stream.
pub(crate) struct SetupSteps {
    writer: ProgressWriter,
    /// Where the call says where it stands; `None` when the app doesn't show
    /// the call.
    states: Option<SetupStateWriter<BridgeBootstrapState>>,
    /// Where a storage setup says whether it is past its point of no return;
    /// `None` when the app doesn't show the call.
    storage_finishing: Option<SetupStateWriter<bool>>,
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

    /// How far a storage setup has come, as coven reports it, and whether
    /// the step it runs is past its point of no return: a change of that
    /// wakes its subscriptions, and each step's counts go to the stream.
    pub(crate) fn storage(&self, progress: coven::Progress<coven::StorageSetupStep>) {
        if let Some(finishing) = &self.storage_finishing {
            finishing.say(bae_library::storage_setup_finishing(&progress.step));
        }
        self.set(BridgeFastValue::StorageSetupProgress {
            progress: crate::types::BridgeStorageSetupProgress::from_core(progress),
        });
    }

    /// Where a restore, join or reset stands now, and whether its Cancel can
    /// still stop it: a state that changed wakes its subscriptions, and the
    /// counts of the step it runs go to the stream, which holds nothing
    /// between counted steps.
    pub(crate) fn bootstrap(&self, status: coven::BootstrapStatus, cancellable: bool) {
        let (status, step) = BridgeBootstrapStatus::from_core(status);
        if let Some(states) = &self.states {
            states.say(BridgeBootstrapState { status, cancellable });
        }
        match step {
            Some(step) => self.set(BridgeFastValue::BootstrapStep { step }),
            None => self.clear(),
        }
    }

    fn set(&self, value: BridgeFastValue) {
        #[cfg(test)]
        self.written.lock().unwrap().push(Some(value.clone()));
        self.writer.set(value);
    }

    /// The stream holds nothing until the next step that counts.
    fn clear(&self) {
        self.writer.clear();
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
