//! Pre-AppHandle free functions: library discovery, creation, restore, unlock.
//!
//! These run before an AppHandle exists (they create or configure the library
//! that AppHandle will later open).

fn parse_oauth_tokens(json: &str) -> Result<coven::OAuthTokens, BridgeError> {
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

/// Build a BridgeLibrary from its raw parts. The two call sites that wrap
/// this — `BridgeLibrary::from_core` for a freshly-opened Config,
/// `BridgeLibrary::from_core_info` for a discovery-scan entry — just differ in
/// where the fields come from.
fn local_library(
    id: String,
    name: String,
    path: std::path::PathBuf,
    cloud_provider: Option<&bae_core::config::CloudProvider>,
    is_active: bool,
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
            std::path::PathBuf::from(&*config.store_dir),
            config.cloud_home.provider.as_ref(),
            true,
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
        } = info;
        local_library(id, name, path, cloud_provider.as_ref(), is_active)
    }
}

use std::sync::{Arc, Mutex};

use tracing::info;

use bae_core::config::Config;
use bae_core::library::{CancellationToken, JoinFromCodeError, RestoreFromCodeError};

use crate::types::{
    BridgeCloudProvider, BridgeError, BridgeInviteCodeInfo, BridgeJoinRequest,
    BridgeJoinRequestInfo, BridgeLibrary, BridgeRestoreCodeInfo, BridgeRestoreSource,
};

#[cfg(feature = "cloudkit")]
use crate::cloudkit::get_cloudkit_ops;

#[cfg(not(feature = "cloudkit"))]
fn get_cloudkit_ops() -> Option<std::sync::Arc<dyn coven::CloudKitOps>> {
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

/// Holds a handle to the in-progress OAuth runtime plus a cancellation flag.
/// `oauth_cancel()` sends a signal via the oneshot, which the authorize flow
/// selects on alongside the callback wait.
#[cfg(feature = "oauth-providers")]
static OAUTH_CANCEL_TX: Mutex<Option<tokio::sync::watch::Sender<bool>>> = Mutex::new(None);
/// A pending host-driven OAuth exchange, keyed by request id between
/// [`oauth_begin`] and [`oauth_complete`]. coven's `AuthorizeRequest` — which
/// carries the PKCE verifier and state `oauth_complete` must feed back — is not
/// part of coven's curated public API and lives in its `pub(crate)` oauth
/// module, so the bridge can't name the type to store it. Instead it captures
/// the request inside this closure (whose type it never names) that runs the
/// exchange when the redirect comes back.
#[cfg(feature = "oauth-providers")]
type OAuthExchange = Box<
    dyn FnOnce(
            coven::CloudProvider, // provider
            String,               // authorization code
            String,               // callback state
            String,               // redirect uri
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<coven::OAuthTokens, coven::OAuthError>>
                    + Send,
            >,
        > + Send,
>;
#[cfg(feature = "oauth-providers")]
static OAUTH_REQUESTS: std::sync::LazyLock<
    Mutex<std::collections::HashMap<String, OAuthExchange>>,
> = std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

fn lock_bridge_mutex<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
/// `Coven::builder(...).synced_tables(...)` at the point the library is opened,
/// so there is no separate process-global registration step.
#[uniffi::export]
pub fn init_keyring() {
    bae_core::config::init_keyring();
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

/// Create a new library. The library is set as the active library.
#[uniffi::export]
pub fn create_library(name: Option<String>) -> Result<BridgeLibrary, BridgeError> {
    let ids = std::sync::Arc::new(coven::UuidProvider);
    let config = match name {
        Some(n) => bae_core::library::create_library(n, ids.as_ref()),
        None => bae_core::library::create_library_default(ids.as_ref()),
    }
    .map_err(|e| BridgeError::config(format!("{e}")))?;

    BridgeLibrary::from_core(&config)
}

/// Run the future built by `make_fut` on a worker of a shared onboarding
/// runtime, blocking the calling thread until it completes.
///
/// The onboarding exports (restore, OAuth) run before any `AppHandle` (and its
/// tokio runtime) exists, so they share this process-wide runtime instead. Its
/// workers have 16 MB stacks (like `init`'s) — deep enough for coven's pull
/// descents. `spawn` moves that work onto a worker; the foreign caller only
/// `block_on`s the shallow `JoinHandle`, so nothing deep is ever polled on its
/// ~0.5 MB stack. (The AWS-SDK S3 endpoint descent runs on coven's own
/// big-stack S3 runtime regardless of who awaits it.)
///
/// This requires the futures to be `Send` + `'static`. They are: coven's pull
/// path carries the database handle as a `Send`-able `SendDbPtr`, so no
/// non-`Send` `*mut sqlite3` is held across the download await.
fn on_worker<T, Fut>(make_fut: impl FnOnce() -> Fut) -> Result<T, BridgeError>
where
    Fut: std::future::Future<Output = Result<T, BridgeError>> + Send + 'static,
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
    match rt.block_on(rt.spawn(make_fut())) {
        Ok(result) => result,
        Err(join_err) => Err(BridgeError::internal(format!(
            "onboarding worker task panicked: {join_err}"
        ))),
    }
}

/// Restore a library from cloud storage using the provided encryption key.
#[uniffi::export]
pub fn restore_from_cloud(
    library_id: String,
    encryption_key_hex: String,
    library_name: Option<String>,
    source: BridgeRestoreSource,
) -> Result<BridgeLibrary, BridgeError> {
    use bae_core::sync::RestoreSource;
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

        BridgeLibrary::from_core(&config)
    })
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

/// Generate this device's join-request code and the fingerprint it encodes, to
/// hand to an existing member for approval. Safe to call before any library
/// exists on this device (the joining device has no library yet); it only needs
/// the keyring initialized.
///
/// `email` is the OAuth account address the joiner authenticated as, baked into
/// the code so the approver can share the OAuth folder to it; `None` for S3. The
/// caller runs the OAuth flow and `fetch_account_email` first, then passes the
/// result here.
#[uniffi::export]
pub fn generate_join_request(email: Option<String>) -> Result<BridgeJoinRequest, BridgeError> {
    let request = bae_core::sync::membership::generate_join_request(email)
        .map_err(|e| BridgeError::config(format!("Failed to generate join request: {e}")))?;
    Ok(BridgeJoinRequest::from_core(request))
}

impl BridgeJoinRequest {
    fn from_core(request: bae_core::sync::membership::JoinRequest) -> Self {
        let bae_core::sync::membership::JoinRequest { code, fingerprint } = request;
        BridgeJoinRequest { code, fingerprint }
    }
}

/// Fetch the email of the OAuth account `oauth_token_json` authenticated, so the
/// joiner can bake it into its join-request. The caller runs `oauth_authorize`
/// (or `oauth_begin`/`oauth_complete`) first and passes the resulting token JSON.
#[cfg(feature = "oauth-providers")]
#[uniffi::export]
pub fn fetch_account_email(
    provider: BridgeCloudProvider,
    oauth_token_json: String,
) -> Result<String, BridgeError> {
    let tokens = parse_oauth_tokens(&oauth_token_json)?;
    let core_provider = provider.into_core();
    on_worker(move || async move {
        bae_core::sync::membership::fetch_account_email(core_provider, &tokens)
            .await
            .map_err(|e| BridgeError::config(format!("Failed to fetch account email: {e}")))
    })
}

/// Decode a join-request code to preview the joining device before approving it.
#[uniffi::export]
pub fn decode_join_request(code: String) -> Result<BridgeJoinRequestInfo, BridgeError> {
    let req =
        bae_core::sync::membership::decode_join_request(&code).map_err(BridgeError::config)?;
    Ok(BridgeJoinRequestInfo::from_core(req))
}

impl BridgeJoinRequestInfo {
    fn from_core(req: bae_core::sync::membership::JoinRequestInfo) -> Self {
        let bae_core::sync::membership::JoinRequestInfo {
            pubkey,
            fingerprint,
            email,
        } = req;
        BridgeJoinRequestInfo {
            pubkey,
            fingerprint,
            email,
        }
    }
}

/// Decode an invite code string and return info for UI preview (before joining).
#[uniffi::export]
pub fn decode_invite_code(code: String) -> Result<BridgeInviteCodeInfo, BridgeError> {
    let info =
        bae_core::sync::membership::decode_invite_code_info(&code).map_err(BridgeError::config)?;
    Ok(BridgeInviteCodeInfo::from_core(info))
}

impl BridgeInviteCodeInfo {
    fn from_core(info: bae_core::sync::membership::InviteCodeInfo) -> Self {
        let bae_core::sync::membership::InviteCodeInfo {
            library_id,
            library_name,
            owner_pubkey,
            owner_fingerprint,
            cloud_provider,
            needs_oauth,
        } = info;
        BridgeInviteCodeInfo {
            library_id,
            library_name,
            owner_pubkey,
            owner_fingerprint,
            cloud_provider: BridgeCloudProvider::from_core(&cloud_provider),
            needs_oauth,
        }
    }
}

fn join_error_to_bridge(error: JoinFromCodeError) -> BridgeError {
    match error {
        JoinFromCodeError::Cancelled => BridgeError::Cancelled,
        JoinFromCodeError::Join(error) => {
            BridgeError::internal(format!("Failed to join library: {error}"))
        }
    }
}

async fn join_from_code_config(
    code: String,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
) -> Result<Config, BridgeError> {
    match cancel {
        Some(cancel) => bae_core::library::join_from_code_cancellable(
            &code,
            oauth_tokens,
            cloudkit_ops,
            cancel,
            |status| info!("{}", status),
        )
        .await
        .map_err(join_error_to_bridge),
        None => bae_core::library::join_from_code(&code, oauth_tokens, cloudkit_ops, |status| {
            info!("{}", status)
        })
        .await
        .map_err(|e| join_error_to_bridge(JoinFromCodeError::Join(e))),
    }
}

/// Join a shared library from an invite code string.
///
/// For OAuth providers, the caller must first run the OAuth flow (`oauth_authorize`
/// on desktop, or `oauth_begin`/`oauth_complete` on mobile) and pass the token
/// JSON as `oauth_token_json`.
#[uniffi::export]
pub fn join_from_code(
    code: String,
    oauth_token_json: Option<String>,
) -> Result<BridgeLibrary, BridgeError> {
    on_worker(move || async move {
        let oauth_tokens = oauth_token_json
            .map(|json| parse_oauth_tokens(&json))
            .transpose()?;

        let config = join_from_code_config(code, oauth_tokens, get_cloudkit_ops(), None).await?;

        BridgeLibrary::from_core(&config)
    })
}

#[derive(uniffi::Object)]
pub struct JoinFromCodeOperation {
    code: String,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: CancellationToken,
    started: Mutex<bool>,
}

#[uniffi::export]
pub fn join_from_code_operation(
    code: String,
    oauth_token_json: Option<String>,
) -> Result<Arc<JoinFromCodeOperation>, BridgeError> {
    let oauth_tokens = oauth_token_json
        .map(|json| parse_oauth_tokens(&json))
        .transpose()?;
    Ok(Arc::new(JoinFromCodeOperation {
        code,
        oauth_tokens,
        cloudkit_ops: get_cloudkit_ops(),
        cancel: CancellationToken::new(),
        started: Mutex::new(false),
    }))
}

#[uniffi::export]
impl JoinFromCodeOperation {
    pub fn join(&self) -> Result<BridgeLibrary, BridgeError> {
        {
            let mut started = lock_bridge_mutex(&self.started);
            if *started {
                return Err(BridgeError::internal(
                    "join operation already started".to_string(),
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
                join_from_code_config(code, oauth_tokens, cloudkit_ops, Some(cancel)).await?;

            BridgeLibrary::from_core(&config)
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
    // Cancel any lingering previous flow
    oauth_cancel();

    let cancel = new_oauth_cancel();

    let result = on_worker(move || async move {
        let core_provider = provider.into_core();
        let clock = std::sync::Arc::new(coven::SystemClock);
        let tokens = coven::authorize_provider(core_provider, cancel, clock.as_ref())
            .await
            .map_err(|e| BridgeError::config(format!("OAuth authorization failed: {e}")))?;

        serde_json::to_string(&tokens)
            .map_err(|e| BridgeError::internal(format!("Failed to serialize tokens: {e}")))
    });

    // Clean up
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
            coven::OAuthClientCreds {
                client_id,
                client_secret,
            },
        );
    }
    coven::set_oauth_client_creds(creds)
        .map_err(|e| BridgeError::config(format!("OAuth client credentials conflict: {e}")))?;
    Ok(())
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
    let req = coven::build_authorize_request_for_provider(core_provider, &redirect_uri)
        .map_err(|e| BridgeError::config(format!("OAuth begin failed: {e}")))?;
    let auth_url = req.auth_url.clone();
    // An opaque handle correlating this begin with its later complete; not a
    // PKCE value (coven holds the real verifier inside `req`).
    let request_id = coven::IdProvider::new_id(&coven::UuidProvider);
    let exchange: OAuthExchange = Box::new(move |provider, code, state, redirect_uri| {
        Box::pin(async move {
            let clock = std::sync::Arc::new(coven::SystemClock);
            coven::exchange_code_for_provider(
                provider,
                &code,
                Some(&state),
                &req,
                &redirect_uri,
                clock.as_ref(),
            )
            .await
        })
    });
    lock_bridge_mutex(&OAUTH_REQUESTS).insert(request_id.clone(), exchange);
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
    let exchange = lock_bridge_mutex(&OAUTH_REQUESTS)
        .remove(&request_id)
        .ok_or_else(|| {
            BridgeError::config("OAuth request not found or already used".to_string())
        })?;
    let tokens = on_worker(move || async move {
        exchange(core_provider, code, state, redirect_uri)
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
}
