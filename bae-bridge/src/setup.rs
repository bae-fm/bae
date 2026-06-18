//! Pre-AppHandle free functions: library discovery, creation, restore, unlock.
//!
//! These run before an AppHandle exists (they create or configure the library
//! that AppHandle will later open).

fn parse_oauth_tokens(json: &str) -> Result<bae_core::oauth::OAuthTokens, BridgeError> {
    serde_json::from_str(json)
        .map_err(|e| BridgeError::config(format!("Invalid OAuth token JSON: {e}")))
}

/// The cloud providers this build supports, in display order: the credential /
/// native providers (S3, then CloudKit) ahead of the OAuth providers, matching
/// the order the picker used before it became data-driven. S3 is always
/// available; CloudKit and the OAuth providers appear only when their features
/// are compiled in. The UI renders its provider picker from this list instead of
/// hardcoding one, so a baeium (S3-only) build offers only S3.
#[uniffi::export]
pub fn available_cloud_providers() -> Vec<BridgeCloudProvider> {
    #[allow(unused_mut)]
    let mut providers = vec![BridgeCloudProvider::S3];
    #[cfg(feature = "cloudkit")]
    providers.push(BridgeCloudProvider::CloudKit);
    #[cfg(feature = "oauth-providers")]
    providers.extend([
        BridgeCloudProvider::GoogleDrive,
        BridgeCloudProvider::Dropbox,
        BridgeCloudProvider::OneDrive,
    ]);
    providers
}

/// Build a BridgeLibrary from its raw parts. The two call sites that wrap
/// this — `local_library_active` for a freshly-opened Config,
/// `local_library_from_info` for a discovery-scan entry — just differ in
/// where the fields come from.
fn local_library(
    id: String,
    name: String,
    path: std::path::PathBuf,
    cloud_provider: Option<&bae_core::config::CloudProvider>,
    is_active: bool,
) -> BridgeLibrary {
    BridgeLibrary {
        id,
        // Library dirs bae creates are UTF-8 by construction (the dir name is a
        // UUID), and discovery skips any non-UTF-8 dir, so the path is always
        // addressable as a `String` — never lossily mangled.
        path: path
            .to_str()
            .expect("library path is UTF-8 by construction")
            .to_string(),
        name,
        cloud_provider_label: bae_core::config::cloud_provider_label(cloud_provider),
        is_active,
    }
}

/// `BridgeLibrary` for a freshly-created/restored local library Config.
/// Always active (the operation just made it the active one).
fn local_library_active(config: &bae_core::config::Config) -> BridgeLibrary {
    local_library(
        config.library_id.clone(),
        config.library_name.clone(),
        std::path::PathBuf::from(&*config.library_dir),
        config.cloud_home.provider.as_ref(),
        true,
    )
}

/// `BridgeLibrary` for a discovered local library — path and active flag
/// come from the discovery scan.
pub(crate) fn local_library_from_info(info: bae_core::config::LibraryInfo) -> BridgeLibrary {
    local_library(
        info.id,
        info.name,
        info.path,
        info.cloud_provider.as_ref(),
        info.is_active,
    )
}

use std::sync::{Arc, Mutex};

use tracing::info;

use bae_core::config::Config;
use bae_core::library::{CancellationToken, RestoreFromCodeError};

use crate::types::{
    bridge_cloud_provider, bridge_cloud_provider_to_core, BridgeCloudProvider, BridgeError,
    BridgeLibrary, BridgeRestoreCodeInfo, BridgeRestoreSource,
};

#[cfg(feature = "cloudkit")]
use crate::cloudkit::get_cloudkit_ops;

#[cfg(not(feature = "cloudkit"))]
fn get_cloudkit_ops() -> Option<std::sync::Arc<dyn bae_core::storage::cloud::cloudkit::CloudKitOps>>
{
    None
}

fn restore_error_to_bridge(error: RestoreFromCodeError) -> BridgeError {
    match error {
        RestoreFromCodeError::Cancelled => BridgeError::Cancelled,
        RestoreFromCodeError::Restore(error) => {
            BridgeError::internal(format!("Failed to restore library: {error}"))
        }
    }
}

async fn restore_from_code_config(
    code: String,
    oauth_tokens: Option<bae_core::oauth::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn bae_core::storage::cloud::cloudkit::CloudKitOps>>,
    cancel: Option<CancellationToken>,
) -> Result<Config, BridgeError> {
    match cancel {
        Some(cancel) => bae_core::library::restore_from_code_cancellable(
            &code,
            oauth_tokens,
            cloudkit_ops,
            cancel,
            |status| info!("{}", status),
        )
        .await
        .map_err(restore_error_to_bridge),
        None => bae_core::library::restore_from_code(&code, oauth_tokens, cloudkit_ops, |status| {
            info!("{}", status)
        })
        .await
        .map_err(|e| restore_error_to_bridge(RestoreFromCodeError::Restore(e))),
    }
}

/// Holds a handle to the in-progress OAuth runtime plus a cancellation flag.
/// `oauth_cancel()` sends a signal via the oneshot, which the authorize flow
/// selects on alongside the callback wait.
#[cfg(feature = "oauth-providers")]
static OAUTH_CANCEL_TX: Mutex<Option<tokio::sync::watch::Sender<bool>>> = Mutex::new(None);

/// Point bae's data directory at `path/.bae` by exporting `path` as `$HOME`,
/// which is what `dirs::home_dir()` (and thus `bae_dir()` / library discovery /
/// restore / `init_app`) resolves against. Mobile app processes don't get a
/// `$HOME`, so the native app MUST call this once at startup — before
/// `init_keyring`, `discover_libraries`, restore, or `init_app` — passing its
/// private files directory (e.g. Android `Context.filesDir`). Without it those
/// calls fail with "could not determine home directory".
#[cfg(any(target_os = "ios", target_os = "android"))]
#[uniffi::export]
pub fn set_data_dir(path: String) {
    // Called once at process startup before any worker thread reads the
    // environment, so the set is race-free.
    std::env::set_var("HOME", path);
}

/// Point the TLS stack at the OS's certificate-authority store via `SSL_CERT_DIR`
/// (a colon-separated list of directories, honored by `rustls-native-certs`,
/// which both the S3 client and reqwest use). Android exposes its trusted roots
/// as PEM files under these directories but not on the POSIX paths the cert
/// loader probes by default, so native-root loading otherwise finds nothing and
/// every TLS handshake fails. Pointing at the OS store keeps certificate trust —
/// including additions and distrusts — owned and updated by the platform, not
/// the app. The Android caller passes its system cert directories.
#[cfg(target_os = "android")]
#[uniffi::export]
pub fn set_ca_cert_dir(dirs: String) {
    std::env::set_var("SSL_CERT_DIR", dirs);
}

/// Initialize the platform keyring. Call once at app startup, before any bridge
/// function that touches the keyring (e.g. `unlockLibrary`, `initApp`).
/// Safe to call multiple times — re-initializing the keyring just replaces the
/// store.
///
/// coven's synced-table set is no longer registered here: the host hands it to
/// `coven::Database::open` (and to restore) at the point the connection is
/// opened, so there is no separate process-global registration step.
#[uniffi::export]
pub fn init_keyring() {
    bae_core::config::init_keyring();
}

/// Short display label for a cloud provider ("S3-compatible", "iCloud", …).
/// The restore and onboarding flows hold a bare provider (not a full library),
/// so they call this to render its name. `BridgeLibrary.cloud_provider_label`
/// covers the library-row case. Both delegate to the one mapping in bae-core.
#[uniffi::export]
pub fn cloud_provider_label(provider: BridgeCloudProvider) -> String {
    bae_core::config::cloud_provider_label(Some(&bridge_cloud_provider_to_core(provider)))
}

/// Discover local libraries in ~/.bae/libraries/, returning each as a
/// `BridgeLibrary` — the libraries created on this device or restored from
/// another of the owner's devices. Both the welcome flow (no active library
/// yet) and the in-app sidebar / quick switcher call this.
#[uniffi::export]
pub fn discover_libraries() -> Result<Vec<BridgeLibrary>, BridgeError> {
    Ok(Config::discover_libraries()
        .into_iter()
        .map(local_library_from_info)
        .collect())
}

/// Create a new library. The library is set as the active library.
#[uniffi::export]
pub fn create_library(name: Option<String>) -> Result<BridgeLibrary, BridgeError> {
    let ids = std::sync::Arc::new(bae_core::id_provider::UuidProvider);
    let config = match name {
        Some(n) => bae_core::library::create_library(n, ids.as_ref()),
        None => bae_core::library::create_library_default(ids.as_ref()),
    }
    .map_err(|e| BridgeError::config(format!("{e}")))?;

    Ok(local_library_active(&config))
}

/// Run the future built by `make_fut` on a worker of a shared onboarding
/// runtime, blocking the calling thread until it completes.
///
/// The onboarding exports (restore, OAuth) run before any `AppHandle`, so
/// they can't borrow `AppHandle::spawn_on_runtime` (see `handle`); they share
/// this process-wide runtime instead. Its workers have 16 MB stacks (like
/// `init`'s) — deep enough for the AWS-SDK S3 / coven pull descents. `spawn`
/// moves that deep work onto a worker; the foreign caller only `block_on`s the
/// shallow `JoinHandle`, so nothing deep is ever polled on its ~0.5 MB stack.
///
/// This requires the futures to be `Send` + `'static`. They are: coven's pull
/// path carries the database handle as a `Send`-able `SendDbPtr`, so no
/// non-`Send` `*mut sqlite3` is held across the download await.
fn on_worker<T, Fut>(make_fut: impl FnOnce() -> Fut) -> T
where
    Fut: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    let rt = RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .expect("build onboarding runtime")
    });
    rt.block_on(rt.spawn(make_fut()))
        .expect("onboarding worker task panicked")
}

/// Restore a library from cloud storage using the provided encryption key.
#[uniffi::export]
pub fn restore_from_cloud(
    library_id: String,
    encryption_key_hex: String,
    library_name: Option<String>,
    source: BridgeRestoreSource,
) -> Result<BridgeLibrary, BridgeError> {
    use bae_core::sync::restore::RestoreSource;
    let library_name = library_name.unwrap_or_else(bae_core::library_name::generate_library_name);

    on_worker(move || async move {
        let core_source = match source {
            BridgeRestoreSource::S3 {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
            } => RestoreSource::S3 {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
            },
            BridgeRestoreSource::CloudKit => {
                let ops = get_cloudkit_ops()
                    .ok_or_else(|| BridgeError::internal("CloudKit driver not set".to_string()))?;
                RestoreSource::CloudKit { ops }
            }
            BridgeRestoreSource::GoogleDrive {
                folder_id,
                oauth_token_json,
            } => {
                let tokens = parse_oauth_tokens(&oauth_token_json)?;
                RestoreSource::GoogleDrive { folder_id, tokens }
            }
            BridgeRestoreSource::Dropbox {
                folder_path,
                oauth_token_json,
            } => {
                let tokens = parse_oauth_tokens(&oauth_token_json)?;
                RestoreSource::Dropbox {
                    folder_path,
                    tokens,
                }
            }
            BridgeRestoreSource::OneDrive {
                drive_id,
                folder_id,
                oauth_token_json,
            } => {
                let tokens = parse_oauth_tokens(&oauth_token_json)?;
                RestoreSource::OneDrive {
                    drive_id,
                    folder_id,
                    tokens,
                }
            }
        };

        let config = bae_core::library::restore_from_cloud(
            &library_id,
            &encryption_key_hex,
            &library_name,
            core_source,
            |status| info!("{}", status),
        )
        .await
        .map_err(|e| BridgeError::internal(format!("Failed to restore library: {e}")))?;

        Ok(local_library_active(&config))
    })
}

/// Decode a restore code string and return info for UI preview.
#[uniffi::export]
pub fn decode_restore_code(code: String) -> Result<BridgeRestoreCodeInfo, BridgeError> {
    let info = bae_core::sync::restore_code::decode_restore_code_info(&code)
        .map_err(BridgeError::config)?;

    Ok(BridgeRestoreCodeInfo {
        library_id: info.library_id,
        library_name: info.library_name,
        cloud_provider_label: bae_core::config::cloud_provider_label(Some(&info.cloud_provider)),
        cloud_provider: bridge_cloud_provider(&info.cloud_provider),
        needs_oauth: info.needs_oauth,
    })
}

/// Restore a library from a restore code string.
///
/// For OAuth providers, the caller must first run `oauth_authorize()` and pass the
/// token JSON as `oauth_token_json`.
#[uniffi::export]
pub fn restore_from_code(
    code: String,
    oauth_token_json: Option<String>,
) -> Result<BridgeLibrary, BridgeError> {
    on_worker(move || async move {
        let oauth_tokens = oauth_token_json
            .map(|json| parse_oauth_tokens(&json))
            .transpose()?;

        let config = restore_from_code_config(code, oauth_tokens, get_cloudkit_ops(), None).await?;

        Ok(local_library_active(&config))
    })
}

#[derive(uniffi::Object)]
pub struct RestoreFromCodeOperation {
    code: String,
    oauth_tokens: Option<bae_core::oauth::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn bae_core::storage::cloud::cloudkit::CloudKitOps>>,
    cancel: CancellationToken,
    started: Mutex<bool>,
}

#[uniffi::export]
pub fn restore_from_code_operation(
    code: String,
    oauth_token_json: Option<String>,
) -> Result<Arc<RestoreFromCodeOperation>, BridgeError> {
    let oauth_tokens = oauth_token_json
        .map(|json| parse_oauth_tokens(&json))
        .transpose()?;
    Ok(Arc::new(RestoreFromCodeOperation {
        code,
        oauth_tokens,
        cloudkit_ops: get_cloudkit_ops(),
        cancel: CancellationToken::new(),
        started: Mutex::new(false),
    }))
}

#[uniffi::export]
impl RestoreFromCodeOperation {
    pub fn restore(&self) -> Result<BridgeLibrary, BridgeError> {
        {
            let mut started = self.started.lock().expect("restore started mutex poisoned");
            if *started {
                return Err(BridgeError::internal(
                    "restore operation already started".to_string(),
                ));
            }
            *started = true;
        }
        let code = self.code.clone();
        let oauth_tokens = self.oauth_tokens.clone();
        let cloudkit_ops = self.cloudkit_ops.clone();
        let cancel = self.cancel.clone();
        on_worker(move || async move {
            let config =
                restore_from_code_config(code, oauth_tokens, cloudkit_ops, Some(cancel)).await?;

            Ok(local_library_active(&config))
        })
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// Create a new cancel channel, store the sender, return the receiver.
#[cfg(feature = "oauth-providers")]
pub(crate) fn new_oauth_cancel() -> tokio::sync::watch::Receiver<bool> {
    let (tx, rx) = tokio::sync::watch::channel(false);
    *OAUTH_CANCEL_TX
        .lock()
        .expect("OAuth cancel tx mutex poisoned") = Some(tx);
    rx
}

/// Run an OAuth flow for the given provider and return the raw token JSON.
///
/// Spawns a localhost callback server that lives until the browser redirects back
/// or `oauth_cancel()` is called. Only one flow can run at a time.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_authorize(provider: BridgeCloudProvider) -> Result<String, BridgeError> {
    // Cancel any lingering previous flow
    oauth_cancel();

    let cancel = new_oauth_cancel();

    let result = on_worker(move || async move {
        let core_provider = bridge_cloud_provider_to_core(provider);
        let clock = std::sync::Arc::new(bae_core::clock::SystemClock);
        let tokens = bae_core::oauth::authorize_provider(core_provider, cancel, clock.as_ref())
            .await
            .map_err(|e| BridgeError::config(format!("OAuth authorization failed: {e}")))?;

        serde_json::to_string(&tokens)
            .map_err(|e| BridgeError::internal(format!("Failed to serialize tokens: {e}")))
    });

    // Clean up
    *OAUTH_CANCEL_TX
        .lock()
        .expect("OAuth cancel tx mutex poisoned") = None;

    result
}

/// One step of the host-driven (mobile) OAuth flow: the URL to open and the
/// PKCE verifier to pass back to [`oauth_complete`].
#[cfg(feature = "oauth-providers")]
#[derive(uniffi::Record)]
pub struct BridgeOAuthRequest {
    pub auth_url: String,
    pub verifier: String,
}

/// Register the host's OAuth client credentials, keyed by provider name
/// (`"google_drive"`, `"dropbox"`, `"onedrive"`). Call once at startup before
/// any OAuth flow. `creds_json` is an object of
/// `{ "<provider>": { "client_id": "...", "client_secret": null } }`. coven
/// ships no credentials of its own — the consuming app registers its own.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn set_oauth_client_creds(creds_json: String) -> Result<(), BridgeError> {
    let parsed: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&creds_json)
            .map_err(|e| BridgeError::config(format!("Invalid OAuth creds JSON: {e}")))?;
    let mut creds = std::collections::HashMap::new();
    for (provider, value) in parsed {
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
            provider,
            bae_core::oauth::OAuthClientCreds {
                client_id,
                client_secret,
            },
        );
    }
    bae_core::oauth::set_oauth_client_creds(creds);
    Ok(())
}

/// Begin a host-driven OAuth flow: build the authorization URL + PKCE verifier
/// for `provider`, redirecting to `redirect_uri` (a custom scheme the mobile OS
/// auth session captures). The host opens `auth_url`, captures the `code` from
/// the redirect, and calls [`oauth_complete`]. Unlike [`oauth_authorize`] this
/// binds no localhost port and opens no browser — it works in the iOS/Android
/// sandbox.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_begin(
    provider: BridgeCloudProvider,
    redirect_uri: String,
) -> Result<BridgeOAuthRequest, BridgeError> {
    let core_provider = bridge_cloud_provider_to_core(provider);
    let req = bae_core::oauth::build_authorize_request_for_provider(core_provider, &redirect_uri)
        .map_err(|e| BridgeError::config(format!("OAuth begin failed: {e}")))?;
    Ok(BridgeOAuthRequest {
        auth_url: req.auth_url,
        verifier: req.verifier,
    })
}

/// Complete a host-driven OAuth flow: exchange the captured `code` for tokens
/// and return the token JSON to pass to [`restore_from_code`]. `redirect_uri`
/// and `verifier` must match the originating [`oauth_begin`].
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_complete(
    provider: BridgeCloudProvider,
    code: String,
    verifier: String,
    redirect_uri: String,
) -> Result<String, BridgeError> {
    let core_provider = bridge_cloud_provider_to_core(provider);
    let tokens = on_worker(move || async move {
        let clock = std::sync::Arc::new(bae_core::clock::SystemClock);
        bae_core::oauth::exchange_code_for_provider(
            core_provider,
            &code,
            &verifier,
            &redirect_uri,
            clock.as_ref(),
        )
        .await
    })
    .map_err(|e| BridgeError::config(format!("OAuth token exchange failed: {e}")))?;
    serde_json::to_string(&tokens)
        .map_err(|e| BridgeError::internal(format!("Failed to serialize tokens: {e}")))
}

/// Cancel an in-progress OAuth flow. Signals the callback server to shut down
/// and frees the port.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_cancel() {
    if let Some(tx) = OAUTH_CANCEL_TX
        .lock()
        .expect("OAuth cancel tx mutex poisoned")
        .take()
    {
        let _ = tx.send(true);
    }
}

/// Unlock a library by providing the encryption key hex.
/// Validates the key against the stored fingerprint, then saves it to the keyring.
#[uniffi::export]
pub fn unlock_library(library_id: String, key_hex: String) -> Result<(), BridgeError> {
    bae_core::library::unlock_library(&library_id, &key_hex).map_err(BridgeError::config)
}

/// Validate restore form fields for a given cloud provider.
#[uniffi::export]
pub fn validate_restore_config(fields: crate::types::BridgeRestoreFormFields) -> bool {
    fields.is_valid()
}
