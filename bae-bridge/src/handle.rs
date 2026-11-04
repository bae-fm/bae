use bae_app::AppServices;
use bae_playback::QueueEntryId;
use std::sync::PoisonError;
use tracing::info;

#[cfg(feature = "desktop")]
use crate::types::BridgeDiscogsSaveOutcome;
use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtistDetail,
    BridgeArtistSortCriterion, BridgeArtistSummary, BridgeCatalog, BridgeComposerDetail,
    BridgeComposerSortCriterion, BridgeComposerSummary, BridgeCoverSelection, BridgeError,
    BridgeFile, BridgeGalleryItem, BridgeGallerySource, BridgePreviewTarget, BridgeRelease,
    BridgeReleaseRoleSummary, BridgeReleaseSummary, BridgeRepeatMode, BridgeSaveSyncConfig,
    BridgeSearchResults, BridgeSortCriterion, BridgeStorageFilter, BridgeStorageRow,
    BridgeStorageRowStatus, BridgeStorageSort, BridgeStoreKeyState, BridgeTrack, BridgeTrackGroup,
    BridgeTrackRoleSummary, BridgeTrackSearchResult, BridgeUploadReleasesOutcome, BridgeWorkDetail,
    BridgeWorkReleaseSummary, BridgeWorkSummary, BridgeWorkTrackSummary,
};
#[cfg(feature = "desktop")]
use crate::types::{BridgeMcpServerStatus, BridgeSubsonicServerStatus};

#[derive(uniffi::Object)]
pub struct AppHandle {
    services: AppServices,
    /// Shared with the work that closes the library, which can outlive the
    /// handle when the handle is released on its own runtime.
    #[cfg(feature = "desktop")]
    desktop: std::sync::Arc<bae_desktop::DesktopServices>,
    #[cfg(feature = "cast")]
    cast: std::sync::Arc<bae_cast::CastController>,
    /// Where the handle's calls run, on the library's runtime.
    runtime: crate::operation_runtime::OperationRuntime,
    /// Where a bug in the handle's work with no caller to fail is reported.
    diagnostics: bae_diagnostics::Diagnostics,
    /// Retains the runtime that prepared coven and its session custody.
    _host: Option<std::sync::Arc<crate::BridgeHost>>,
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
#[cfg(feature = "desktop")]
mod desktop_services;
#[cfg(feature = "desktop")]
mod editing_projection;
mod fast_values;
#[cfg(feature = "desktop")]
mod import_list;
#[cfg(feature = "desktop")]
mod release_edit;
#[cfg(feature = "desktop")]
mod release_grouping;
#[cfg(feature = "desktop")]
pub use release_edit::ReleaseEditSession;
#[cfg(feature = "desktop")]
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
    /// The library is open on the runtime.
    Open(tokio::runtime::Runtime),
    /// Removing the library deleted its store; only the runtime is left to end.
    Removed(tokio::runtime::Runtime),
    /// The runtime has ended.
    Ended,
}

type Closing = Result<(), bae_library::LibraryError>;

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
        end(state, closing)
    }

    /// [`Self::end`] for a released handle, with no caller to hear a failure,
    /// which `failed` is told of: the close's error, or its panic. Released
    /// on a thread of a runtime — the last call on the handle ending on the
    /// app runtime — it can't wait there for work only a runtime can do, so a
    /// thread of its own ends the session, and `ending_on` is handed the
    /// thread to wait on.
    fn end_released(
        &self,
        closing: impl std::future::Future<Output = Closing> + Send + 'static,
        failed: impl Fn(&dyn std::fmt::Display) + Send + Sync + 'static,
        ending_on: impl FnOnce(std::thread::JoinHandle<()>),
    ) {
        let state = std::mem::replace(
            &mut *self.0.lock().unwrap_or_else(PoisonError::into_inner),
            SessionState::Ended,
        );
        if matches!(state, SessionState::Ended) {
            return;
        }
        let failed = std::sync::Arc::new(failed);
        let closing_failed = failed.clone();
        let ending: Box<dyn FnOnce() + Send> = Box::new(move || {
            match bae_util::task_failure::caught_sync("close a released library", || {
                end(state, closing)
            }) {
                Ok(None | Some(Ok(()))) => {}
                Ok(Some(Err(error))) => closing_failed(&error),
                Err(panicked) => closing_failed(&panicked),
            }
        });
        if tokio::runtime::Handle::try_current().is_err() {
            ending();
            return;
        }
        // The thread is handed the close once it has started, so a close
        // whose thread can't start is still here, not dropped with the
        // spawn's error.
        let (work_tx, work) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let started =
            std::thread::Builder::new().name("bae-library-close".to_string()).spawn(move || {
                if let Ok(ending) = work.recv() {
                    ending();
                }
            });
        let (unstarted, why) = match started {
            Ok(thread) => {
                ending_on(thread);
                match work_tx.send(ending) {
                    Ok(()) => return,
                    Err(unsent) => {
                        (unsent.0, "its thread ended before it was handed the close".to_string())
                    }
                }
            }
            Err(error) => (ending, format!("its thread could not start: {error}")),
        };
        // A runtime thread can neither run the close nor drop the runtime, so
        // the library stays open, owned by nothing, until the process exits;
        // opening it again finds its store held and says so.
        failed(&format!("the library was left open: {why}"));
        std::mem::forget(unstarted);
    }

    /// Record that removing the open library deleted its store.
    fn removed(&self) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *state = match std::mem::replace(&mut *state, SessionState::Ended) {
            SessionState::Open(runtime) => SessionState::Removed(runtime),
            ended @ (SessionState::Removed(_) | SessionState::Ended) => ended,
        };
    }
}

/// See [`Session::end`]. After a removal, `closing` is dropped unrun once the
/// runtime has ended, so what it holds outlives the runtime's tasks.
fn end(
    state: SessionState,
    closing: impl std::future::Future<Output = Closing>,
) -> Option<Closing> {
    let (runtime, closed) = match state {
        SessionState::Open(runtime) => {
            let closed = runtime.block_on(closing);
            (runtime, closed)
        }
        SessionState::Removed(runtime) => {
            drop(runtime);
            drop(closing);
            return Some(Ok(()));
        }
        SessionState::Ended => return None,
    };
    drop(runtime);
    Some(closed)
}

#[cfg(feature = "desktop")]
pub use editing_projection::{bridge_validation_reason_key, shape_release_edit};

impl AppHandle {
    /// Start the handle over an open library. `diagnostics` is where a panic
    /// in the handle's operations, its deliveries to the host, and casting's
    /// background work is reported.
    pub(crate) fn start(
        services: AppServices,
        runtime: tokio::runtime::Runtime,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Result<Self, bae_app::BootstrapError> {
        #[cfg(feature = "desktop")]
        let desktop = {
            let runtime_handle = runtime.handle().clone();
            let starting = services.clone();
            let started = runtime.block_on(bae_util::task_failure::joined(
                "start the desktop services",
                crate::operation_runtime::spawn(runtime.handle(), {
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
                        services.report_bug("close the library a failed open left open", &closing);
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
            diagnostics.clone(),
        );

        Ok(Self {
            services,
            #[cfg(feature = "desktop")]
            desktop,
            #[cfg(feature = "cast")]
            cast,
            runtime: crate::operation_runtime::OperationRuntime::new(
                runtime.handle().clone(),
                diagnostics.clone(),
            ),
            diagnostics,
            session: Session(std::sync::Mutex::new(SessionState::Open(runtime))),
            _host: None,
        })
    }

    pub(crate) fn start_for_host(
        services: AppServices,
        runtime: tokio::runtime::Runtime,
        diagnostics: bae_diagnostics::Diagnostics,
        host: std::sync::Arc<crate::BridgeHost>,
    ) -> Result<Self, bae_app::BootstrapError> {
        let mut app = Self::start(services, runtime, diagnostics)?;
        app._host = Some(host);
        Ok(app)
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
        #[cfg(feature = "desktop")]
        let desktop = self.desktop.clone();
        let host = self._host.clone();
        async move {
            #[cfg(feature = "desktop")]
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
        let services = self.services.clone();
        self.session.end_released(
            self.closing(crate::screens::ClosingSteps::unread()),
            move |error: &dyn std::fmt::Display| {
                services.report_bug("close the library its handle released", error)
            },
            |thread| {
                if let Some(host) = &self._host {
                    host.library_closing(self.services.store_id(), thread);
                }
            },
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
        self.session.end(self.closing(crate::screens::ClosingSteps::of(closing))).ok_or_else(
            || {
                BridgeError::from(bae_library::LibraryError::Internal(
                    "the library is already closed".to_string(),
                ))
            },
        )??;
        info!("Closed library");
        Ok(())
    }
}

#[cfg(test)]
#[path = "handle_tests.rs"]
mod tests;

#[cfg(test)]
mod session_tests;

mod personal;

pub use personal::*;

mod membership;
pub use membership::*;

mod removal;
pub use removal::*;

mod admin;
