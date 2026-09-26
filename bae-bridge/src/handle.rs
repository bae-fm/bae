use bae_core::library::AppServices;
use bae_core::playback::QueueEntryId;
use tracing::info;

#[cfg(feature = "oauth-providers")]
use crate::types::BridgeCloudProvider;
#[cfg(feature = "desktop")]
use crate::types::BridgeDiscogsSaveOutcome;
#[cfg(any(feature = "cloudkit", feature = "oauth-providers"))]
use crate::types::BridgeHomeStorage;
use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtistDetail,
    BridgeArtistSearchResult, BridgeArtistSortCriterion, BridgeArtistSummary, BridgeCatalog,
    BridgeCloudHomeKeyState, BridgeComposerDetail, BridgeComposerSortCriterion,
    BridgeComposerSummary, BridgeComposerWorkGroup, BridgeConfig, BridgeCoverSelection,
    BridgeError, BridgeFile, BridgeGalleryItem, BridgeGallerySource,
    BridgeMakeReleasesRemoteOutcome, BridgePairingDevice, BridgePlaybackValues,
    BridgePreviewTarget, BridgeQueueSnapshot, BridgeRelease, BridgeReleaseRoleSummary,
    BridgeReleaseSummary, BridgeRepeatMode, BridgeSaveSyncConfig, BridgeSearchResults,
    BridgeSortCriterion, BridgeStorageFilter, BridgeStorageRow, BridgeStorageSort,
    BridgeSyncStatusSnapshot, BridgeTrack, BridgeTrackGroup, BridgeTrackRoleSummary,
    BridgeTrackSearchResult, BridgeWorkDetail, BridgeWorkReleaseSummary, BridgeWorkSummary,
    BridgeWorkTrackSummary,
};
#[cfg(feature = "desktop")]
use crate::types::{BridgeMcpServerStatus, BridgeStorageMode, BridgeSubsonicServerStatus};

#[derive(uniffi::Object)]
pub struct AppHandle {
    services: AppServices,
    ui_event_bus: bae_core::ui::UiEventBus,
    #[cfg(feature = "desktop")]
    desktop: bae_desktop::DesktopServices,
    #[cfg(feature = "cast")]
    cast: std::sync::Arc<bae_cast::CastController>,
    runtime: tokio::runtime::Handle,
    /// Last so every retained service is dropped while its tasks can still run.
    owned_runtime: OwnedRuntime,
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
mod base;
mod cloud_operations;
mod collection_subscription;
mod configuration;
mod detail;
mod device_pairing;
mod library_search;
#[cfg(feature = "desktop")]
pub use collection_subscription::{
    AlbumBrowseSubscription, ArtistBrowseSubscription, BridgeAlbumBrowseSnapshot,
    BridgeAlbumBrowseWindow, BridgeArtistBrowseSnapshot, BridgeArtistBrowseWindow,
    BridgeComposerBrowseSnapshot, BridgeComposerBrowseWindow, ComposerBrowseSubscription,
};
pub use device_pairing::BridgeDevicePairingSession;
pub use library_search::{BridgeLibrarySearchSnapshot, LibrarySearchSubscription};
#[cfg(any(feature = "cast", not(any(target_os = "ios", target_os = "android"))))]
mod desktop;
#[cfg(feature = "desktop")]
mod desktop_services;
#[cfg(feature = "desktop")]
mod editing_projection;
#[cfg(feature = "desktop")]
mod import_list;
#[cfg(feature = "desktop")]
mod release_grouping;
#[cfg(feature = "desktop")]
pub use import_list::ImportListSubscription;
#[cfg(feature = "desktop")]
mod import_projection;
mod library_projection;
#[cfg(feature = "desktop")]
mod library_status;
mod live;
mod playback_persistence;
mod queue_projection;
mod queue_upcoming;
mod service_status;
mod storage_browse;
mod sync_status;
mod ui_events;
use queue_projection::pump_ui_events;
use ui_events::convert_ui_event;

/// Owns the runtime every task of an open library runs on.
/// [`AppHandle::close_library`] runs its last work on it and then drops it,
/// waiting for every task on it to end; otherwise it shuts down in the
/// background when the handle drops.
struct OwnedRuntime(std::sync::Mutex<Option<tokio::runtime::Runtime>>);

impl OwnedRuntime {
    /// Run `last` to completion, then shut the runtime down and wait for every
    /// task on it to end. `None` when the runtime was already closed.
    fn close<F: std::future::Future>(&self, last: F) -> Option<F::Output> {
        let runtime = self.0.lock().expect("app runtime lock").take()?;
        let output = runtime.block_on(last);
        drop(runtime);
        Some(output)
    }
}

impl Drop for OwnedRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.lock().expect("app runtime lock").take() {
            runtime.shutdown_background();
        }
    }
}

#[cfg(feature = "desktop")]
pub use editing_projection::{bridge_validation_reason_key, shape_release_edit};

impl AppHandle {
    pub(crate) fn start(
        services: AppServices,
        ui_event_bus: bae_core::ui::UiEventBus,
        runtime: tokio::runtime::Runtime,
    ) -> Result<Self, bae_core::app::BootstrapError> {
        #[cfg(feature = "desktop")]
        let desktop = {
            let runtime_handle = runtime.handle().clone();
            let services = services.clone();
            runtime
                .block_on(crate::operation_runtime::spawn(
                    runtime_handle.clone(),
                    move || bae_desktop::DesktopServices::start(services, runtime_handle),
                ))
                .map_err(|error| {
                    bae_core::app::BootstrapError::Internal(format!(
                        "desktop services failed to start: {error}"
                    ))
                })?
        };
        #[cfg(feature = "cast")]
        let cast = bae_cast::CastController::start(
            services.clone(),
            runtime.handle().clone(),
            bae_core::renderer::RendererDiscovery::for_host(),
        );

        Ok(Self {
            services,
            ui_event_bus,
            #[cfg(feature = "desktop")]
            desktop,
            #[cfg(feature = "cast")]
            cast,
            runtime: runtime.handle().clone(),
            owned_runtime: OwnedRuntime(std::sync::Mutex::new(Some(runtime))),
        })
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
}

#[uniffi::export]
impl AppHandle {
    /// Close the library so nothing of this process holds its files: stop the
    /// desktop services, playback, identification and import, close the
    /// store, then shut the app runtime down, waiting for every task on it to
    /// end. The caller may then remove the library through
    /// `BridgeHost::remove_local_library`, which refuses while anything still
    /// holds the store.
    ///
    /// Blocks until done, so it is called off the UI thread and never from the
    /// app runtime. Every later call on this handle fails.
    pub fn close_library(&self) -> Result<(), BridgeError> {
        self.owned_runtime
            .close(async {
                #[cfg(feature = "desktop")]
                self.desktop.shutdown().await;
                self.services.close().await;
            })
            .ok_or_else(|| {
                BridgeError::from(bae_core::library::LibraryError::Internal(
                    "the library is already closed".to_string(),
                ))
            })?;
        info!("Closed library");
        Ok(())
    }
}

#[cfg(test)]
#[path = "handle_tests.rs"]
mod tests;
