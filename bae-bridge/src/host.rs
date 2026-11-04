//! Process registrations and the runtime used before a library is open.
use crate::{BridgeAppDir, BridgeDiagnostics, BridgeError, BridgeLibrary};
use std::sync::{Arc, Mutex};

const HOST_LOCK: &str = "host registrations lock";

#[derive(uniffi::Object)]
pub struct BridgeHost {
    diagnostics: Arc<BridgeDiagnostics>,
    app_dir: bae_app_dir::AppDir,
    /// This device's libraries as coven keeps them, under `app_dir`.
    coven: coven::Coven,
    runtime: Option<tokio::runtime::Runtime>,
    /// Where the host's own calls run, on `runtime`.
    operations: crate::operation_runtime::OperationRuntime,
    oauth_clients: Mutex<coven::OAuthClients>,
    #[cfg(not(desktop))]
    presenter: Mutex<Option<Arc<dyn coven::OAuthPresenter>>>,
    /// How this platform shares the speaker, for every library's playback.
    audio_focus: bae_playback::AudioFocus,
    closing_libraries: closing_libraries::ClosingLibraries,
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
        self.diagnostics.open_prepared(
            self.app_dir.clone(),
            handle,
            self.clone(),
            interval,
            restore,
        )
    }

    /// How this platform shares the speaker, which an opened library's
    /// playback asks before every start.
    pub(crate) fn audio_focus(&self) -> bae_playback::AudioFocus {
        self.audio_focus.clone()
    }

    /// Record `closing`, the thread closing `store` after its library's handle
    /// was released on the library's own runtime, or after the
    /// [`crate::OpenedLibrary`] holding it was released unopened.
    pub(crate) fn library_closing(
        &self,
        store: coven::StoreId,
        closing: std::thread::JoinHandle<()>,
    ) {
        self.closing_libraries.add(store, closing);
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
        let started =
            std::thread::Builder::new().name("bae-store-close".to_string()).spawn(move || {
                let runtime = host
                    .runtime
                    .as_ref()
                    .expect("a host that is still referenced holds its runtime")
                    .handle()
                    .clone();
                if let Err(error) = runtime.block_on(handle.close()) {
                    host.diagnostics.report_bug("close the store a released library held", &error);
                }
            });
        match started {
            Ok(thread) => self.library_closing(store, thread),
            Err(error) => self.diagnostics.report_bug(
                "close the store a released library held",
                &format!("its thread could not start: {error}"),
            ),
        }
    }

    /// Wait until `store` has closed, if its library's handle was released on
    /// its runtime, or the [`crate::OpenedLibrary`] holding it was released,
    /// and it is still closing, before a call opens or removes the store. Blocks, so it runs off any async runtime.
    pub(crate) fn wait_for_closing(&self, store: coven::StoreId) {
        self.closing_libraries.wait(store);
    }

    /// [`Self::wait_for_closing`] from an async call on the host's runtime.
    pub(crate) async fn closed(self: &Arc<Self>, store: coven::StoreId) -> Result<(), BridgeError> {
        let host = Arc::clone(self);
        bae_util::task_failure::blocking("wait for a library to close", move || {
            host.wait_for_closing(store)
        })
        .await
        .map_err(|failed| BridgeError::unfinished(self.diagnostics.bug(failed)))
    }

    /// Finish every recorded removal, on the caller's task, writing how many
    /// are done of how many there are to `steps`.
    async fn finish_removals_now(
        &self,
        steps: &crate::screens::SetupSteps,
    ) -> Result<(), BridgeError> {
        Ok(self
            .diagnostics
            .finish_removals(&self.app_dir, &self.coven, &|done, total| {
                steps.step(crate::types::BridgeSetupStep::FinishingRemovals, done, total)
            })
            .await?)
    }

    /// Remove `store`, which isn't open, without opening it, on the caller's
    /// task.
    async fn remove_unopenable_library_now(
        &self,
        store: coven::StoreId,
        steps: &crate::screens::ClosingSteps,
    ) -> Result<(), bae_library::LibraryRemovalError> {
        self.diagnostics
            .remove_unopenable_library(&self.app_dir, &self.coven, store, &|step| steps.core(step))
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

    fn builder(&self) -> coven::CovenBuilder {
        let builder =
            self.coven.builder().oauth_clients(self.oauth_clients.lock().expect(HOST_LOCK).clone());
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
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
        coven: coven::Coven,
        audio_focus: bae_playback::AudioFocus,
    ) -> Result<Arc<Self>, BridgeError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .map_err(BridgeError::internal)?;
        let operations = diagnostics.operations(runtime.handle().clone());
        Ok(Arc::new(Self {
            diagnostics,
            operations,
            app_dir: app_dir.core().clone(),
            coven,
            runtime: Some(runtime),
            oauth_clients: Mutex::new(coven::OAuthClients::new(None, None)),
            #[cfg(not(desktop))]
            presenter: Mutex::new(None),
            audio_focus,
            closing_libraries: Default::default(),
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
        let coven = coven::Coven::in_memory(app_dir.core().coven_app_dir());
        Self::start(diagnostics, app_dir, coven, bae_playback::AudioFocus::Unshared)
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
        let coven = diagnostics.installation(app_dir.core())?;
        Self::start(diagnostics, app_dir, coven, bae_playback::AudioFocus::Unshared)
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
        let coven = diagnostics.installation(app_dir.core())?;
        let audio_focus = crate::PlatformAudioFocus::new(audio_focus, diagnostics.clone());
        Self::start(
            diagnostics,
            app_dir,
            coven,
            bae_playback::AudioFocus::Shared(Arc::new(audio_focus)),
        )
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    pub async fn flush_diagnostics(self: Arc<Self>) -> Result<(), BridgeError> {
        self.operations
            .clone()
            .run(
                "BridgeHost.flush_diagnostics",
                move || async move { self.diagnostics.flush().await },
            )
            .await
    }

    pub async fn list_libraries(self: Arc<Self>) -> Result<Vec<BridgeLibrary>, BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.list_libraries", move || async move {
                self.app_dir
                    .discover_libraries(&self.coven)
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
                let removal =
                    self.diagnostics.unfinished_removal(&self.app_dir, &self.coven).await?;
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
        let builder = self.builder();
        let report = opening.report();
        self.operations
            .clone()
            .run("BridgeHost.unsent_before_removal", move || async move {
                self.closed(store).await?;
                Ok(crate::BridgeUnsentWork::from_core(
                    bae_library::unsent_before_removal(builder, store, report).await?,
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
        let steps = crate::screens::ClosingSteps::of(closing);
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

    /// Complete every recorded removal, writing how many are done of how
    /// many there are into `setup`'s stream. Safe to retry after any failure.
    pub async fn finish_removals(
        self: Arc<Self>,
        setup: Arc<crate::BridgeLibrarySetup>,
    ) -> Result<(), BridgeError> {
        self.operations
            .clone()
            .run_to_completion(self.clone(), "finish library removals", move || async move {
                let steps = setup.writer();
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
        let builder = self.builder();
        let steps = setup.writer();
        let name = match name {
            Some(name) => crate::from_app::name(&name)?,
            None => bae_library::library_name::generate_library_name(),
        };
        self.operations
            .clone()
            .run_to_completion(self.clone(), "create library", move || async move {
                steps.uncounted(crate::types::BridgeSetupStep::CreatingLibrary);
                let handle = bae_library::create_library(builder, name).await?;
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
    host.diagnostics.open_app(
        host.app_dir.clone(),
        library_id,
        position_update_interval_ms,
        restore_playback,
        host.builder(),
        &opening,
        host.clone(),
    )
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
    host.diagnostics.open_app_on_fixture(
        fixture_path.into(),
        host.app_dir.clone(),
        library_id,
        position_update_interval_ms,
        restore_playback,
        host.builder(),
        &opening,
        host.clone(),
    )
}

mod bootstrap;
pub use bootstrap::*;
mod closing_libraries;
#[cfg(all(feature = "dev-reset", feature = "desktop", unix))]
mod dev_reset;
mod steps;

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
