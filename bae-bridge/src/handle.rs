use bae_app::AppServices;
use bae_playback::QueueEntryId;
use std::sync::PoisonError;
use tracing::info;

#[cfg(desktop)]
use crate::types::BridgeServerStatus;
use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtistDetail,
    BridgeArtistSummary, BridgeComposerDetail, BridgeComposerSummary, BridgeError, BridgeFile,
    BridgeGalleryItem, BridgeGallerySource, BridgeList, BridgeListWindow, BridgePreviewTarget,
    BridgeRelease, BridgeReleaseRoleSummary, BridgeReleaseSummary, BridgeRepeatMode,
    BridgeSaveSyncConfig, BridgeSearchResults, BridgeSortCriterion, BridgeSortValue,
    BridgeStorageFilter, BridgeStorageRow, BridgeStorageRowStatus, BridgeStorageSort,
    BridgeStoreKeyState, BridgeTrack, BridgeTrackGroup, BridgeTrackRoleSummary,
    BridgeTrackSearchResult, BridgeUploadReleasesOutcome, BridgeWorkDetail,
    BridgeWorkReleaseSummary, BridgeWorkSummary, BridgeWorkTrackSummary,
};
#[cfg(desktop)]
use crate::types::{BridgeCoverSelection, BridgeDiscogsSaveOutcome};

#[derive(uniffi::Object)]
pub struct AppHandle {
    services: AppServices,
    /// Shared with the work that closes the library, which can outlive the
    /// handle when the handle is released on its own runtime.
    #[cfg(desktop)]
    desktop: std::sync::Arc<bae_desktop::DesktopServices>,
    #[cfg(feature = "cast")]
    cast: std::sync::Arc<bae_cast::CastController>,
    /// Where the handle's calls run, on the library's runtime.
    runtime: crate::operation_runtime::OperationRuntime,
    /// Where a bug in the handle's work with no caller to fail is reported.
    diagnostics: bae_diagnostics::Diagnostics,
    /// The host that opened the library: it retains the runtime that
    /// prepared coven and its session custody, and removes the library.
    host: std::sync::Arc<crate::BridgeHost>,
    session: Session,
}

/// The exported methods that are one call into the services behind the handle.
///
/// Two shapes. An `async` method runs its body on the app runtime through
/// [`AppHandle::run_exported`] and reports a [`BridgeError`]; a `sync` one is a
/// plain forward. Each block names the handle itself — a macro's own `self` is
/// not the caller's, so the bodies say `this`.
///
/// Both arms write the `#[uniffi::export]` block, because the export is a proc
/// macro and would otherwise see the macro call rather than the methods.
macro_rules! forward {
    (
        $(#[$block:meta])*
        async $this:ident => {
            $(
                $(#[$method:meta])*
                fn $name:ident($($arg:ident: $arg_type:ty),* $(,)?) -> $ret:ty $body:block
            )*
        }
    ) => {
        $(#[$block])*
        #[uniffi::export(async_runtime = "tokio", cancellable)]
        impl AppHandle {
            $(
                $(#[$method])*
                pub async fn $name(
                    self: std::sync::Arc<Self>,
                    $($arg: $arg_type),*
                ) -> Result<$ret, BridgeError> {
                    self.run_exported(concat!("AppHandle.", stringify!($name)), move |$this| async move $body).await
                }
            )*
        }
    };
    (
        $(#[$block:meta])*
        sync $this:ident => {
            $(
                $(#[$method:meta])*
                fn $name:ident($($arg:ident: $arg_type:ty),* $(,)?) $(-> $ret:ty)? $body:block
            )*
        }
    ) => {
        $(#[$block])*
        #[uniffi::export]
        impl AppHandle {
            $(
                $(#[$method])*
                pub fn $name(&self, $($arg: $arg_type),*) $(-> $ret)? {
                    let $this = self;
                    $body
                }
            )*
        }
    };
}

mod base;
mod cloud_operations;
mod configuration;
#[cfg(any(feature = "cast", desktop))]
mod desktop;
#[cfg(desktop)]
mod desktop_services;
#[cfg(desktop)]
mod editing_projection;
mod fast_values;
#[cfg(desktop)]
mod import_list;
#[cfg(desktop)]
mod release_edit;
#[cfg(desktop)]
mod release_grouping;
#[cfg(desktop)]
pub use release_edit::ReleaseEditSession;
#[cfg(desktop)]
mod import_projection;
mod library_projection;
mod live;
mod memory;
mod playback_persistence;
mod queue_projection;
mod service_status;
#[cfg(desktop)]
mod storage_inspector;
mod subscriptions;
mod sync_status;

/// The runtime every task of an open library runs on, and whether the library
/// is still open on it. A library ends before its runtime does: closing the
/// handle, or releasing it, closes an open library on the runtime and only then
/// shuts the runtime down, so nothing of the library is left running for the
/// shutdown to cancel under it.
///
/// Every change to the state is one `mem::replace`, which can't panic
/// partway, so a poisoned lock leaves it whole and says nothing about it; a
/// released handle's `Drop` takes it, where a panic must not start.
struct Session(std::sync::Mutex<SessionState>);

enum SessionState {
    /// The library is open on the runtime, and in its host's record of open
    /// libraries while `record` is held.
    Open { runtime: tokio::runtime::Runtime, record: crate::OpenRecord },
    /// Removing the library deleted its store; only the runtime is left to end.
    Removed(tokio::runtime::Runtime),
    /// The runtime has ended.
    Ended,
}

/// What is left of a session once it has begun to end.
enum Ending {
    /// Close the open library on the runtime once the calls that found it
    /// open have ended, then end the runtime.
    Close { runtime: tokio::runtime::Runtime, calls: crate::OpenCalls },
    /// Removing the library deleted its store; only the runtime is left to end.
    Removed(tokio::runtime::Runtime),
    /// The runtime has ended.
    Ended,
}

impl SessionState {
    /// Begin to end the session. An open library leaves its host's record
    /// and its close is recorded there, under way until the returned close is
    /// dropped, so a call that names the store waits for it.
    fn ending(self) -> (Ending, Option<crate::CloseUnderWay>) {
        match self {
            Self::Open { runtime, record } => {
                let (under_way, calls) = record.close();
                (Ending::Close { runtime, calls }, Some(under_way))
            }
            Self::Removed(runtime) => (Ending::Removed(runtime), None),
            Self::Ended => (Ending::Ended, None),
        }
    }
}

type Closing = Result<(), bae_library::LibraryError>;

/// What closing a released handle's library is reported as.
const CLOSE_RELEASED: &str = "close the library its handle released";

impl Session {
    /// Close the open library with `closing` on its runtime, then shut the
    /// runtime down, waiting for every task on it to end. After a removal
    /// deleted the store only the runtime is left to end. `None` when the
    /// session already ended.
    fn end(&self, closing: impl std::future::Future<Output = Closing>) -> Option<Closing> {
        let state = std::mem::replace(
            &mut *self.0.lock().unwrap_or_else(PoisonError::into_inner),
            SessionState::Ended,
        );
        let (ending, under_way) = state.ending();
        let ended = end(ending, closing);
        drop(under_way);
        ended
    }

    /// [`Self::end`] for a released handle, with no caller to hear a failure:
    /// the close's error is reported to `diagnostics` as the bug it is, and
    /// its panic, which the panic hook reported, is logged there. Released on a thread of a runtime — the last call on the
    /// handle ending on the app runtime — it can't wait there for work only
    /// a runtime can do, so a thread of its own ends the session, and the
    /// close stays under way in the host's record until that thread ends.
    fn end_released(
        &self,
        closing: impl std::future::Future<Output = Closing> + Send + 'static,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) {
        let state = std::mem::replace(
            &mut *self.0.lock().unwrap_or_else(PoisonError::into_inner),
            SessionState::Ended,
        );
        let (state, under_way) = state.ending();
        if matches!(state, Ending::Ended) {
            return;
        }
        let reporter = diagnostics.clone();
        let ending: Box<dyn FnOnce() + Send> = Box::new(move || {
            match bae_util::task_failure::caught_sync(CLOSE_RELEASED, || end(state, closing)) {
                Ok(None | Some(Ok(()))) => {}
                Ok(Some(Err(error))) => reporter.report_failure(CLOSE_RELEASED, &error),
                Err(panicked) => {
                    reporter.panicked(panicked);
                }
            }
        });
        if tokio::runtime::Handle::try_current().is_err() {
            ending();
            drop(under_way);
            return;
        }
        // The thread is handed the close once it has started, so a close
        // whose thread can't start is still here, not dropped with the
        // spawn's error.
        let (work_tx, work) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        // The close under way moves into the thread, so it ends with the
        // thread, or with a thread that could not start, never with the close
        // forgotten below: a call waiting for it goes on and finds the store
        // still held.
        let started = diagnostics.spawn_thread_reported(
            "bae-library-close",
            move || {
                if let Ok(ending) = work.recv() {
                    ending();
                }
                drop(under_way);
            },
            // The close catches its own panic; the close under way drops as
            // a panic outside it unwinds, ending the close the record holds.
            |_| {},
        );
        let (unstarted, why) = match started {
            Ok(_) => match work_tx.send(ending) {
                Ok(()) => return,
                Err(unsent) => {
                    (unsent.0, "its thread ended before it was handed the close".to_string())
                }
            },
            Err(error) => (ending, format!("its thread could not start: {error}")),
        };
        // A runtime thread can neither run the close nor drop the runtime, so
        // the library stays open, owned by nothing, until the process exits;
        // opening it again finds its store held and says so.
        diagnostics.report_bug(CLOSE_RELEASED, &format!("the library was left open: {why}"));
        std::mem::forget(unstarted);
    }

    /// Record that removing the open library deleted its store.
    fn removed(&self) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *state = match std::mem::replace(&mut *state, SessionState::Ended) {
            // The store is gone: the library leaves its host's record with no
            // close to wait for.
            SessionState::Open { runtime, record } => {
                drop(record);
                SessionState::Removed(runtime)
            }
            ended @ (SessionState::Removed(_) | SessionState::Ended) => ended,
        };
    }
}

/// See [`Session::end`]. After a removal, `closing` is dropped unrun once the
/// runtime has ended, so what it holds outlives the runtime's tasks.
fn end(ending: Ending, closing: impl std::future::Future<Output = Closing>) -> Option<Closing> {
    let (runtime, closed) = match ending {
        Ending::Close { runtime, calls } => {
            calls.wait();
            let closed = runtime.block_on(closing);
            (runtime, closed)
        }
        Ending::Removed(runtime) => {
            drop(runtime);
            drop(closing);
            return Some(Ok(()));
        }
        Ending::Ended => return None,
    };
    drop(runtime);
    Some(closed)
}

#[cfg(desktop)]
pub use editing_projection::{bridge_validation_reason_key, shape_release_edit};

impl AppHandle {
    /// Start the handle over a library `host` opened. `diagnostics` is where
    /// a panic in the handle's operations, its deliveries to the host, and
    /// casting's background work is reported; `ids` is where a cast
    /// session's ids come from.
    pub(crate) fn start(
        services: AppServices,
        runtime: tokio::runtime::Runtime,
        diagnostics: bae_diagnostics::Diagnostics,
        #[cfg(feature = "cast")] ids: coven::IdSourceRef,
        host: std::sync::Arc<crate::BridgeHost>,
        record: crate::OpenRecord,
    ) -> Result<Self, bae_app::BootstrapError> {
        #[cfg(desktop)]
        let desktop = {
            let runtime_handle = runtime.handle().clone();
            let starting = services.clone();
            let started = runtime.block_on(bae_util::task_failure::joined(
                "start the desktop services",
                crate::operation_runtime::spawn(runtime.handle(), "start the desktop services", {
                    let diagnostics = diagnostics.clone();
                    move || {
                        bae_desktop::DesktopServices::start(starting, runtime_handle, diagnostics)
                    }
                }),
            ));
            match started {
                Ok(desktop) => std::sync::Arc::new(desktop),
                // The library is open on the runtime, so it closes before the
                // runtime ends.
                Err(failed) => {
                    let failed = diagnostics.bug(failed);
                    if let Err(closing) = runtime.block_on(services.close()) {
                        diagnostics
                            .report_failure("close the library a failed open left open", &closing);
                    }
                    return Err(bae_app::BootstrapError::Internal(format!(
                        "desktop services failed to start: {failed}"
                    )));
                }
            }
        };
        #[cfg(feature = "cast")]
        let cast = bae_cast::CastController::start(
            services.clone(),
            runtime.handle().clone(),
            bae_cast::renderer::RendererDiscovery::for_host(),
            ids,
            diagnostics.clone(),
        );

        Ok(Self {
            services,
            #[cfg(desktop)]
            desktop,
            #[cfg(feature = "cast")]
            cast,
            runtime: crate::operation_runtime::OperationRuntime::new(
                runtime.handle().clone(),
                diagnostics.clone(),
            ),
            diagnostics,
            session: Session(std::sync::Mutex::new(SessionState::Open { runtime, record })),
            host,
        })
    }

    /// Set S3 up as the library's storage, writing each step coven runs into
    /// `steps`: the exported `save_sync_config` and the dev reset both set
    /// storage up through it.
    pub(crate) async fn save_storage(
        &self,
        config: BridgeSaveSyncConfig,
        steps: &crate::screens::SetupSteps,
    ) -> Result<crate::types::BridgeStorageSetup, BridgeError> {
        Ok(self
            .services
            .save_s3_config(config.into_core()?, &|progress| steps.storage(progress))
            .await
            .map(crate::types::BridgeStorageSetup::from_core)?)
    }

    async fn run_exported<T, Build, Fut>(
        self: std::sync::Arc<Self>,
        operation: &'static str,
        build: Build,
    ) -> Result<T, BridgeError>
    where
        T: Send + 'static,
        Build: FnOnce(std::sync::Arc<Self>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, BridgeError>> + Send + 'static,
    {
        let runtime = self.runtime.clone();
        runtime.run(operation, move || build(self)).await
    }

    /// Closing the library, as its runtime runs it: the desktop services, then
    /// playback, identification, import, sync, the manager's tasks and the
    /// store, each step written to `steps` as it starts. It holds the host,
    /// whose runtime prepared the library's store, until the library has
    /// closed.
    fn closing(
        &self,
        steps: crate::screens::ClosingSteps,
    ) -> impl std::future::Future<Output = Closing> + Send + 'static {
        let services = self.services.clone();
        #[cfg(desktop)]
        let desktop = self.desktop.clone();
        let host = self.host.clone();
        async move {
            #[cfg(desktop)]
            steps.stop_servers(&desktop).await;
            let closed = services.close_reporting(&|step| steps.core(step)).await;
            drop(host);
            closed
        }
    }
}

/// A handle released without [`AppHandle::close_library`] closes its library
/// the same way, and a failure, which no caller hears, is reported as a bug.
/// A library that closes on a thread of its own is handed to the host, which
/// waits for it before opening or removing the library again.
impl Drop for AppHandle {
    fn drop(&mut self) {
        self.session.end_released(
            self.closing(crate::screens::ClosingFeed::new(None).steps()),
            &self.diagnostics,
        );
    }
}

#[uniffi::export]
impl AppHandle {
    /// Close the library so nothing of this process holds its files: stop the
    /// desktop services, playback, identification and import, save where
    /// playback was, stop sync, close the store, then shut the app runtime
    /// down, waiting for every task on it to end. After a removal deleted the
    /// store only the runtime is left to end. Releasing the handle does the
    /// same; closing it first is how a caller waits for it and hears a
    /// failure.
    ///
    /// Blocks until done, so it is called off the UI thread and never from the
    /// app runtime. Every later call on this handle fails. Each step the close
    /// runs is written to `closing`'s stream, when the app shows the close.
    pub fn close_library(
        &self,
        closing: Option<std::sync::Arc<crate::BridgeLibraryClosing>>,
    ) -> Result<(), BridgeError> {
        self.session
            .end(self.closing(crate::screens::ClosingFeed::new(closing).steps()))
            .ok_or_else(|| {
                BridgeError::from(bae_library::LibraryError::Internal(
                    "the library is already closed".to_string(),
                ))
            })??;
        info!("Closed library");
        Ok(())
    }
}

#[cfg(test)]
#[path = "handle_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
mod session_tests;

mod personal;

pub use personal::*;

mod membership;
pub use membership::*;

mod removal;
pub use removal::*;

mod admin;
