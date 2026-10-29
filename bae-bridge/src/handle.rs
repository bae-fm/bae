use bae_core::library::AppServices;
use bae_core::playback::QueueEntryId;
use tracing::info;

#[cfg(feature = "desktop")]
use crate::types::BridgeDiscogsSaveOutcome;
use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtistDetail,
    BridgeArtistSearchResult, BridgeArtistSortCriterion, BridgeArtistSummary, BridgeCatalog,
    BridgeComposerDetail, BridgeComposerSortCriterion, BridgeComposerSummary,
    BridgeComposerWorkGroup, BridgeConfig, BridgeCoverSelection, BridgeError, BridgeFile,
    BridgeGalleryItem, BridgeGallerySource, BridgePlaybackValues, BridgePreviewTarget,
    BridgeQueueSnapshot, BridgeRelease, BridgeReleaseRoleSummary, BridgeReleaseSummary,
    BridgeRepeatMode, BridgeSaveSyncConfig, BridgeSearchResults, BridgeSortCriterion,
    BridgeStorageFilter, BridgeStorageRow, BridgeStorageRowStatus, BridgeStorageSort,
    BridgeStoreKeyState, BridgeSyncStatusSnapshot, BridgeTrack, BridgeTrackGroup,
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
    runtime: tokio::runtime::Handle,
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
                    self.run_exported(move |$this| async move $body).await
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

mod album_selection;
mod artist_search;
pub use artist_search::{
    ArtistSearchSubscription, BridgeArtistSearchSnapshot, BridgeArtistSearchWindow,
};
mod base;
mod cloud_operations;
mod collection_subscription;
mod configuration;
mod detail;
mod download_browse;
pub use download_browse::{
    BridgeDownloadBrowseSnapshot, BridgeDownloadBrowseWindow, DownloadBrowseSubscription,
};
mod library_search;
#[cfg(feature = "desktop")]
pub use collection_subscription::{
    AlbumBrowseSubscription, ArtistBrowseSubscription, BridgeAlbumBrowseSnapshot,
    BridgeAlbumBrowseWindow, BridgeArtistBrowseSnapshot, BridgeArtistBrowseWindow,
    BridgeComposerBrowseSnapshot, BridgeComposerBrowseWindow, ComposerBrowseSubscription,
};
pub use library_search::{BridgeLibrarySearchSnapshot, LibrarySearchSubscription};
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
pub use import_list::ImportListSubscription;
#[cfg(feature = "desktop")]
pub use release_edit::ReleaseEditSession;
#[cfg(feature = "desktop")]
mod import_projection;
mod library_projection;
#[cfg(feature = "desktop")]
mod library_status;
mod live;
mod playback_persistence;
mod queue_projection;
mod queue_upcoming;
#[cfg(desktop)]
mod release_files;
mod service_status;
mod storage_browse;
#[cfg(desktop)]
mod storage_inspector;
mod sync_status;
mod ui_events;

/// The runtime every task of an open library runs on, and whether the library
/// is still open on it. A library ends before its runtime does: closing the
/// handle, or releasing it, closes an open library on the runtime and only then
/// shuts the runtime down, so nothing of the library is left running for the
/// shutdown to cancel under it.
struct Session(std::sync::Mutex<SessionState>);

enum SessionState {
    /// The library is open on the runtime.
    Open(tokio::runtime::Runtime),
    /// Removing the library deleted its store; only the runtime is left to end.
    Removed(tokio::runtime::Runtime),
    /// The runtime has ended.
    Ended,
}

type Closing = Result<(), bae_core::library::LibraryError>;

impl Session {
    /// Close the open library with `closing` on its runtime, then shut the
    /// runtime down, waiting for every task on it to end. After a removal
    /// deleted the store only the runtime is left to end. `None` when the
    /// session already ended.
    fn end(&self, closing: impl std::future::Future<Output = Closing>) -> Option<Closing> {
        let state = std::mem::replace(
            &mut *self.0.lock().expect("library session lock"),
            SessionState::Ended,
        );
        end(state, closing)
    }

    /// [`Self::end`] for a released handle, with no caller to hear a failure,
    /// which `failed` is told of. Released on a thread of a runtime — the last
    /// call on the handle ending on the app runtime — it can't wait there for
    /// work only a runtime can do, so a thread of its own ends the session,
    /// and `ending_on` is handed the thread to wait on.
    fn end_released(
        &self,
        closing: impl std::future::Future<Output = Closing> + Send + 'static,
        failed: impl FnOnce(bae_core::library::LibraryError) + Send + 'static,
        ending_on: impl FnOnce(std::thread::JoinHandle<()>),
    ) {
        let state = std::mem::replace(
            &mut *self.0.lock().expect("library session lock"),
            SessionState::Ended,
        );
        if matches!(state, SessionState::Ended) {
            return;
        }
        let ending = move || {
            if let Some(Err(error)) = end(state, closing) {
                failed(error);
            }
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            ending_on(
                std::thread::Builder::new()
                    .name("bae-library-close".to_string())
                    .spawn(ending)
                    .expect("spawn the thread that closes a released library"),
            );
        } else {
            ending();
        }
    }

    /// Record that removing the open library deleted its store.
    fn removed(&self) {
        let mut state = self.0.lock().expect("library session lock");
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
    pub(crate) fn start(
        services: AppServices,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, bae_core::app::BootstrapError> {
        #[cfg(feature = "desktop")]
        let desktop = {
            let runtime_handle = runtime.handle().clone();
            let starting = services.clone();
            let started = runtime.block_on(crate::operation_runtime::spawn(
                runtime_handle.clone(),
                move || bae_desktop::DesktopServices::start(starting, runtime_handle),
            ));
            match started {
                Ok(desktop) => std::sync::Arc::new(desktop),
                // The library is open on the runtime, so it closes before the
                // runtime ends.
                Err(error) => {
                    if let Err(closing) = runtime.block_on(services.close()) {
                        services.report_bug("close the library a failed open left open", &closing);
                    }
                    return Err(bae_core::app::BootstrapError::Internal(format!(
                        "desktop services failed to start: {error}"
                    )));
                }
            }
        };
        #[cfg(feature = "cast")]
        let cast = bae_cast::CastController::start(
            services.clone(),
            runtime.handle().clone(),
            bae_cast::renderer::RendererDiscovery::for_host(),
        );

        Ok(Self {
            services,
            #[cfg(feature = "desktop")]
            desktop,
            #[cfg(feature = "cast")]
            cast,
            runtime: runtime.handle().clone(),
            session: Session(std::sync::Mutex::new(SessionState::Open(runtime))),
            _host: None,
        })
    }

    pub(crate) fn start_for_host(
        services: AppServices,
        runtime: tokio::runtime::Runtime,
        host: std::sync::Arc<crate::BridgeHost>,
    ) -> Result<Self, bae_core::app::BootstrapError> {
        let mut app = Self::start(services, runtime)?;
        app._host = Some(host);
        Ok(app)
    }

    async fn run_exported<T, Build, Fut>(
        self: std::sync::Arc<Self>,
        build: Build,
    ) -> Result<T, BridgeError>
    where
        T: Send + 'static,
        Build: FnOnce(std::sync::Arc<Self>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, BridgeError>> + Send + 'static,
    {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || build(self)).await
    }

    /// Closing the library, as its runtime runs it: the desktop services, then
    /// playback, identification, import, sync, the manager's tasks and the
    /// store. It holds the host, whose runtime prepared the library's store,
    /// until the library has closed.
    fn closing(&self) -> impl std::future::Future<Output = Closing> + Send + 'static {
        let services = self.services.clone();
        #[cfg(feature = "desktop")]
        let desktop = self.desktop.clone();
        let host = self._host.clone();
        async move {
            #[cfg(feature = "desktop")]
            desktop.shutdown().await;
            let closed = services.close().await;
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
            self.closing(),
            move |error| services.report_bug("close the library its handle released", &error),
            |thread| {
                if let Some(host) = &self._host {
                    host.library_closing(self.services.get_config().store_id, thread);
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
    /// app runtime. Every later call on this handle fails.
    pub fn close_library(&self) -> Result<(), BridgeError> {
        self.session.end(self.closing()).ok_or_else(|| {
            BridgeError::from(bae_core::library::LibraryError::Internal(
                "the library is already closed".to_string(),
            ))
        })??;
        info!("Closed library");
        Ok(())
    }
}

#[cfg(test)]
#[path = "handle_tests.rs"]
mod tests;

mod personal;

pub use personal::*;

mod membership;
pub use membership::*;

mod removal;
pub use removal::*;

mod admin;
