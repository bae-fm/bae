//! Pre-AppHandle free functions: library discovery, creation, restore, unlock.
//!
//! These run before an AppHandle exists (they create or configure the library
//! that AppHandle will later open).

fn parse_oauth_tokens(json: &str) -> Result<coven::OAuthTokens, BridgeError> {
    serde_json::from_str(json)
        .map_err(|e| BridgeError::config(format!("Invalid OAuth token JSON: {e}")))
}

/// The cloud providers this build supports, in picker display order: the
/// credential / native providers (S3, then CloudKit) ahead of the OAuth ones. S3
/// is always available; the rest appear only when their features are compiled in,
/// so a baeium (S3-only) build offers only S3. The UI renders its provider picker
/// from this list rather than hardcoding one.
#[uniffi::export]
pub fn available_cloud_providers() -> Vec<BridgeCloudProvider> {
    vec![
        BridgeCloudProvider::S3,
        #[cfg(feature = "cloudkit")]
        BridgeCloudProvider::CloudKit,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::GoogleDrive,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::Dropbox,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::OneDrive,
    ]
}

/// Build a BridgeLibrary from its raw parts. Its two wrappers —
/// `BridgeLibrary::from_core` (a freshly-opened Config) and `from_core_info` (a
/// discovery-scan entry) — differ only in where the fields come from.
fn local_library(
    id: String,
    name: String,
    path: std::path::PathBuf,
    cloud_provider: Option<&bae_core::config::CloudProvider>,
    is_active: bool,
    error: Option<String>,
) -> Result<BridgeLibrary, BridgeError> {
    let path = path
        .to_str()
        .ok_or_else(|| {
            BridgeError::config(format!("Library path is not UTF-8: {}", path.display()))
        })?
        .to_string();

    Ok(BridgeLibrary {
        id,
        path,
        name,
        cloud_provider: cloud_provider.map(BridgeCloudProvider::from_core),
        is_active,
        error,
    })
}

impl BridgeLibrary {
    /// `BridgeLibrary` for a freshly-created/restored local library Config.
    /// Always active (the operation just made it the active one). The `Config`
    /// it reads is coven's (external crate) via `Deref`, so its fields stay
    /// dotted reads rather than an exhaustive destructure.
    fn from_core(config: &bae_core::config::Config) -> Result<Self, BridgeError> {
        local_library(
            config.store_id.clone(),
            config.store_name.clone(),
            config.library_path().to_path_buf(),
            config.cloud_home.provider.as_ref(),
            true,
            // A Config in hand is a config that loaded. There is nothing broken
            // about it, by construction.
            None,
        )
    }

    /// `BridgeLibrary` for a discovered local library — path and active flag
    /// come from the discovery scan.
    pub(crate) fn from_core_info(info: bae_core::config::LibraryInfo) -> Result<Self, BridgeError> {
        let bae_core::config::LibraryInfo {
            id,
            name,
            path,
            is_active,
            cloud_provider,
            error,
        } = info;
        local_library(id, name, path, cloud_provider.as_ref(), is_active, error)
    }
}

use std::sync::{Arc, Mutex};

use tracing::info;

use bae_core::config::Config;
use bae_core::library::{CancellationToken, JoinDevicePairingError, RestoreFromCodeError};

use crate::get_cloudkit_ops;
use crate::types::{
    BridgeCloudProvider, BridgeDevicePairingOffer, BridgeError, BridgeJoiningDeviceJoinProgress,
    BridgeLibrary, BridgeRestoreCodeInfo, JoiningDeviceJoinProgressCallback,
};

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
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
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

/// The cancel sender for the one in-flight OAuth flow. `oauth_cancel` flips it;
/// the matching receiver rides into `coven::authorize_provider`, which watches it
/// alongside the callback wait and tears the listener down.
#[cfg(feature = "oauth-providers")]
static OAUTH_CANCEL_TX: Mutex<Option<tokio::sync::watch::Sender<bool>>> = Mutex::new(None);
/// The pending host-driven OAuth requests, keyed by the request id
/// [`oauth_begin`] hands out and [`oauth_complete`] passes back. Each carries
/// the PKCE verifier and callback state the exchange must present.
#[cfg(feature = "oauth-providers")]
static OAUTH_REQUESTS: std::sync::LazyLock<
    Mutex<std::collections::HashMap<String, coven::AuthorizeRequest>>,
> = std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn lock_bridge_mutex<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Point bae's data directory at `path/.bae` by exporting `path` as `$HOME`,
/// which `dirs::home_dir()` — and so `bae_dir()`, library discovery, restore, and
/// `init_app` — resolves against. Mobile app processes get no `$HOME`, so the
/// native app MUST call this once at startup, before `init_keyring`,
/// `discover_libraries`, restore, or `init_app`, passing its private files
/// directory (e.g. Android `Context.filesDir`). Without it those calls fail with
/// "could not determine home directory".
#[cfg(any(target_os = "ios", target_os = "android"))]
#[uniffi::export]
pub fn set_data_dir(path: String) {
    // Called once at process startup before any worker thread reads the
    // environment, so the set is race-free.
    std::env::set_var("HOME", path);
}

/// Initialize coven's platform keyring. Call once at app startup, after
/// `configure_diagnostics` and before any bridge function that touches the
/// keyring. A failure is returned so the host stops startup and displays it.
#[uniffi::export]
pub fn init_keyring(diagnostics: Arc<crate::init::BridgeDiagnostics>) -> Result<(), BridgeError> {
    diagnostics.init_keyring()
}

/// Install the in-memory keyring used by debug UI-test app processes. The test
/// launches the real application flow without a signed keychain entitlement,
/// so its key custody is injected before library creation and opening.
#[cfg(debug_assertions)]
#[uniffi::export]
pub fn init_test_keyring() {
    bae_core::config::install_test_keyring();
}

/// Discover local libraries in ~/.bae/libraries/, returning each as a
/// `BridgeLibrary` — the libraries created on this device or restored from
/// another of the owner's devices. Both the welcome flow (no active library
/// yet) and the in-app sidebar / quick switcher call this.
#[uniffi::export]
pub fn discover_libraries() -> Result<Vec<BridgeLibrary>, BridgeError> {
    Config::discover_libraries()
        .map_err(BridgeError::config)?
        .into_iter()
        .map(BridgeLibrary::from_core_info)
        .collect()
}

/// Remove one library's local data without opening its database. Its cloud
/// copy and restore code, if any, are not changed.
#[uniffi::export]
pub fn remove_local_library(library_id: String) -> Result<(), BridgeError> {
    bae_core::library::remove_local_library(&library_id).map_err(BridgeError::from)
}

/// Create a new library and establish its device identity. The library becomes
/// active after the frontend opens it successfully.
#[uniffi::export]
pub fn create_library(name: Option<String>) -> Result<BridgeLibrary, BridgeError> {
    let ids = std::sync::Arc::new(coven::UuidProvider);
    let config = match name {
        Some(n) => {
            // Trim + non-blank is core policy: parse before creating, so a blank
            // name is rejected the same way a rename's is.
            let name = bae_core::library_name::LibraryName::parse(&n)
                .map_err(|e| BridgeError::config(e.to_string()))?;
            bae_core::library::create_library(name, ids.as_ref())
        }
        None => bae_core::library::create_library_default(ids.as_ref()),
    }
    .map_err(BridgeError::from)?;

    BridgeLibrary::from_core(&config)
}

/// Run the future built by `make_fut` on a worker of a shared onboarding runtime,
/// blocking the calling thread until it completes.
///
/// The onboarding exports (restore, OAuth) run before any `AppHandle` — and its
/// tokio runtime — exists, so they share this process-wide one. Its workers have
/// 16 MB stacks (like `init`'s), deep enough for coven's pull descents; `spawn`
/// moves the work onto a worker, so the foreign caller only `block_on`s the
/// shallow `JoinHandle` and nothing deep is ever polled on its ~0.5 MB stack.
/// (The AWS-SDK S3 endpoint descent runs on coven's own big-stack S3 runtime
/// regardless of who awaits it.)
///
/// That requires the futures to be `Send` + `'static`. They are: coven's pull
/// path carries the database handle as a `Send`-able `SendDbPtr`, so no
/// non-`Send` `*mut sqlite3` is held across the download await.
/// The shared onboarding runtime, built once on first use. `Runtime::build` is
/// fallible — the OS can refuse to spawn the worker threads under thread, file-
/// descriptor, or memory exhaustion — so a build failure crosses the boundary as
/// a `BridgeError` like every other fallible onboarding step, rather than
/// panicking on the foreign caller's thread.
fn onboarding_runtime() -> Result<&'static tokio::runtime::Runtime, BridgeError> {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    if let Some(rt) = RT.get() {
        return Ok(rt);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .map_err(|e| BridgeError::internal(format!("build onboarding runtime: {e}")))?;
    // A concurrent caller may have won the init race; `get_or_init` keeps the
    // stored runtime and drops ours. Harmless — the loser's runtime was never
    // entered.
    Ok(RT.get_or_init(|| rt))
}

pub(crate) fn onboarding_runtime_handle() -> Result<tokio::runtime::Handle, BridgeError> {
    Ok(onboarding_runtime()?.handle().clone())
}

fn on_worker<T, Fut>(make_fut: impl FnOnce() -> Fut + Send + 'static) -> Result<T, BridgeError>
where
    Fut: std::future::Future<Output = Result<T, BridgeError>> + Send + 'static,
    T: Send + 'static,
{
    let rt = onboarding_runtime()?;
    match rt.block_on(crate::operation_runtime::spawn(
        rt.handle().clone(),
        make_fut,
    )) {
        Ok(result) => result,
        Err(join_err) => Err(BridgeError::internal(format!(
            "onboarding worker task panicked: {join_err}"
        ))),
    }
}

/// Decode a restore code string and return info for UI preview.
///
/// coven's `RestoreCodeInfo` is not part of its curated public API, so the
/// decoded value is consumed structurally here at its one call site rather
/// than named in a conversion signature. Its fields stay dotted reads.
#[uniffi::export]
pub fn decode_restore_code(code: String) -> Result<BridgeRestoreCodeInfo, BridgeError> {
    let info = bae_core::sync::decode_restore_code_info(&code).map_err(BridgeError::config)?;

    Ok(BridgeRestoreCodeInfo {
        library_id: info.store_id,
        library_name: info.store_name,
        cloud_provider: BridgeCloudProvider::from_core(&info.cloud_provider),
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

        BridgeLibrary::from_core(&config)
    })
}

#[derive(uniffi::Object)]
pub struct RestoreFromCodeOperation {
    code: String,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
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
            let mut started = lock_bridge_mutex(&self.started);
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

            BridgeLibrary::from_core(&config)
        })
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

// =============================================================================
// Membership: joining a library and managing devices
// =============================================================================

/// Decode the one pairing code displayed by an existing device.
#[uniffi::export]
pub fn decode_device_pairing_offer(code: String) -> Result<BridgeDevicePairingOffer, BridgeError> {
    let info =
        bae_core::library::inspect_device_pairing_offer(&code).map_err(BridgeError::config)?;
    Ok(BridgeDevicePairingOffer::from_core(info))
}

impl BridgeDevicePairingOffer {
    fn from_core(info: bae_core::library::DevicePairingOfferInfo) -> Self {
        let bae_core::library::DevicePairingOfferInfo {
            library_name,
            cloud_provider,
            needs_oauth,
            expires_at_unix_seconds,
        } = info;
        BridgeDevicePairingOffer {
            library_name,
            cloud_provider: BridgeCloudProvider::from_core(&cloud_provider),
            needs_oauth,
            expires_at_unix_seconds,
        }
    }
}

fn join_error_to_bridge(error: JoinDevicePairingError) -> BridgeError {
    use crate::types::BridgeErrorCategory;
    match error {
        JoinDevicePairingError::Cancelled => BridgeError::Cancelled,
        // Both devices must be running the join at once. These two are the
        // "nothing is wrong with the code, the other side isn't there" ends, kept
        // apart from a genuine failure so the UI can tell the user what to do.
        JoinDevicePairingError::OwnerOffline => BridgeError::diagnostic(
            BridgeErrorCategory::Membership,
            "the existing device is not accepting this pairing — open bae on it and show a pairing code",
        ),
        JoinDevicePairingError::Abandoned => BridgeError::diagnostic(
            BridgeErrorCategory::Membership,
            "the existing device ended this pairing",
        ),
        JoinDevicePairingError::Join(error) => {
            BridgeError::internal(format!("Failed to join library: {error}"))
        }
    }
}

async fn join_device_pairing_config(
    prepared: bae_core::library::PreparedDevicePairingJoin,
    cancel: CancellationToken,
    progress: Arc<dyn JoiningDeviceJoinProgressCallback>,
) -> Result<Config, BridgeError> {
    let on_progress: coven::JoiningDeviceJoinProgressObserver = Arc::new(move |value| {
        progress.on_progress(BridgeJoiningDeviceJoinProgress::from_core(value));
    });
    bae_core::library::join_prepared_device_pairing_cancellable(prepared, cancel, on_progress)
        .await
        .map_err(join_error_to_bridge)
}

#[derive(uniffi::Object)]
pub struct JoinDevicePairingOperation {
    fingerprint: String,
    state: Mutex<JoinDevicePairingOperationState>,
    cancel: CancellationToken,
}

enum JoinDevicePairingOperationState {
    Prepared(bae_core::library::PreparedDevicePairingJoin),
    Started,
    Abandoned,
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn join_device_pairing_operation(
    pairing_code: String,
    oauth_token_json: Option<String>,
) -> Result<Arc<JoinDevicePairingOperation>, BridgeError> {
    let oauth_tokens = oauth_token_json
        .map(|json| parse_oauth_tokens(&json))
        .transpose()?;
    let prepared = bae_core::library::prepare_device_pairing_join(
        &pairing_code,
        oauth_tokens,
        get_cloudkit_ops(),
    )
    .await
    .map_err(join_error_to_bridge)?;
    let fingerprint = prepared.fingerprint();
    Ok(Arc::new(JoinDevicePairingOperation {
        fingerprint,
        state: Mutex::new(JoinDevicePairingOperationState::Prepared(prepared)),
        cancel: CancellationToken::new(),
    }))
}

#[uniffi::export]
impl JoinDevicePairingOperation {
    pub fn fingerprint(&self) -> String {
        self.fingerprint.clone()
    }

    pub fn join(
        &self,
        progress: Box<dyn JoiningDeviceJoinProgressCallback>,
    ) -> Result<BridgeLibrary, BridgeError> {
        let prepared = {
            let mut state = lock_bridge_mutex(&self.state);
            match std::mem::replace(&mut *state, JoinDevicePairingOperationState::Started) {
                JoinDevicePairingOperationState::Prepared(prepared) => prepared,
                previous @ JoinDevicePairingOperationState::Started => {
                    *state = previous;
                    return Err(BridgeError::internal(
                        "join operation already started".to_string(),
                    ));
                }
                previous @ JoinDevicePairingOperationState::Abandoned => {
                    *state = previous;
                    return Err(BridgeError::Cancelled);
                }
            }
        };
        let cancel = self.cancel.clone();
        let progress = Arc::from(progress);
        on_worker(move || async move {
            let config = join_device_pairing_config(prepared, cancel, progress).await?;

            BridgeLibrary::from_core(&config)
        })
    }

    pub fn cancel(&self) -> Result<(), BridgeError> {
        let mut state = lock_bridge_mutex(&self.state);
        match &*state {
            JoinDevicePairingOperationState::Prepared(prepared) => {
                prepared.abandon().map_err(join_error_to_bridge)?;
                *state = JoinDevicePairingOperationState::Abandoned;
            }
            JoinDevicePairingOperationState::Started => self.cancel.cancel(),
            JoinDevicePairingOperationState::Abandoned => {}
        }
        Ok(())
    }
}

#[cfg(feature = "oauth-providers")]
pub(crate) fn new_oauth_cancel() -> tokio::sync::watch::Receiver<bool> {
    let (tx, rx) = tokio::sync::watch::channel(false);
    *lock_bridge_mutex(&OAUTH_CANCEL_TX) = Some(tx);
    rx
}

/// Run an OAuth flow for the given provider and return the raw token JSON.
///
/// Spawns a localhost callback server that lives until the browser redirects back
/// or `oauth_cancel()` is called. Only one flow can run at a time.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_authorize(provider: BridgeCloudProvider) -> Result<String, BridgeError> {
    oauth_cancel();

    let cancel = new_oauth_cancel();

    let result = on_worker(move || async move {
        let core_provider = provider.into_core();
        let clock = std::sync::Arc::new(coven::SystemClock);
        let tokens = bae_core::oauth::clients()
            .authorize(core_provider, cancel, clock.as_ref())
            .await
            .map_err(|e| BridgeError::config(format!("OAuth authorization failed: {e}")))?;

        serde_json::to_string(&tokens)
            .map_err(|e| BridgeError::internal(format!("Failed to serialize tokens: {e}")))
    });

    *lock_bridge_mutex(&OAUTH_CANCEL_TX) = None;

    result
}

/// One step of the host-driven (mobile) OAuth flow: the URL to open and the
/// opaque request id to pass back to [`oauth_complete`].
#[cfg(feature = "oauth-providers")]
#[derive(uniffi::Record)]
pub struct BridgeOAuthRequest {
    pub auth_url: String,
    pub request_id: String,
}

/// Register the host's OAuth client credentials, keyed by provider name
/// (`"google_drive"`, `"dropbox"`, `"onedrive"`). Call once at startup, before
/// any OAuth flow and before opening a library whose cloud home is one of those
/// providers — a library opens over the set registered at that moment.
/// `creds_json` is an object of
/// `{ "<provider>": { "client_id": "...", "client_secret": null } }`. coven
/// ships no credentials of its own — the consuming app registers its own.
/// Registering again replaces the previous set.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn set_oauth_client_creds(creds_json: String) -> Result<(), BridgeError> {
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
    bae_core::oauth::set_client_creds(creds)
        .map_err(|e| BridgeError::config(format!("OAuth client credentials rejected: {e}")))?;
    Ok(())
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

/// Begin a host-driven OAuth flow: build the authorization URL for `provider`,
/// redirecting to `redirect_uri` (a custom scheme the mobile OS auth session
/// captures). The host opens `auth_url`, captures the `code` and `state` from
/// the redirect, and calls [`oauth_complete`]. Unlike [`oauth_authorize`] this
/// binds no localhost port and opens no browser — it works in the iOS/Android
/// sandbox.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_begin(
    provider: BridgeCloudProvider,
    redirect_uri: String,
) -> Result<BridgeOAuthRequest, BridgeError> {
    let core_provider = provider.into_core();
    let request = bae_core::oauth::clients()
        .build_authorize_request(core_provider, &redirect_uri)
        .map_err(|e| BridgeError::config(format!("OAuth begin failed: {e}")))?;
    let auth_url = request.auth_url.clone();
    // An opaque handle correlating this begin with its later complete; not a
    // PKCE value (coven holds the real verifier inside `request`).
    let request_id = coven::IdProvider::new_id(&coven::UuidProvider);
    // At most one host-driven exchange is ever pending: a fresh begin supersedes
    // any earlier un-completed one, so a host that restarts the flow without an
    // intervening cancel doesn't strand the prior entry. Hold the lock across the
    // clear and insert so the two are atomic.
    {
        let mut requests = lock_bridge_mutex(&OAUTH_REQUESTS);
        requests.clear();
        requests.insert(request_id.clone(), request);
    }
    Ok(BridgeOAuthRequest {
        auth_url,
        request_id,
    })
}

/// Complete a host-driven OAuth flow: exchange the captured `code` for tokens
/// and return the token JSON to pass to [`restore_from_code`]. `redirect_uri`,
/// `state`, and `request_id` must match the originating [`oauth_begin`].
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_complete(
    provider: BridgeCloudProvider,
    code: String,
    state: String,
    request_id: String,
    redirect_uri: String,
) -> Result<String, BridgeError> {
    let core_provider = provider.into_core();
    let request = lock_bridge_mutex(&OAUTH_REQUESTS)
        .remove(&request_id)
        .ok_or_else(|| {
            BridgeError::config("OAuth request not found or already used".to_string())
        })?;
    let tokens = on_worker(move || async move {
        let clock = std::sync::Arc::new(coven::SystemClock);
        bae_core::oauth::clients()
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

/// Cancel an in-progress OAuth flow. Signals the callback server to shut down
/// and frees the port.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn oauth_cancel() {
    if let Some(tx) = lock_bridge_mutex(&OAUTH_CANCEL_TX).take() {
        let _ = tx.send(true);
    }
    // Reclaim any pending host-driven exchange from an abandoned `oauth_begin`; a
    // cancel ends every in-progress flow, and a fresh flow starts with a new
    // `oauth_begin`.
    lock_bridge_mutex(&OAUTH_REQUESTS).clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BridgeErrorCategory;

    #[cfg(unix)]
    #[test]
    fn local_library_from_info_rejects_non_utf8_path() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let info = bae_core::config::LibraryInfo {
            id: "library-id".to_string(),
            name: "Library Name".to_string(),
            path: std::path::PathBuf::from(OsString::from_vec(vec![0xff])),
            is_active: true,
            cloud_provider: None,
            error: None,
        };

        let error = BridgeLibrary::from_core_info(info).expect_err("non-UTF-8 path should fail");
        match error {
            BridgeError::Diagnostic { category, detail } => {
                assert_eq!(category, BridgeErrorCategory::Config);
                assert!(detail.contains("Library path is not UTF-8"));
            }
            other => panic!("expected config bridge error, got {other:?}"),
        }
    }

    #[cfg(feature = "oauth-providers")]
    #[test]
    fn host_driven_oauth_flow_holds_at_most_one_pending_exchange() {
        // Serialize against the process-global OAUTH_REQUESTS: this is the only
        // test that touches it, and it drives every phase in order.
        lock_bridge_mutex(&OAUTH_REQUESTS).clear();

        let mut creds = std::collections::HashMap::new();
        creds.insert(
            coven::CloudProvider::GoogleDrive,
            coven::OAuthClientCreds {
                client_id: "test-client-id".to_string(),
                client_secret: None,
            },
        );
        bae_core::oauth::set_client_creds(creds).expect("register test OAuth client creds");

        let redirect_uri = "bae://oauth".to_string();

        // A second begin supersedes the first: at most one pending exchange.
        oauth_begin(BridgeCloudProvider::GoogleDrive, redirect_uri.clone())
            .expect("first begin builds an authorize request");
        let second = oauth_begin(BridgeCloudProvider::GoogleDrive, redirect_uri.clone())
            .expect("second begin builds an authorize request");
        assert_eq!(
            lock_bridge_mutex(&OAUTH_REQUESTS).len(),
            1,
            "a fresh begin supersedes the prior un-completed one"
        );

        // Cancel reclaims the pending host-driven exchange.
        oauth_cancel();
        assert!(
            lock_bridge_mutex(&OAUTH_REQUESTS).is_empty(),
            "cancel clears the pending host-driven exchange"
        );

        // Completing the cancelled flow's request finds nothing to exchange.
        let err = oauth_complete(
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

    #[test]
    fn onboarding_runtime_is_ok_and_idempotent() {
        let first = onboarding_runtime().expect("onboarding runtime builds on the normal path");
        let second = onboarding_runtime().expect("onboarding runtime is reused");
        assert!(
            std::ptr::eq(first, second),
            "repeated calls return the same runtime"
        );
    }

    #[test]
    fn on_worker_returns_bridge_error_for_panicked_task() {
        let result: Result<(), BridgeError> = on_worker(|| async {
            panic!("onboarding test panic");
        });

        let error = result.expect_err("worker panic should become a bridge error");
        match error {
            BridgeError::Diagnostic { category, detail } => {
                assert_eq!(category, BridgeErrorCategory::Internal);
                assert!(detail.contains("onboarding worker task panicked"));
            }
            other => panic!("expected diagnostic bridge error, got {other:?}"),
        }
    }

    #[test]
    fn on_worker_constructs_the_future_on_its_runtime() {
        let caller = std::thread::current().id();
        let constructed = on_worker(move || {
            let construction_thread = std::thread::current().id();
            async move { Ok::<_, BridgeError>(construction_thread) }
        })
        .expect("worker returns its construction thread");

        assert_ne!(
            constructed, caller,
            "the operation future must be constructed after entering the owned runtime"
        );
    }
}
