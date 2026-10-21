//! Process registrations and the runtime used before a library is open.
use crate::{BridgeAppDir, BridgeDiagnostics, BridgeError, BridgeLibrary};
use std::sync::{Arc, Mutex};

const HOST_LOCK: &str = "host registrations lock";

#[derive(uniffi::Object)]
pub struct BridgeHost {
    diagnostics: Arc<BridgeDiagnostics>,
    app_dir: bae_core::config::AppDir,
    keychain: bae_core::library::Keychain,
    runtime: Option<tokio::runtime::Runtime>,
    oauth_clients: Mutex<coven::OAuthClients>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    presenter: Mutex<Option<Arc<dyn coven::OAuthPresenter>>>,
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

    fn builder(&self) -> coven::CovenBuilder {
        let builder = self
            .keychain
            .store_builder(&self.app_dir)
            .oauth_clients(self.oauth_clients.lock().expect(HOST_LOCK).clone());
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            builder.oauth_presenter(Arc::new(coven::DesktopOAuthPresenter))
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
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
        keychain: bae_core::library::Keychain,
    ) -> Result<Arc<Self>, BridgeError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .map_err(BridgeError::internal)?;
        Ok(Arc::new(Self {
            diagnostics,
            app_dir: app_dir.core().clone(),
            keychain,
            runtime: Some(runtime),
            oauth_clients: Mutex::new(coven::OAuthClients::new(
                None,
                None,
                None,
                Arc::new(coven::SystemClock),
            )),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            presenter: Mutex::new(None),
        }))
    }
}

/// A host whose keys and credentials stay in an in-memory keychain for this
/// process: a debug app's UI tests start it so they never touch the login
/// keychain.
#[cfg(any(test, feature = "test-keyring"))]
#[uniffi::export]
impl BridgeHost {
    #[uniffi::constructor]
    pub fn with_test_keyring(
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
    ) -> Result<Arc<Self>, BridgeError> {
        Self::start(
            diagnostics,
            app_dir,
            bae_core::library::Keychain::in_memory(),
        )
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    #[uniffi::constructor]
    pub fn new(
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
    ) -> Result<Arc<Self>, BridgeError> {
        Self::start(diagnostics, app_dir, bae_core::library::Keychain::System)
    }

    pub async fn flush_diagnostics(self: Arc<Self>) -> Result<(), BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move { self.diagnostics.flush().await },
        )
        .await
    }

    pub async fn list_libraries(self: Arc<Self>) -> Result<Vec<BridgeLibrary>, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                bae_core::config::Config::discover_libraries(&self.app_dir)
                    .await?
                    .into_iter()
                    .map(BridgeLibrary::from_core_info)
                    .collect()
            },
        )
        .await
    }

    /// A recorded removal that never finished, read from its record so it
    /// outlives the process that started it.
    pub async fn unfinished_removal(
        self: Arc<Self>,
    ) -> Result<Option<crate::BridgeUnfinishedRemoval>, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                Ok(bae_core::library::UnfinishedRemoval::find(&self.app_dir)
                    .await?
                    .map(|removal| {
                        crate::BridgeUnfinishedRemoval::new(
                            removal.category(),
                            &removal,
                            removal.files_remain(),
                        )
                    }))
            },
        )
        .await
    }

    /// What removing the library `library_id`, which isn't open, would lose,
    /// for the person to confirm first.
    pub async fn unsent_before_removal(
        self: Arc<Self>,
        library_id: String,
    ) -> Result<crate::BridgeUnsentWork, BridgeError> {
        let store = coven::StoreId(library_id.parse().map_err(BridgeError::config)?);
        let builder = self.builder();
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                Ok(crate::BridgeUnsentWork::from_core(
                    bae_core::library::unsent_before_removal(builder, store).await?,
                ))
            },
        )
        .await
    }

    /// Remove the library `library_id`, which can't open, without opening it,
    /// losing whatever this device hadn't sent.
    pub async fn remove_unopenable_library(
        self: Arc<Self>,
        library_id: String,
    ) -> Result<crate::BridgeLibraryRemoval, BridgeError> {
        let store = coven::StoreId(library_id.parse().map_err(BridgeError::config)?);
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "remove a library that can't open",
            move || async move {
                crate::BridgeLibraryRemoval::from_core(
                    bae_core::library::remove_unopenable_library(
                        &self.app_dir,
                        &self.keychain,
                        store,
                    )
                    .await,
                )
            },
        )
        .await
    }

    /// Complete every recorded removal. Safe to retry after any failure.
    pub async fn finish_removals(self: Arc<Self>) -> Result<(), BridgeError> {
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "finish library removals",
            move || async move { Ok(bae_core::library::finish_removals(&self.app_dir).await?) },
        )
        .await
    }

    pub async fn create_library(
        self: Arc<Self>,
        name: Option<String>,
    ) -> Result<Arc<crate::OpenedLibrary>, BridgeError> {
        let builder = self.builder();
        let name = match name {
            Some(name) => {
                bae_core::library_name::LibraryName::parse(&name).map_err(BridgeError::config)?
            }
            None => bae_core::library_name::generate_library_name(),
        };
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "create library",
            move || async move {
                let handle = bae_core::library::create_library(builder, name).await?;
                Ok(Arc::new(crate::OpenedLibrary::new(handle, self)))
            },
        )
        .await
    }
}

#[cfg(feature = "oauth-providers")]
#[uniffi::export]
impl BridgeHost {
    pub fn set_oauth_clients(&self, clients: crate::BridgeOAuthClients) {
        *self.oauth_clients.lock().expect(HOST_LOCK) = coven::OAuthClients::new(
            clients.google_admin_client_id,
            clients
                .dropbox_app_key
                .map(|app_key| coven::DropboxClients::FullDropbox { app_key }),
            clients.onedrive_client_id,
            Arc::new(coven::SystemClock),
        );
    }
}

#[cfg(all(
    feature = "oauth-providers",
    any(target_os = "ios", target_os = "android")
))]
#[uniffi::export]
impl BridgeHost {
    pub fn set_oauth_presenter(
        &self,
        redirects: crate::BridgeOAuthRedirects,
        callback: Box<dyn crate::OAuthPresenterCallback>,
    ) {
        *self.presenter.lock().expect(HOST_LOCK) =
            Some(Arc::new(crate::oauth::NativeOAuthPresenter {
                redirects,
                callback,
            }));
    }
}

#[uniffi::export]
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    host: Arc<BridgeHost>,
) -> Result<Arc<crate::AppHandle>, BridgeError> {
    host.diagnostics.open_app(
        host.app_dir.clone(),
        library_id,
        position_update_interval_ms,
        restore_playback,
        host.builder(),
        host.clone(),
    )
}

#[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
#[uniffi::export]
pub fn init_app_on_fixture(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    fixture_path: String,
    host: Arc<BridgeHost>,
) -> Result<Arc<crate::AppHandle>, BridgeError> {
    host.diagnostics.open_app_on_fixture(
        fixture_path.into(),
        host.app_dir.clone(),
        library_id,
        position_update_interval_ms,
        restore_playback,
        host.builder(),
        host.clone(),
    )
}

mod bootstrap;
pub use bootstrap::*;
#[cfg(all(feature = "dev-reset", unix))]
mod dev_reset;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_opened_store_keeps_its_original_custody() {
        let root = tempfile::tempdir().unwrap();
        let host = BridgeHost::with_test_keyring(
            BridgeDiagnostics::noop(),
            BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
        )
        .unwrap();
        let caller = tokio::runtime::Runtime::new().unwrap();
        let opened = caller
            .block_on(host.clone().create_library(Some("Session Library".into())))
            .unwrap();
        let app = opened.open(50, false).unwrap();
        let id = opened.library_id();
        drop(opened);
        drop(host);
        assert_eq!(app.get_config().library_id, id);
        assert_eq!(
            caller.block_on(app.clone().library_name()).unwrap(),
            "Session Library"
        );
        app.close_library().unwrap();
    }
}
