//! Process registrations and the runtime used before a library is open.
use crate::handle::AppHandle;
use crate::{BridgeAppDir, BridgeDiagnostics, BridgeError, BridgeLibrary};
use bae_app::{bootstrap, BootstrapError};
use bae_diagnostics::{AppStartFailureKind, Diagnostics, DiagnosticsError, TelemetryEvent};
use std::sync::{Arc, Mutex};

const HOST_LOCK: &str = "host registrations lock";

#[derive(uniffi::Object)]
pub struct BridgeHost {
    /// Where the host reports bugs and a failed open: the sink the app built
    /// at launch.
    diagnostics: Diagnostics,
    app_dir: bae_app_dir::AppDir,
    /// This device's libraries as coven keeps them, under `app_dir`.
    coven: coven::Coven,
    /// The wall clock every library this host builds or opens reads, and the
    /// ids that clock times: one pair for the host and each of its libraries.
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    runtime: Option<tokio::runtime::Runtime>,
    /// Where the host's own calls run, on `runtime`.
    operations: crate::operation_runtime::OperationRuntime,
    oauth_clients: Mutex<coven::OAuthClients>,
    #[cfg(not(desktop))]
    presenter: Mutex<Option<Arc<dyn coven::OAuthPresenter>>>,
    /// How this platform shares the speaker, for every library's playback.
    audio_focus: bae_playback::AudioFocus,
    /// The libraries this host has open and the stores still closing, so a
    /// call that names a store reaches its open library, or waits for its
    /// close.
    library_sessions: Arc<LibrarySessions>,
}

impl Drop for BridgeHost {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl BridgeHost {
    pub(crate) fn open_prepared(
        self: &Arc<Self>,
        handle: coven::CovenHandle,
        interval: u32,
        restore: bool,
    ) -> Result<Arc<crate::AppHandle>, BridgeError> {
        self.opened(bae_app::bootstrap_prepared(
            self.app_dir.clone(),
            handle,
            interval,
            restore,
            bae_app::PlaybackOutput::System(self.audio_focus()),
            self.diagnostics.clone(),
            self.clock.clone(),
            self.ids.clone(),
            start_handle(self.clone(), self.diagnostics.clone()),
        ))
    }

    /// Bootstrap the library `library_id` with this host's sink, its open's
    /// steps written into `opening`'s stream. A bootstrap failure ships
    /// `app_start_failed` through it before the error returns.
    pub(crate) fn open_app(
        self: &Arc<Self>,
        library_id: String,
        position_update_interval_ms: u32,
        restore_playback: bool,
        opening: &crate::BridgeLibraryOpening,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bootstrap(
            self.app_dir.clone(),
            library_id,
            position_update_interval_ms,
            restore_playback,
            bae_app::PlaybackOutput::System(self.audio_focus()),
            self.diagnostics.clone(),
            self.clock.clone(),
            self.ids.clone(),
            self.library_builder(),
            opening.report(),
            start_handle(self.clone(), self.diagnostics.clone()),
        ))
    }

    /// [`Self::open_app`] with the fixture a debug UI test left at `fixture`
    /// written into the library before its services start.
    #[cfg(all(debug_assertions, desktop))]
    pub(crate) fn open_app_on_fixture(
        self: &Arc<Self>,
        fixture: std::path::PathBuf,
        library_id: String,
        position_update_interval_ms: u32,
        restore_playback: bool,
        opening: &crate::BridgeLibraryOpening,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        self.opened(bae_app::bootstrap_on_fixture(
            fixture,
            self.app_dir.clone(),
            library_id,
            position_update_interval_ms,
            restore_playback,
            bae_app::PlaybackOutput::System(self.audio_focus()),
            self.diagnostics.clone(),
            self.clock.clone(),
            self.ids.clone(),
            self.library_builder(),
            opening.report(),
            start_handle(self.clone(), self.diagnostics.clone()),
        ))
    }

    /// The handle a bootstrap built, or its failure shipped as
    /// `app_start_failed` and returned.
    fn opened(
        &self,
        bootstrapped: Result<AppHandle, BootstrapError>,
    ) -> Result<Arc<AppHandle>, BridgeError> {
        bootstrapped.map(Arc::new).map_err(|error| {
            self.diagnostics
                .event(TelemetryEvent::AppStartFailed { kind: app_start_failure_kind(&error) });
            bootstrap_error_to_bridge(error)
        })
    }

    /// How this platform shares the speaker, which an opened library's
    /// playback asks before every start.
    pub(crate) fn audio_focus(&self) -> bae_playback::AudioFocus {
        self.audio_focus.clone()
    }

    /// Close `handle`, a store an [`crate::OpenedLibrary`] held and was
    /// released with, on a thread of its own, recorded so a call that opens or
    /// removes the store waits for it. Dropping the handle alone would leave
    /// the store closing on this host's runtime with nothing to wait on, and
    /// a removal that followed would find it still held. A close that fails
    /// is reported as a bug: no caller is left to hear it.
    pub(crate) fn close_released(self: &Arc<Self>, handle: coven::CovenHandle) {
        let store = handle.store_id();
        let host = Arc::clone(self);
        // Moved into the thread, so the close ends with it, or with the
        // thread that could not start.
        let closing = self.library_sessions.closing(store);
        let started = self.diagnostics.spawn_thread_reported(
            "bae-store-close",
            move || {
                let runtime = host
                    .runtime
                    .as_ref()
                    .expect("a host that is still referenced holds its runtime")
                    .handle()
                    .clone();
                if let Err(error) = runtime.block_on(handle.close()) {
                    let error = coven::task::reported(&host.diagnostics, error);
                    host.diagnostics
                        .report_failure("close the store a released library held", &error);
                }
                drop(closing);
            },
            // The close's guard drops as the panic unwinds, ending the close
            // the host's record holds.
            |_| {},
        );
        if let Err(error) = started {
            self.diagnostics.report_bug(
                "close the store a released library held",
                &format!("its thread could not start: {error}"),
            );
        }
    }

    /// Wait until `store` has closed, if its library is closing, before a
    /// call opens, reads or removes the store. Blocks, so it runs off any
    /// async runtime.
    pub(crate) fn wait_for_closing(&self, store: coven::StoreId) {
        self.library_sessions.wait(store);
    }

    /// [`Self::wait_for_closing`] from an async call on the host's runtime.
    pub(crate) async fn closed(self: &Arc<Self>, store: coven::StoreId) -> Result<(), BridgeError> {
        let host = Arc::clone(self);
        self.diagnostics
            .blocking("wait for a library to close", move || host.wait_for_closing(store))
            .await
            .map_err(BridgeError::unfinished)
    }

    /// Finish every recorded removal, on the caller's task, writing how many
    /// are done of how many there are to `steps`.
    async fn finish_removals_now(
        &self,
        steps: &crate::screens::SetupSteps,
    ) -> Result<(), BridgeError> {
        Ok(bae_library::finish_removals(
            &self.app_dir,
            &self.coven,
            &self.diagnostics,
            &|done, total| {
                steps.step(crate::types::BridgeSetupStep::FinishingRemovals, done, total)
            },
        )
        .await?)
    }

    /// Remove `store`, which isn't open, without opening it, on the caller's
    /// task.
    async fn remove_unopenable_library_now(
        &self,
        store: coven::StoreId,
        steps: &crate::screens::ClosingSteps,
    ) -> Result<(), bae_library::LibraryRemovalError> {
        bae_library::remove_unopenable_library(
            &self.app_dir,
            &self.coven,
            &self.diagnostics,
            store,
            &|step| steps.core(step),
        )
        .await
    }

    /// Finish removing `store`, whose removal stopped partway, on the
    /// caller's task.
    async fn finish_deletion_now(
        &self,
        store: coven::StoreId,
        steps: &crate::screens::ClosingSteps,
    ) -> Result<(), bae_library::LibraryRemovalError> {
        bae_library::finish_deletion(
            &self.app_dir,
            &self.coven,
            self.library_builder(),
            &self.diagnostics,
            store,
            &|step| steps.core(step),
        )
        .await
    }

    /// Remove the open library `services` runs, which this host opened, on
    /// the caller's task. A store whose close fails after this device left
    /// it is discarded from this host's installation.
    pub(crate) async fn remove_open_library(
        &self,
        services: &bae_app::AppServices,
        steps: &crate::screens::ClosingSteps,
    ) -> Result<(), bae_library::LibraryRemovalError> {
        services.remove_library(&self.coven, &|step| steps.core(step)).await
    }

    fn library_builder(&self) -> coven::CovenBuilder {
        let builder = bae_store::store_builder(&self.coven, self.clock.clone(), self.ids.clone())
            .oauth_clients(self.oauth_clients.lock().expect(HOST_LOCK).clone());
        #[cfg(desktop)]
        {
            builder.oauth_presenter(Arc::new(coven::DesktopOAuthPresenter))
        }
        #[cfg(not(desktop))]
        {
            match self.presenter.lock().expect(HOST_LOCK).clone() {
                Some(presenter) => builder.oauth_presenter(presenter),
                None => builder,
            }
        }
    }

    fn start(
        diagnostics: Diagnostics,
        app_dir: Arc<BridgeAppDir>,
        coven: coven::Coven,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        audio_focus: bae_playback::AudioFocus,
    ) -> Result<Arc<Self>, BridgeError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .map_err(BridgeError::internal)?;
        let operations = crate::operation_runtime::OperationRuntime::new(
            runtime.handle().clone(),
            diagnostics.clone(),
        );
        Ok(Arc::new(Self {
            diagnostics,
            operations,
            app_dir: app_dir.core().clone(),
            coven,
            clock,
            ids,
            runtime: Some(runtime),
            oauth_clients: Mutex::new(coven::OAuthClients::new(None, None)),
            #[cfg(not(desktop))]
            presenter: Mutex::new(None),
            audio_focus,
            library_sessions: LibrarySessions::new(),
        }))
    }
}

/// A host whose keys and credentials stay in an in-memory keychain for this
/// process: the macOS debug app's UI tests start it so they never touch the
/// login keychain.
#[cfg(all(any(test, feature = "test-keyring"), desktop))]
#[uniffi::export]
impl BridgeHost {
    #[uniffi::constructor]
    pub fn with_test_keyring(
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
    ) -> Result<Arc<Self>, BridgeError> {
        let (clock, ids) = diagnostics.sources();
        let diagnostics = diagnostics.core().clone();
        let coven = coven::Coven::in_memory_reporting_to(
            app_dir.core().coven_app_dir(),
            diagnostics.bug_sink(),
        );
        Self::start(diagnostics, app_dir, coven, clock, ids, bae_playback::AudioFocus::Unshared)
    }
}

/// The desktop's host: any app plays without asking for the speaker.
#[cfg(desktop)]
#[uniffi::export]
impl BridgeHost {
    #[uniffi::constructor]
    pub fn new(
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
    ) -> Result<Arc<Self>, BridgeError> {
        let (clock, ids) = diagnostics.sources();
        let diagnostics = diagnostics.core().clone();
        let coven = installation(&diagnostics, app_dir.core())?;
        Self::start(diagnostics, app_dir, coven, clock, ids, bae_playback::AudioFocus::Unshared)
    }
}

/// A phone's host, holding the system's audio focus (Android) or audio
/// session (iOS), which every library's playback asks before it starts.
#[cfg(not(desktop))]
#[uniffi::export]
impl BridgeHost {
    #[uniffi::constructor]
    pub fn new(
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
        audio_focus: Box<dyn crate::BridgeAudioFocus>,
    ) -> Result<Arc<Self>, BridgeError> {
        let (clock, ids) = diagnostics.sources();
        let diagnostics = diagnostics.core().clone();
        let coven = installation(&diagnostics, app_dir.core())?;
        let audio_focus = crate::PlatformAudioFocus::new(audio_focus, diagnostics.clone());
        Self::start(
            diagnostics,
            app_dir,
            coven,
            clock,
            ids,
            bae_playback::AudioFocus::Shared(Arc::new(audio_focus)),
        )
    }
}

#[uniffi::export]
impl BridgeHost {
    /// The preset a person adds to their list, as core makes it: FLAC, the
    /// default filename pattern and pregap placement, both levels, the cover
    /// embedded, under an id from the host's id source.
    pub fn new_save_preset(&self) -> crate::types::BridgeSavePreset {
        crate::types::BridgeSavePreset::from_core(&bae_preferences::SavePreset::new_flac(
            self.ids.as_ref(),
        ))
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    pub async fn flush_diagnostics(self: Arc<Self>) -> Result<(), BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.flush_diagnostics", move || async move {
                self.diagnostics.flush().await.map_err(diagnostics_error_to_bridge)
            })
            .await
    }

    pub async fn list_libraries(self: Arc<Self>) -> Result<Vec<BridgeLibrary>, BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.list_libraries", move || async move {
                self.app_dir
                    .discover_libraries(&self.coven, &self.diagnostics)
                    .await?
                    .into_iter()
                    .map(BridgeLibrary::from_core_info)
                    .collect()
            })
            .await
    }

    /// A recorded removal that never finished, read from its record so it
    /// outlives the process that started it.
    pub async fn unfinished_removal(
        self: Arc<Self>,
    ) -> Result<Option<crate::BridgeUnfinishedRemoval>, BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.unfinished_removal", move || async move {
                let removal = bae_library::UnfinishedRemoval::find(
                    &self.app_dir,
                    &self.coven,
                    &self.diagnostics,
                )
                .await?;
                Ok(removal.map(|removal| {
                    crate::BridgeUnfinishedRemoval::new(
                        removal.category(),
                        &removal,
                        removal.files_remain(),
                    )
                }))
            })
            .await
    }

    /// What removing the library `library_id`, which isn't open, would lose,
    /// for the person to confirm first. Counting opens the library, and the
    /// open's steps are written into `opening`'s stream.
    pub async fn unsent_before_removal(
        self: Arc<Self>,
        library_id: String,
        opening: Arc<crate::BridgeLibraryOpening>,
    ) -> Result<crate::BridgeUnsentWork, BridgeError> {
        let store = crate::from_app::store(&library_id)?;
        let builder = self.library_builder();
        let report = opening.report();
        let clock = self.clock.clone();
        self.operations
            .clone()
            .run("BridgeHost.unsent_before_removal", move || async move {
                self.closed(store).await?;
                Ok(crate::BridgeUnsentWork::from_core(
                    bae_library::unsent_before_removal(
                        builder,
                        store,
                        report,
                        clock,
                        &self.diagnostics,
                    )
                    .await?,
                ))
            })
            .await
    }

    /// How the library `library_id` stands with its storage: its sync state,
    /// when it last synced and whether it holds unsent changes. A library
    /// this host has open answers from its live sync status; any other is read
    /// as it last stood, without opening it, once a close of it under way has
    /// ended.
    pub async fn library_standing(
        self: Arc<Self>,
        library_id: String,
    ) -> Result<crate::BridgeLibraryStanding, BridgeError> {
        let store = crate::from_app::store(&library_id)?;
        if let Some(standing) = self.library_sessions.open_standing(store).await {
            return standing;
        }
        self.operations
            .clone()
            .run("BridgeHost.library_standing", move || async move {
                self.closed(store).await?;
                Ok(crate::BridgeLibraryStanding::from_core(
                    bae_library::closed_library_standing(&self.coven, store, &self.diagnostics)
                        .await?,
                ))
            })
            .await
    }

    /// Remove the library `library_id`, which can't open, without opening it,
    /// losing whatever this device hadn't sent. Each step the removal runs is
    /// written to `closing`'s stream, when the app shows the removal.
    pub async fn remove_unopenable_library(
        self: Arc<Self>,
        library_id: String,
        closing: Option<Arc<crate::BridgeLibraryClosing>>,
    ) -> Result<crate::BridgeLibraryRemoval, BridgeError> {
        let store = crate::from_app::store(&library_id)?;
        let steps = crate::screens::ClosingFeed::new(closing).steps();
        self.operations
            .clone()
            .run_to_completion(
                self.clone(),
                "remove a library that can't open",
                move || async move {
                    self.closed(store).await?;
                    crate::BridgeLibraryRemoval::from_core(
                        self.remove_unopenable_library_now(store, &steps).await,
                    )
                },
            )
            .await
    }

    /// Finish removing the library `library_id`, whose removal stopped
    /// partway and which never opens again: this device tells the library's
    /// other devices it left, if it hadn't, then its copy is deleted. Each
    /// step is written to `closing`'s stream, when the app shows the removal.
    pub async fn finish_deletion(
        self: Arc<Self>,
        library_id: String,
        closing: Option<Arc<crate::BridgeLibraryClosing>>,
    ) -> Result<crate::BridgeLibraryRemoval, BridgeError> {
        let store = crate::from_app::store(&library_id)?;
        let steps = crate::screens::ClosingFeed::new(closing).steps();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "finish removing a library", move || async move {
                self.closed(store).await?;
                crate::BridgeLibraryRemoval::from_core(
                    self.finish_deletion_now(store, &steps).await,
                )
            })
            .await
    }

    /// Complete every recorded removal, writing how many are done of how
    /// many there are into `setup`'s stream. Safe to retry after any failure.
    pub async fn finish_removals(
        self: Arc<Self>,
        setup: Arc<crate::BridgeLibrarySetup>,
    ) -> Result<(), BridgeError> {
        self.operations
            .clone()
            .run_to_completion(self.clone(), "finish library removals", move || async move {
                let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
                // Uncounted until the removal records are read.
                steps.uncounted(crate::types::BridgeSetupStep::FinishingRemovals);
                self.finish_removals_now(&steps).await
            })
            .await
    }

    /// Create a library, named `name` or a generated name, its one step
    /// written into `setup`'s stream.
    pub async fn create_library(
        self: Arc<Self>,
        name: Option<String>,
        setup: Arc<crate::BridgeLibrarySetup>,
    ) -> Result<Arc<crate::OpenedLibrary>, BridgeError> {
        let builder = self.library_builder();
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        let name = match name {
            Some(name) => crate::from_app::name(&name)?,
            None => bae_library::library_name::generate_library_name(),
        };
        self.operations
            .clone()
            .run_to_completion(self.clone(), "create library", move || async move {
                steps.uncounted(crate::types::BridgeSetupStep::CreatingLibrary);
                let handle = bae_library::create_library(builder, name, &self.diagnostics).await?;
                Ok(Arc::new(crate::OpenedLibrary::new(handle, self)))
            })
            .await
    }
}

#[cfg(feature = "oauth-providers")]
#[uniffi::export]
impl BridgeHost {
    pub fn set_oauth_clients(&self, clients: crate::BridgeOAuthClients) {
        *self.oauth_clients.lock().expect(HOST_LOCK) = coven::OAuthClients::new(
            clients.dropbox_app_key.map(|app_key| coven::DropboxClients::FullDropbox { app_key }),
            clients.onedrive_client_id,
        );
    }
}

#[cfg(all(feature = "oauth-providers", not(desktop)))]
#[uniffi::export]
impl BridgeHost {
    pub fn set_oauth_presenter(
        &self,
        redirects: crate::BridgeOAuthRedirects,
        callback: Box<dyn crate::OAuthPresenterCallback>,
    ) {
        *self.presenter.lock().expect(HOST_LOCK) = Some(Arc::new(
            crate::oauth::NativeOAuthPresenter::new(redirects, callback, self.diagnostics.clone()),
        ));
    }
}

/// Keep SQLite's temporary tables in the app's cache folder, `directory`.
/// The Android app calls it at launch, before the host opens any library.
#[cfg(target_os = "android")]
#[uniffi::export]
pub fn set_temporary_directory(directory: String) -> Result<(), BridgeError> {
    bae_library::set_temporary_directory(std::path::Path::new(&directory))
        .map_err(|error| BridgeError::categorized(error.category(), error))
}

/// Open the library `library_id`, writing the open's steps into `opening`'s
/// stream.
#[uniffi::export]
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    host: Arc<BridgeHost>,
    opening: Arc<crate::BridgeLibraryOpening>,
) -> Result<Arc<crate::AppHandle>, BridgeError> {
    host.wait_for_closing(crate::from_app::store(&library_id)?);
    host.open_app(library_id, position_update_interval_ms, restore_playback, &opening)
}

#[cfg(all(debug_assertions, desktop))]
#[uniffi::export]
pub fn init_app_on_fixture(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    fixture_path: String,
    host: Arc<BridgeHost>,
    opening: Arc<crate::BridgeLibraryOpening>,
) -> Result<Arc<crate::AppHandle>, BridgeError> {
    host.wait_for_closing(crate::from_app::store(&library_id)?);
    host.open_app_on_fixture(
        fixture_path.into(),
        library_id,
        position_update_interval_ms,
        restore_playback,
        &opening,
    )
}

/// This device's libraries as coven keeps them under `app_dir`, their
/// keys in the OS keychain, which building it opens. A keychain coven
/// can't reach ships `keyring_init_failed` through this sink and fails
/// as a keyring error.
fn installation(
    diagnostics: &Diagnostics,
    app_dir: &bae_app_dir::AppDir,
) -> Result<coven::Coven, BridgeError> {
    app_dir.coven(diagnostics.bug_sink()).map_err(|error| {
        diagnostics.event(TelemetryEvent::KeyringInitFailed {});
        BridgeError::Diagnostic {
            category: crate::types::BridgeErrorCategory::Keyring,
            detail: error.to_string(),
        }
    })
}

fn app_start_failure_kind(error: &BootstrapError) -> AppStartFailureKind {
    match error {
        BootstrapError::LibraryNotFound(_) => AppStartFailureKind::LibraryNotFound,
        BootstrapError::Config(_) => AppStartFailureKind::Config,
        BootstrapError::Database(_) | BootstrapError::DeviceResetRequired(_) => {
            AppStartFailureKind::Database
        }
        BootstrapError::Internal(_) => AppStartFailureKind::Internal,
        BootstrapError::KeyringUnavailable => AppStartFailureKind::KeyringUnavailable,
        BootstrapError::KeychainEntitlementMissing(_) => {
            AppStartFailureKind::KeychainEntitlementMissing
        }
        BootstrapError::ReplacementUnfinished(_) => AppStartFailureKind::ReplacementUnfinished,
        BootstrapError::DeletionUnfinished => AppStartFailureKind::DeletionUnfinished,
    }
}

fn diagnostics_error_to_bridge(e: DiagnosticsError) -> BridgeError {
    BridgeError::internal(format!("diagnostics failed: {e}"))
}

/// A bootstrap failure in the class core gave it, with its text as the
/// untranslated detail; a library that isn't there crosses as not found.
fn bootstrap_error_to_bridge(e: BootstrapError) -> BridgeError {
    match e.ui_error() {
        Ok(error) => BridgeError::from_core(error),
        Err(id) => BridgeError::NotFound {
            entity: crate::types::BridgeEntityKind::Library,
            id: id.to_string(),
        },
    }
}

/// What a bootstrap runs once the library is open: record it in `host`'s
/// open libraries and start the handle for `host` over it, reporting a panic
/// in its work through `diagnostics`.
pub(crate) fn start_handle(
    host: Arc<crate::BridgeHost>,
    diagnostics: Diagnostics,
) -> impl FnOnce(
    bae_app::AppServices,
    tokio::runtime::Runtime,
) -> Result<AppHandle, bae_app::BootstrapError>
       + Send
       + 'static {
    move |services, runtime| {
        // The library is in the host's record from here, until its session
        // begins to close; an open that fails here drops it again.
        let record = host.library_sessions.open(
            services.store_id(),
            Arc::new(library_sessions::OpenLibrary::new(
                services.clone(),
                crate::operation_runtime::OperationRuntime::new(
                    runtime.handle().clone(),
                    diagnostics.clone(),
                ),
            )),
        );
        AppHandle::start(
            services,
            runtime,
            diagnostics,
            #[cfg(feature = "cast")]
            host.ids.clone(),
            host,
            record,
        )
    }
}

mod bootstrap;
pub use bootstrap::*;
mod library_sessions;
pub(crate) use library_sessions::{CloseUnderWay, LibrarySessions, OpenCalls, OpenRecord};
// The session tests stand in their own answer for an open library's standing.
#[cfg(test)]
pub(crate) use library_sessions::OpenStanding;
#[cfg(all(feature = "dev-reset", desktop, unix))]
mod dev_reset;
mod steps;

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
