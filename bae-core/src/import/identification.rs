//! One queue for identifying release candidates, whoever asked for it.
//!
//! A candidate is admitted one of two ways:
//!
//! - **Automatic**, at [`CallPriority::Background`]: a release a scan
//!   announces as found for the first time while identification runs on its
//!   own (`found_while_automatic` on [`ScanEvent::FolderCandidate`]). Nothing
//!   else admits a candidate on its own.
//! - **Requested**, at [`CallPriority::Interactive`] and ahead of automatic
//!   jobs: a person asked for it.
//!
//! A job leaves the queue when its answer is stored or fails, when its
//! candidate can no longer be answered, when a person's decision about the
//! candidate ends its run, or when a person cancels it; nothing about a cancel
//! is kept.
//!
//! One loop runs the queue for the library's lifetime: it fills the slots,
//! follows the import bus's events in the order they were sent, found
//! releases among them, and hands each answer to a settle task, which
//! fetches the matched pressing's documents and writes the verdict. The queue
//! is the only writer of verdicts. A result writes the draft only when it
//! found one release, and an automatic run that stores an auto-importable verdict starts
//! the candidate's import when "Import automatically when identified" is on.

use super::handle::{ImportEvent, ImportServiceHandle, ScanEvent};
use super::folder_scanner::FolderCandidate;
use crate::db::{DbImportCandidateState, NewImportCandidateVerdict};
use crate::identify::{IdentifyRunId, IdentifyState, TerminalVerdict, TitleSearch};
use crate::import::candidates::Admission;
use crate::import::LookupChoices;
use crate::library::LibraryManager;
use crate::signals::ExtractionSource;
use crate::util::rate_limiter::CallPriority;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
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

/// How many candidates are identified at once: a cap on local work such as
/// OCR, since the provider rate limiter already paces the network.
const MAX_IN_FLIGHT: usize = 4;

/// A candidate's content hash and file-decision revision. Candidates sharing it
/// are one job, since the answer is stored under it.
type CandidateIdentity = (String, u64);

/// The services the queue runs on.
#[derive(Clone)]
struct Context {
    import: ImportServiceHandle,
    library_manager: LibraryManager,
}

/// What the handle asks the queue's loop for.
enum Command {
    /// A person asked for this candidate to be identified now.
    Request { candidate_key: String },
    /// A person cancelled these candidates' identification. `done` hears
    /// once the jobs are gone.
    Cancel {
        candidate_keys: Vec<String>,
        done: tokio::sync::oneshot::Sender<()>,
    },
    /// A person cancelled every identification the queue holds.
    CancelAll {
        done: tokio::sync::oneshot::Sender<()>,
    },
    /// Say when every release found so far has been admitted and every job
    /// `drain` names has ended.
    #[cfg(any(test, feature = "test-utils"))]
    AwaitDrained {
        drain: Drain,
        drained: tokio::sync::oneshot::Sender<()>,
    },
    /// Say when every command sent before this one has been handled and the
    /// queue has started what it has room for.
    #[cfg(any(test, feature = "test-utils"))]
    AwaitCommandsTaken {
        taken: tokio::sync::oneshot::Sender<()>,
    },
}

/// Which jobs a test waits to see end.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy)]
enum Drain {
    Automatic,
    Every,
}

/// Start the identification queue over `import`. There is one per import
/// service, since it takes the service's event feed.
pub fn start(import: ImportServiceHandle, library_manager: LibraryManager) -> IdentificationHandle {
    let token = CancellationToken::new();
    let tasks = TaskTracker::new();
    let context = Context {
        import,
        library_manager,
    };

    let mut bus = context
        .import
        .take_event_feed()
        .expect("one identification queue per import service");
    let config = context.library_manager.subscribe_config_changes();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("identification queue runtime");
    let runtime_handle = runtime.handle().clone();
    let loop_token = token.clone();
    let loop_context = context.clone();
    tasks.spawn_on(
        async move {
            queue::run(
                &loop_context,
                &loop_token,
                &mut bus,
                &mut command_rx,
                &config,
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
