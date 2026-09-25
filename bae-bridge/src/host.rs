//! What the host app registers once per process, as one object.
//!
//! The host builds a `BridgeHost` right after `configure_diagnostics`, holds it
//! for the whole run, and passes it to every call that reads one of its
//! registrations: bae's directory, the OAuth applications, the CloudKit driver,
//! the in-flight OAuth sign-in, and the runtime the onboarding calls run on.

use std::sync::{Arc, Mutex};

use crate::app_dir::BridgeAppDir;
use crate::init::BridgeDiagnostics;
use crate::setup::{
    join_error_to_bridge, on_worker, parse_oauth_tokens, restore_from_code_config,
    JoinDevicePairingOperation, RestoreFromCodeOperation,
};
#[cfg(feature = "oauth-providers")]
use crate::types::BridgeCloudProvider;
use crate::types::{BridgeError, BridgeLibrary, BridgePendingDevicePairingJoin};

/// Nothing panics while one of the host's registrations is locked, so a
/// poisoned lock is a bug and fails loudly.
const HOST_LOCK: &str = "a host registration lock is never held across a panic";

#[derive(uniffi::Object)]
pub struct BridgeHost {
    diagnostics: Arc<BridgeDiagnostics>,
    /// bae's directory: every library this device has registered, the
    /// active-library pointer, and the journals of pending pairing attempts
    /// live under it.
    app_dir: bae_core::config::AppDir,
    /// The runtime the onboarding calls (restore, join, OAuth) and the
    /// telemetry flush run on: they run before any `AppHandle` — and its
    /// runtime — exists. Its workers have 16 MB stacks (like `init`'s), deep
    /// enough for coven's pull descents.
    runtime: tokio::runtime::Runtime,
    /// The OAuth applications every sign-in and every opened store uses. Empty
    /// until `set_oauth_client_creds`, which is the correct set for an S3,
    /// CloudKit, or local-only library.
    oauth_clients: Mutex<coven::OAuthClients>,
    #[cfg(feature = "cloudkit")]
    cloudkit_driver: Mutex<Option<Arc<dyn crate::cloudkit::CloudKitDriver>>>,
    /// The cancel sender for the one in-flight desktop OAuth flow.
    /// `oauth_cancel` flips it; the matching receiver rides into coven's
    /// `authorize`, which watches it alongside the callback wait and tears the
    /// listener down.
    #[cfg(feature = "oauth-providers")]
    oauth_cancel: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// The pending host-driven OAuth requests, keyed by the request id
    /// `oauth_begin` hands out and `oauth_complete` passes back. Each carries
    /// the PKCE verifier and callback state the exchange must present.
    #[cfg(feature = "oauth-providers")]
    oauth_requests: Mutex<std::collections::HashMap<String, coven::AuthorizeRequest>>,
}

impl BridgeHost {
    /// The CloudKit operations over the registered driver, if one was
    /// registered.
    #[cfg(feature = "cloudkit")]
    fn cloudkit_ops(&self) -> Option<Arc<dyn coven::CloudKitOps>> {
        let driver = self.cloudkit_driver.lock().expect(HOST_LOCK).clone()?;
        Some(crate::cloudkit::cloudkit_ops(driver))
    }

    /// No CloudKit driver can be registered without the `cloudkit` feature, so
    /// there are never any CloudKit ops.
    #[cfg(not(feature = "cloudkit"))]
    fn cloudkit_ops(&self) -> Option<Arc<dyn coven::CloudKitOps>> {
        None
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    /// Build the host's registrations around the telemetry sink from
    /// `configure_diagnostics` and bae's directory. Building the onboarding
    /// runtime can fail — the OS can refuse to spawn its worker threads under
    /// thread, file-descriptor, or memory exhaustion — and that failure is
    /// returned for the host to display.
    #[uniffi::constructor]
    pub fn new(
        diagnostics: Arc<BridgeDiagnostics>,
        app_dir: Arc<BridgeAppDir>,
    ) -> Result<Arc<Self>, BridgeError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .map_err(|e| BridgeError::internal(format!("build onboarding runtime: {e}")))?;
        Ok(Arc::new(Self {
            diagnostics,
            app_dir: app_dir.core().clone(),
            runtime,
            oauth_clients: Mutex::new(coven::OAuthClients::empty()),
            #[cfg(feature = "cloudkit")]
            cloudkit_driver: Mutex::new(None),
            #[cfg(feature = "oauth-providers")]
            oauth_cancel: Mutex::new(None),
            #[cfg(feature = "oauth-providers")]
            oauth_requests: Mutex::new(std::collections::HashMap::new()),
        }))
    }

    /// Flush any buffered telemetry now. Hosts call this at exit so the last
    /// events reach Datadog before the process ends.
    pub async fn flush_diagnostics(&self) -> Result<(), BridgeError> {
        let diagnostics = Arc::clone(&self.diagnostics);
        crate::operation_runtime::run(self.runtime.handle().clone(), move || async move {
            diagnostics.flush().await
        })
        .await
    }
}

#[uniffi::export]
impl BridgeHost {
    /// Discover the libraries registered under bae's directory — the ones
    /// created on this device or restored from another of the owner's devices.
    /// Both the welcome flow (no active library yet) and the in-app sidebar /
    /// quick switcher call this.
    pub fn discover_libraries(&self) -> Result<Vec<BridgeLibrary>, BridgeError> {
        bae_core::config::Config::discover_libraries(&self.app_dir)?
            .into_iter()
            .map(BridgeLibrary::from_core_info)
            .collect()
    }

    /// Remove one library's local data without opening its database. Its cloud
    /// copy and restore code, if any, are not changed.
    pub fn remove_local_library(&self, library_id: String) -> Result<(), BridgeError> {
        bae_core::library::remove_local_library(&self.app_dir, &library_id)
            .map_err(BridgeError::from)
    }

    /// Create a new library and establish its device identity. The library
    /// becomes active after the frontend opens it successfully.
    pub fn create_library(&self, name: Option<String>) -> Result<BridgeLibrary, BridgeError> {
        let ids = coven::UuidProvider;
        let config = match name {
            Some(n) => {
                // Trim + non-blank is core policy: parse before creating, so a
                // blank name is rejected the same way a rename's is.
                let name = bae_core::library_name::LibraryName::parse(&n)
                    .map_err(|e| BridgeError::config(e.to_string()))?;
                bae_core::library::create_library(&self.app_dir, name, &ids)
            }
            None => bae_core::library::create_library_default(&self.app_dir, &ids),
        }
        .map_err(BridgeError::from)?;

        BridgeLibrary::from_core(&config)
    }

    /// The pairing attempt retained by coven that can continue without
    /// rescanning the existing device's code.
    pub fn pending_device_pairing_join(
        &self,
    ) -> Result<Option<BridgePendingDevicePairingJoin>, BridgeError> {
        bae_core::library::pending_device_pairing_join(&self.app_dir)
            .map_err(join_error_to_bridge)
            .map(|pending| pending.map(BridgePendingDevicePairingJoin::from_core))
    }

    /// Discard the joining identity and journal for the one pending enrollment.
    pub fn abandon_pending_device_pairing_join(&self) -> Result<(), BridgeError> {
        bae_core::library::abandon_pending_device_pairing_join(&self.app_dir)
            .map_err(join_error_to_bridge)
    }

    /// Restore a library from a restore code string.
    ///
    /// For OAuth providers, the caller must first run `oauth_authorize()` and
    /// pass the token JSON as `oauth_token_json`.
    pub fn restore_from_code(
        &self,
        code: String,
        oauth_token_json: Option<String>,
    ) -> Result<BridgeLibrary, BridgeError> {
        let app_dir = self.app_dir.clone();
        let oauth_clients = self.oauth_clients.lock().expect(HOST_LOCK).clone();
        let cloudkit_ops = self.cloudkit_ops();
        on_worker(self.runtime.handle(), move || async move {
            let oauth_tokens = oauth_token_json
                .map(|json| parse_oauth_tokens(&json))
                .transpose()?;

            let config = restore_from_code_config(
                app_dir,
                code,
                oauth_clients,
                oauth_tokens,
                cloudkit_ops,
                None,
            )
            .await?;

            BridgeLibrary::from_core(&config)
        })
    }

    /// A cancellable restore over the registrations as they are now.
    pub fn restore_from_code_operation(
        &self,
        code: String,
        oauth_token_json: Option<String>,
    ) -> Result<Arc<RestoreFromCodeOperation>, BridgeError> {
        let oauth_tokens = oauth_token_json
            .map(|json| parse_oauth_tokens(&json))
            .transpose()?;
        let oauth_clients = self.oauth_clients.lock().expect(HOST_LOCK).clone();
        Ok(Arc::new(RestoreFromCodeOperation::new(
            self.app_dir.clone(),
            code,
            oauth_clients,
            oauth_tokens,
            self.cloudkit_ops(),
            self.runtime.handle().clone(),
        )))
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    /// Prepare to join a library through the pairing code an existing device
    /// displays, over the registrations as they are now.
    pub async fn join_device_pairing_operation(
        &self,
        pairing_code: String,
        oauth_token_json: Option<String>,
    ) -> Result<Arc<JoinDevicePairingOperation>, BridgeError> {
        let oauth_tokens = oauth_token_json
            .map(|json| parse_oauth_tokens(&json))
            .transpose()?;
        let app_dir = self.app_dir.clone();
        let oauth_clients = self.oauth_clients.lock().expect(HOST_LOCK).clone();
        let cloudkit_ops = self.cloudkit_ops();
        let runtime = self.runtime.handle().clone();
        crate::operation_runtime::run(runtime.clone(), move || async move {
            JoinDevicePairingOperation::prepare(
                &app_dir,
                &pairing_code,
                oauth_clients,
                oauth_tokens,
                cloudkit_ops,
                runtime,
            )
            .await
            .map(Arc::new)
        })
        .await
    }
}

#[cfg(feature = "cloudkit")]
#[uniffi::export]
impl BridgeHost {
    /// Register the CloudKit driver. Call before `init_app` on CloudKit-backed
    /// libraries.
    pub fn set_cloudkit_driver(&self, driver: Box<dyn crate::cloudkit::CloudKitDriver>) {
        *self.cloudkit_driver.lock().expect(HOST_LOCK) = Some(Arc::from(driver));
    }
}

/// One step of the host-driven (mobile) OAuth flow: the URL to open and the
/// opaque request id to pass back to `oauth_complete`.
#[cfg(feature = "oauth-providers")]
#[derive(uniffi::Record)]
pub struct BridgeOAuthRequest {
    pub auth_url: String,
    pub request_id: String,
}

#[cfg(feature = "oauth-providers")]
#[uniffi::export]
impl BridgeHost {
    /// Register the host's OAuth client credentials, keyed by provider name
    /// (`"google_drive"`, `"dropbox"`, `"onedrive"`). Call once at startup,
    /// before any OAuth flow and before opening a library whose cloud home is
    /// one of those providers — a library opens over the set registered at
    /// that moment. `creds_json` is an object of
    /// `{ "<provider>": { "client_id": "...", "client_secret": null } }`. coven
    /// ships no credentials of its own — the consuming app registers its own.
    /// Registering again replaces the previous set.
    pub fn set_oauth_client_creds(&self, creds_json: String) -> Result<(), BridgeError> {
        let parsed: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&creds_json)
                .map_err(|e| BridgeError::config(format!("Invalid OAuth creds JSON: {e}")))?;
        let mut creds = std::collections::HashMap::new();
        for (provider, value) in parsed {
            let core_provider = oauth_provider_by_name(&provider)?;
            let client_id = value
                .get("client_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    BridgeError::config(format!("OAuth creds for {provider} missing client_id"))
                })?
                .to_string();
            let client_secret = value
                .get("client_secret")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            creds.insert(
                core_provider,
                coven::OAuthClientCreds {
                    client_id,
                    client_secret,
                },
            );
        }
        let clients = coven::OAuthClients::new(creds)
            .map_err(|e| BridgeError::config(format!("OAuth client credentials rejected: {e}")))?;
        *self.oauth_clients.lock().expect(HOST_LOCK) = clients;
        Ok(())
    }

    /// Run an OAuth flow for the given provider and return the raw token JSON.
    ///
    /// Spawns a localhost callback server that lives until the browser
    /// redirects back or `oauth_cancel()` is called. Only one flow can run at a
    /// time.
    pub fn oauth_authorize(&self, provider: BridgeCloudProvider) -> Result<String, BridgeError> {
        self.oauth_cancel();

        let (cancel_tx, cancel) = tokio::sync::watch::channel(false);
        *self.oauth_cancel.lock().expect(HOST_LOCK) = Some(cancel_tx);

        let clients = self.oauth_clients.lock().expect(HOST_LOCK).clone();
        let result = on_worker(self.runtime.handle(), move || async move {
            let core_provider = provider.into_core();
            let clock = std::sync::Arc::new(coven::SystemClock);
            let tokens = clients
                .authorize(core_provider, cancel, clock.as_ref())
                .await
                .map_err(|e| BridgeError::config(format!("OAuth authorization failed: {e}")))?;

            serde_json::to_string(&tokens)
                .map_err(|e| BridgeError::internal(format!("Failed to serialize tokens: {e}")))
        });

        *self.oauth_cancel.lock().expect(HOST_LOCK) = None;

        result
    }

    /// Begin a host-driven OAuth flow: build the authorization URL for
    /// `provider`, redirecting to `redirect_uri` (a custom scheme the mobile OS
    /// auth session captures). The host opens `auth_url`, captures the `code`
    /// and `state` from the redirect, and calls `oauth_complete`. Unlike
    /// `oauth_authorize` this binds no localhost port and opens no browser — it
    /// works in the iOS/Android sandbox.
    pub fn oauth_begin(
        &self,
        provider: BridgeCloudProvider,
        redirect_uri: String,
    ) -> Result<BridgeOAuthRequest, BridgeError> {
        let core_provider = provider.into_core();
        let request = self
            .oauth_clients
            .lock()
            .expect(HOST_LOCK)
            .build_authorize_request(core_provider, &redirect_uri)
            .map_err(|e| BridgeError::config(format!("OAuth begin failed: {e}")))?;
        let auth_url = request.auth_url.clone();
        // An opaque handle correlating this begin with its later complete; not a
        // PKCE value (coven holds the real verifier inside `request`).
        let request_id = coven::IdProvider::new_id(&coven::UuidProvider);
        // At most one host-driven exchange is ever pending: a fresh begin
        // supersedes any earlier un-completed one, so a host that restarts the
        // flow without an intervening cancel doesn't strand the prior entry.
        // Hold the lock across the clear and insert so the two are atomic.
        {
            let mut requests = self.oauth_requests.lock().expect(HOST_LOCK);
            requests.clear();
            requests.insert(request_id.clone(), request);
        }
        Ok(BridgeOAuthRequest {
            auth_url,
            request_id,
        })
    }

    /// Complete a host-driven OAuth flow: exchange the captured `code` for
    /// tokens and return the token JSON to pass to `restore_from_code`.
    /// `redirect_uri`, `state`, and `request_id` must match the originating
    /// `oauth_begin`.
    pub fn oauth_complete(
        &self,
        provider: BridgeCloudProvider,
        code: String,
        state: String,
        request_id: String,
        redirect_uri: String,
    ) -> Result<String, BridgeError> {
        let core_provider = provider.into_core();
        let request = self
            .oauth_requests
            .lock()
            .expect(HOST_LOCK)
            .remove(&request_id)
            .ok_or_else(|| {
                BridgeError::config("OAuth request not found or already used".to_string())
            })?;
        let clients = self.oauth_clients.lock().expect(HOST_LOCK).clone();
        let tokens = on_worker(self.runtime.handle(), move || async move {
            let clock = std::sync::Arc::new(coven::SystemClock);
            clients
                .exchange_code(
                    core_provider,
                    &code,
                    Some(&state),
                    &request,
                    &redirect_uri,
                    clock.as_ref(),
                )
                .await
                .map_err(|e| BridgeError::config(format!("OAuth token exchange failed: {e}")))
        })?;
        serde_json::to_string(&tokens)
            .map_err(|e| BridgeError::internal(format!("Failed to serialize tokens: {e}")))
    }

    /// Cancel an in-progress OAuth flow. Signals the callback server to shut
    /// down and frees the port.
    pub fn oauth_cancel(&self) {
        if let Some(tx) = self.oauth_cancel.lock().expect(HOST_LOCK).take() {
            if tx.send(true).is_err() {
                tracing::debug!("OAuth flow already finished before its cancel arrived");
            }
        }
        // Reclaim any pending host-driven exchange from an abandoned
        // `oauth_begin`; a cancel ends every in-progress flow, and a fresh flow
        // starts with a new `oauth_begin`.
        self.oauth_requests.lock().expect(HOST_LOCK).clear();
    }
}

/// The provider a `set_oauth_client_creds` JSON key names. Only the three
/// account-based clouds run an OAuth flow, so any other key is a host mistake
/// worth naming rather than silently dropping.
#[cfg(feature = "oauth-providers")]
fn oauth_provider_by_name(name: &str) -> Result<coven::CloudProvider, BridgeError> {
    match name {
        "google_drive" => Ok(coven::CloudProvider::GoogleDrive),
        "dropbox" => Ok(coven::CloudProvider::Dropbox),
        "onedrive" => Ok(coven::CloudProvider::OneDrive),
        other => Err(BridgeError::config(format!(
            "OAuth creds name a provider that uses no OAuth flow: {other}"
        ))),
    }
}

/// Open the library `library_id` and start its services, over the host's
/// registrations.
///
/// `restore_playback` is the platform's "Restore on launch" preference: `true`
/// restores the saved queue/current track/position at startup, `false` starts
/// with nothing in playback. Platforms without the preference pass `true`
/// (mobile always resumes where playback left off).
///
/// The host carries the telemetry sink built at process start, so "telemetry
/// set up first" cannot be skipped. A bootstrap failure ships
/// `app_start_failed` through it before the error returns.
#[uniffi::export]
pub fn init_app(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    host: Arc<BridgeHost>,
) -> Result<Arc<crate::handle::AppHandle>, BridgeError> {
    // Cloned in its own statement: the guard of a lock taken inside the call's
    // arguments would stay held until the whole bootstrap returns.
    let oauth_clients = host.oauth_clients.lock().expect(HOST_LOCK).clone();
    host.diagnostics.open_app(
        host.app_dir.clone(),
        library_id,
        position_update_interval_ms,
        restore_playback,
        host.cloudkit_ops(),
        oauth_clients,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host over a fresh temp home. The `TempDir` owns bae's directory, so it
    /// must outlive the host.
    fn test_host() -> (Arc<BridgeHost>, tempfile::TempDir) {
        let home = tempfile::TempDir::new().expect("create the test home");
        let app_dir = BridgeAppDir::new(
            home.path()
                .to_str()
                .expect("the temp home path is UTF-8")
                .to_string(),
        );
        let host = BridgeHost::new(BridgeDiagnostics::noop(), app_dir).expect("build the host");
        (host, home)
    }

    #[test]
    fn a_created_library_is_registered_under_the_home_the_host_names() {
        bae_core::config::install_test_keyring();
        let (host, home) = test_host();

        let created = host
            .create_library(Some("Test Library".to_string()))
            .expect("create a library");

        let registered = home.path().join(".bae").join("libraries").join(&created.id);
        assert_eq!(created.path, registered.to_str().expect("UTF-8 path"));
        let discovered = host.discover_libraries().expect("discover libraries");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, created.id);
        assert_eq!(discovered[0].path, created.path);

        host.remove_local_library(created.id.clone())
            .expect("remove the library");
        assert!(!registered.exists());
        assert!(host
            .discover_libraries()
            .expect("discover libraries")
            .is_empty());
    }

    /// Forgetting the open library: close its handle, drop it, and remove it
    /// through the host. Nothing of the closed handle still holds the store, so
    /// the removal takes its directory and every keyring entry for it.
    #[test]
    fn a_closed_library_is_removed_with_its_keyring_entries() {
        bae_core::config::install_test_keyring();
        let (host, home) = test_host();
        let created = host
            .create_library(Some("Test Library".to_string()))
            .expect("create a library");
        let handle = init_app(created.id.clone(), 500, false, host.clone()).expect("open it");
        let keys = coven::StoreKeys::bind(created.id.clone());
        keys.set_host_secret("mcp_bearer_token", "forget-test-secret")
            .expect("store a host secret");

        handle.close_library().expect("close the library");
        drop(handle);
        host.remove_local_library(created.id.clone())
            .expect("remove the closed library");

        let registered = home.path().join(".bae").join("libraries").join(&created.id);
        assert!(!registered.exists());
        assert_eq!(keys.get_host_secret("mcp_bearer_token").unwrap(), None);
        assert!(host
            .discover_libraries()
            .expect("discover libraries")
            .is_empty());
    }

    #[cfg(feature = "oauth-providers")]
    #[test]
    fn host_driven_oauth_flow_holds_at_most_one_pending_exchange() {
        use crate::types::BridgeErrorCategory;

        let (host, _home) = test_host();
        host.set_oauth_client_creds(
            r#"{ "google_drive": { "client_id": "test-client-id", "client_secret": null } }"#
                .to_string(),
        )
        .expect("register test OAuth client creds");

        let redirect_uri = "bae://oauth".to_string();

        // A second begin supersedes the first: at most one pending exchange.
        host.oauth_begin(BridgeCloudProvider::GoogleDrive, redirect_uri.clone())
            .expect("first begin builds an authorize request");
        let second = host
            .oauth_begin(BridgeCloudProvider::GoogleDrive, redirect_uri.clone())
            .expect("second begin builds an authorize request");
        assert_eq!(
            host.oauth_requests.lock().expect(HOST_LOCK).len(),
            1,
            "a fresh begin supersedes the prior un-completed one"
        );

        // Cancel reclaims the pending host-driven exchange.
        host.oauth_cancel();
        assert!(
            host.oauth_requests.lock().expect(HOST_LOCK).is_empty(),
            "cancel clears the pending host-driven exchange"
        );

        // Completing the cancelled flow's request finds nothing to exchange.
        let err = host
            .oauth_complete(
                BridgeCloudProvider::GoogleDrive,
                "auth-code".to_string(),
                "callback-state".to_string(),
                second.request_id,
                redirect_uri,
            )
            .expect_err("a cancelled request has no pending exchange to complete");
        match err {
            BridgeError::Diagnostic { category, detail } => {
                assert_eq!(category, BridgeErrorCategory::Config);
                assert!(detail.contains("not found or already used"));
            }
            other => panic!("expected config bridge error, got {other:?}"),
        }
    }

    #[cfg(feature = "oauth-providers")]
    #[test]
    fn begin_fails_on_a_host_with_no_registered_creds() {
        use crate::types::BridgeErrorCategory;

        let (host, _home) = test_host();

        match host.oauth_begin(BridgeCloudProvider::GoogleDrive, "bae://oauth".to_string()) {
            Err(BridgeError::Diagnostic { category, detail }) => {
                assert_eq!(category, BridgeErrorCategory::Config);
                assert!(detail.contains("OAuth begin failed"));
            }
            Err(other) => panic!("expected config bridge error, got {other:?}"),
            Ok(_) => panic!("a host with no registered OAuth applications cannot begin a flow"),
        }
    }

    #[test]
    fn flush_diagnostics_completes_for_a_caller_on_another_runtime() {
        let (host, _home) = test_host();
        let caller = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build the caller runtime");

        caller
            .block_on(host.flush_diagnostics())
            .expect("the no-op sink flushes");
    }
}
