//! One queue for identifying release candidates, whoever asked for it.
//!
//! Every identification of an import candidate enters here, through one of two
//! admissions:
//!
//! - **Automatic** — while `identify_automatically` is on, every candidate
//!   without a usable stored answer is admitted, at [`CallPriority::Background`].
//! - **Requested** — a person pressed Identify or Retry, changed what a
//!   candidate's lookup asks about, or switched a source off under a run. The
//!   candidate goes to the front and runs at [`CallPriority::Interactive`].
//!
//! One driver loop runs the queue: it fills the slots, listens to the import
//! bus once, and spawns one settle task per answer. The settle step buys the
//! documents of the single pressing a run matched — the tracklist that decides
//! Ready, and everything opening the candidate would otherwise re-fetch — and
//! writes the verdict. The candidate runtime is the queue's published state:
//! the queue writes `queued`, the driver's broadcasts write the run, and the
//! verdict write ends what it started.
//!
//! **It starts and stops with the library, not with a view.**
//! [`crate::library::AppServices`] constructs one and its `Drop` stops it, so
//! the queue is identified whether or not anyone has the Import section open.
//! Opening a view triggers nothing.
//!
//! **It is the one writer of a candidate's verdict**, for both admissions.
//! Everything that decides what to store lives here rather than being spread
//! across the producers. The row's other half — the user's sheet bindings — is
//! written by the import handle, and writing it *clears* the verdict, which is
//! what brings a re-bound candidate back to the automatic admission.
//!
//! **A candidate with a finished result for the files it has right now is not
//! admitted automatically, and nothing else is skipped.** What the draft holds
//! — a pre-fill from the folder's tags, a release a person chose, fields they
//! typed — is not an answer to the question a run asks. A stored result is
//! settled because the settle step and the result are written together, and
//! files that change retire it. A request ignores the stored result outright:
//! it is what the person is asking to replace.
//!
//! **A result changes the draft only when it found one release.** A run that
//! settled on a release writes that release's draft over whatever stood. A run
//! that settled on none — nothing found, several offered, a failure — stores
//! its result and writes no draft: it says what the candidate is not, and a
//! person's pre-fill, edits and pick are none of its business.
//!
//! **Provider failures are answers.** They are stored as failed verdicts and
//! the automatic admission leaves them alone; only a request replaces one.
//! Cancellation and a candidate that vanished mid-flight still write nothing,
//! because neither is an outcome of the candidate's lookup.

use super::handle::{ImportEvent, ImportServiceHandle, ScanEvent};
use super::release_candidate::ReleaseCandidate;
use crate::db::{DbImportCandidateState, NewImportCandidateVerdict};
use crate::identify::{IdentifyRunId, IdentifyState, TerminalVerdict, TitleSearch};
use crate::import::candidates::Admission;
use crate::import::search::MetadataResult;
use crate::import::LookupChoices;
use crate::library::LibraryManager;
use crate::signals::ExtractionSource;
use crate::util::rate_limiter::CallPriority;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, info, warn};

mod admission;
mod handle;
mod queue;
mod settle;

use admission::*;
pub use handle::IdentificationHandle;
use queue::{admit, Queue};
use settle::*;

/// How many candidates are identified at once.
///
/// The local half of a candidate — the folder walk, disc-ID derivation,
/// duration probing, artwork OCR — is CPU and disk work that parallelises, and
/// the network half is serialised by the provider rate limiter however many run
/// at once. So the cap exists to keep OCR off every core, not to pace the
/// network. It applies to every job: a request goes to the front of the queue
/// and takes the next slot, but a batch of requests does not put a run on
/// every core at once. A constant, not configuration: there is no setting a
/// user could meaningfully choose here.
const MAX_IN_FLIGHT: usize = 4;

/// What one candidate's identity is for identification: the bytes it holds and
/// the revision of the file decisions taken over them. Candidates sharing it
/// are one job — the answer one of them stores is keyed by it, and so answers
/// all of them.
type CandidateIdentity = (String, u64);

/// The services the queue runs on. The identify driver and the extraction
/// behind it are the import handle's, so the queue and the commands that decide
/// a candidate act on the same pair.
#[derive(Clone)]
struct Context {
    import: ImportServiceHandle,
    library_manager: LibraryManager,
}

/// What the handle asks the queue for. The queue's own state lives on its loop
/// and is reached only from there, so everything from outside arrives as one of
/// these.
enum Command {
    /// A person asked for this candidate to be identified now.
    Request { candidate_key: String },
    /// Run the automatic admission, and say when everything it is responsible
    /// for has ended. The events that trigger one in the app carry no
    /// acknowledgement, and nothing there waits for the queue to drain — it is
    /// never done, only idle — so this is a test's way of asking.
    #[cfg(any(test, feature = "test-utils"))]
    AdmitAutomatic {
        drained: tokio::sync::oneshot::Sender<()>,
    },
}

/// Start the identification queue. A candidate becoming answerable, a binding
/// change, a completed folder scan, or a person's request puts work on it.
pub fn start(import: ImportServiceHandle, library_manager: LibraryManager) -> IdentificationHandle {
    let token = CancellationToken::new();
    let tasks = TaskTracker::new();
    let context = Context {
        import,
        library_manager,
    };

    // Subscribe before the task is spawned so the launch scan's `Finished`
    // cannot land in the gap between `start` returning and the loop's first
    // `recv`.
    let mut bus = context.import.subscribe_events();
    let mut config = context.library_manager.subscribe_config_changes();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("identification queue runtime");
    let runtime_handle = runtime.handle().clone();
    let relay_token = token.clone();
    tasks.spawn_on(
        async move {
            loop {
                let event = tokio::select! {
                    biased;
                    _ = relay_token.cancelled() => return,
                    event = bus.recv() => event,
                };
                if event_tx.send(event).is_err() {
                    return;
                }
            }
        },
        &runtime_handle,
    );
    let loop_token = token.clone();
    let loop_context = context.clone();
    tasks.spawn_on(
        async move {
            queue::run(
                &loop_context,
                &loop_token,
                &mut event_rx,
                &mut command_rx,
                &mut config,
            )
            .await;
        },
        &runtime_handle,
    );

    let completion_tasks = tasks.clone();
    let executor_thread = std::thread::Builder::new()
        .name("bae-import-identification".to_string())
        .spawn(move || runtime.block_on(completion_tasks.wait()))
        .expect("identification queue executor thread");

    IdentificationHandle::new(context, token, tasks, executor_thread, command_tx)
}

#[cfg(test)]
mod tests;
