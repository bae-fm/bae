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
//! fetches the matched pressing's documents and writes the verdict. A settle
//! holds no slot: the next run starts as soon as one answers. The queue is the
//! only writer of verdicts. A result writes the draft only when it found one
//! release, and a run that stores an auto-importable verdict, however it was
//! admitted, starts the candidate's import when "Import automatically when
//! identified" is on, or when a person asked for the import while the settle
//! read the release.

use super::folder_scanner::FolderCandidate;
use crate::extraction::ExtractionSource;
use crate::tables::NewImportCandidateVerdict;
use crate::IdentifyRunId;
use crate::{Admission, CandidateFiles};
use crate::{ImportEvent, ImportServiceHandle, ScanEvent};
use bae_catalogs::rate_limiter::CallPriority;
use bae_identify::LookupChoices;
use bae_identify::{IdentifyState, TerminalVerdict, TitleSearch};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
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

/// How many runs go at once: a cap on local work such as OCR, since the
/// provider rate limiter already paces the network. Settles run beside them.
pub const MAX_IN_FLIGHT: usize = 4;

/// The services the queue runs on.
#[derive(Clone)]
struct Context {
    import: ImportServiceHandle,
    /// The library's store, which the queue reads the person's preferences
    /// from.
    store: bae_store::Store,
    /// Import's tables, which the queue reads candidates' stored state from.
    tables: crate::tables::ImportTables,
    /// Where a bug in the queue's work is reported.
    diagnostics: bae_diagnostics::Diagnostics,
}

/// What the handle asks the queue's loop for.
enum Command {
    /// A person asked for this candidate to be identified now.
    Request { candidate_key: crate::CandidateKey },
    /// A person cancelled these candidates' identification. `done` hears
    /// once the jobs are gone, or the first ending that could not be written.
    Cancel {
        candidate_keys: Vec<crate::CandidateKey>,
        done: tokio::sync::oneshot::Sender<Result<(), crate::ImportError>>,
    },
    /// A person cancelled every identification the queue holds.
    CancelAll { done: tokio::sync::oneshot::Sender<Result<(), crate::ImportError>> },
    /// Say when every release found so far has been admitted and every job
    /// `drain` names has ended.
    #[cfg(any(test, feature = "test-utils"))]
    AwaitDrained { drain: Drain, drained: tokio::sync::oneshot::Sender<()> },
    /// Say when every command sent before this one has been handled and the
    /// queue has started what it has room for.
    #[cfg(any(test, feature = "test-utils"))]
    AwaitCommandsTaken { taken: tokio::sync::oneshot::Sender<()> },
}

/// Which jobs a test waits to see end.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy)]
enum Drain {
    Automatic,
    Every,
}

/// Start the identification queue over `import`, reading the store
/// `store` and import's tables `tables`, and reporting bugs to
/// `diagnostics`. There is one per import
/// service, since it takes the service's event feed.
pub(crate) fn start(
    import: ImportServiceHandle,
    store: bae_store::Store,
    tables: crate::tables::ImportTables,
    diagnostics: bae_diagnostics::Diagnostics,
) -> IdentificationHandle {
    let token = CancellationToken::new();
    let tasks = TaskTracker::new();
    let context = Context { import, store, tables, diagnostics };

    let mut bus =
        context.import.take_event_feed().expect("one identification queue per import service");
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("identification queue runtime");
    let runtime_handle = runtime.handle().clone();
    let loop_token = token.clone();
    let loop_context = context.clone();
    let loop_tasks = tasks.clone();
    tasks.spawn_on(
        async move {
            let ran = bae_util::task_failure::caught(
                "run the identification queue",
                queue::run(&loop_context, &loop_token, &mut bus, &mut command_rx),
            )
            .await;
            // A queue that panicked is reported and stopped as a shutdown
            // stops it, so its executor ends and what asks it after hears it
            // has stopped.
            if let Err(bug) = ran {
                loop_context.diagnostics.panicked(bug);
                loop_token.cancel();
                loop_tasks.close();
            }
        },
        &runtime_handle,
    );

    let completion_tasks = tasks.clone();
    let executor_thread = std::thread::Builder::new()
        .name("bae-import-identification".to_string())
        .spawn(move || runtime.block_on(completion_tasks.wait()))
        .expect("identification queue executor thread");

    IdentificationHandle::new(token, tasks, executor_thread, command_tx)
}

#[cfg(test)]
#[path = "identification_tests.rs"]
pub(crate) mod tests;
