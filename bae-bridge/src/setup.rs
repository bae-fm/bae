//! Pre-AppHandle onboarding: decoding setup codes, the restore and join
//! operations, and the conversions `BridgeHost`'s library discovery, creation,
//! and pairing methods return through.
//!
//! These run before an AppHandle exists (they create or configure the library
//! that AppHandle will later open).

pub(crate) fn parse_oauth_tokens(json: &str) -> Result<coven::OAuthTokens, BridgeError> {
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
    pub(crate) fn from_core(config: &bae_core::config::Config) -> Result<Self, BridgeError> {
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

use crate::types::{
    BridgeCloudProvider, BridgeDevicePairingOffer, BridgeError, BridgeJoiningDeviceJoinProgress,
    BridgeLibrary, BridgePendingDevicePairingJoin, BridgeRestoreCodeInfo,
    JoiningDeviceJoinProgressCallback,
};

fn restore_error_to_bridge(error: RestoreFromCodeError) -> BridgeError {
    match error {
        RestoreFromCodeError::Cancelled => BridgeError::Cancelled,
        error @ RestoreFromCodeError::Restore(_) => BridgeError::diagnostic(
            crate::types::BridgeErrorCategory::from_core(error.category()),
            error,
        ),
    }
}

pub(crate) async fn restore_from_code_config(
    app_dir: bae_core::config::AppDir,
    code: String,
    oauth_clients: coven::OAuthClients,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
) -> Result<Config, BridgeError> {
    match cancel {
        Some(cancel) => bae_core::library::restore_from_code_cancellable(
            &app_dir,
            &code,
            oauth_clients,
            oauth_tokens,
            cloudkit_ops,
            cancel,
            |status| info!("{}", status),
        )
        .await
        .map_err(restore_error_to_bridge),
        None => bae_core::library::restore_from_code(
            &app_dir,
            &code,
            oauth_clients,
            oauth_tokens,
            cloudkit_ops,
            |status| info!("{}", status),
        )
        .await
        .map_err(restore_error_to_bridge),
    }
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

/// Run the future built by `make_fut` on a worker of `runtime`, blocking the
/// calling thread until it completes.
///
/// `spawn` moves the work onto a worker, so the foreign caller only
/// `block_on`s the shallow `JoinHandle` and nothing deep is ever polled on its
/// ~0.5 MB stack. (The AWS-SDK S3 endpoint descent runs on coven's own
/// big-stack S3 runtime regardless of who awaits it.)
///
/// That requires the futures to be `Send` + `'static`, which the bounds below
/// enforce at compile time for every operation handed to it.
pub(crate) fn on_worker<T, Fut>(
    runtime: &tokio::runtime::Handle,
    make_fut: impl FnOnce() -> Fut + Send + 'static,
) -> Result<T, BridgeError>
where
    Fut: std::future::Future<Output = Result<T, BridgeError>> + Send + 'static,
    T: Send + 'static,
{
    match runtime.block_on(crate::operation_runtime::spawn(runtime.clone(), make_fut)) {
        Ok(result) => result,
        Err(join_err) => Err(BridgeError::internal(format!(
            "onboarding worker task panicked: {join_err}"
        ))),
    }
}

/// Decode a restore code string and return info for UI preview.
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

/// Nothing panics while an onboarding operation's state is locked, so a
/// poisoned lock is a bug and fails loudly.
const OPERATION_LOCK: &str = "an onboarding operation lock is never held across a panic";

#[derive(uniffi::Object)]
pub struct RestoreFromCodeOperation {
    app_dir: bae_core::config::AppDir,
    code: String,
    oauth_clients: coven::OAuthClients,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    runtime: tokio::runtime::Handle,
    cancel: CancellationToken,
    started: Mutex<bool>,
}

impl RestoreFromCodeOperation {
    pub(crate) fn new(
        app_dir: bae_core::config::AppDir,
        code: String,
        oauth_clients: coven::OAuthClients,
        oauth_tokens: Option<coven::OAuthTokens>,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            app_dir,
            code,
            oauth_clients,
            oauth_tokens,
            cloudkit_ops,
            runtime,
            cancel: CancellationToken::new(),
            started: Mutex::new(false),
        }
    }
}

#[uniffi::export]
impl RestoreFromCodeOperation {
    pub fn restore(&self) -> Result<BridgeLibrary, BridgeError> {
        {
            let mut started = self.started.lock().expect(OPERATION_LOCK);
            if *started {
                return Err(BridgeError::internal(
                    "restore operation already started".to_string(),
                ));
            }
            *started = true;
        }
        let app_dir = self.app_dir.clone();
        let code = self.code.clone();
        let oauth_clients = self.oauth_clients.clone();
        let oauth_tokens = self.oauth_tokens.clone();
        let cloudkit_ops = self.cloudkit_ops.clone();
        let cancel = self.cancel.clone();
        on_worker(&self.runtime, move || async move {
            let config = restore_from_code_config(
                app_dir,
                code,
                oauth_clients,
                oauth_tokens,
                cloudkit_ops,
                Some(cancel),
            )
            .await?;

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

/// Whether a scanned setup code belongs to the device-pairing flow. The
/// envelope decides the destination before either payload decoder runs.
#[uniffi::export]
pub fn is_device_pairing_code(code: String) -> bool {
    coven::DevicePairingOffer::is_pairing_code(&code)
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

mirror_enum! {
    crate::types::BridgeDevicePairingPhase = coven::DevicePairingPhase,
    from_core: fn,
    variants: {
        AwaitingInvitation,
        ProviderAccessPending,
        LibraryInstallationPending,
    },
}

mirror_struct! {
    BridgePendingDevicePairingJoin = bae_core::library::PendingDevicePairingJoinInfo,
    from_core: pub(crate) fn,
    fields: {
        pairing_code,
        offer: (BridgeDevicePairingOffer),
        fingerprint,
        phase: (crate::types::BridgeDevicePairingPhase),
    },
}

pub(crate) fn join_error_to_bridge(error: JoinDevicePairingError) -> BridgeError {
    use crate::types::{BridgeDeviceJoinFailure, BridgeErrorCategory};
    // Every end a join can come to that the user can act on carries its own
    // line, because the advice differs: get a fresh code, open bae over there,
    // or start again. Only a local cancel is silent — the user did it.
    let join_failed = |failure, detail: &str| {
        BridgeError::diagnostic(BridgeErrorCategory::DeviceJoin { failure }, detail)
    };
    match error {
        JoinDevicePairingError::Cancelled => BridgeError::Cancelled,
        JoinDevicePairingError::Expired => {
            join_failed(BridgeDeviceJoinFailure::Expired, "pairing session expired")
        }
        JoinDevicePairingError::OwnerOffline => join_failed(
            BridgeDeviceJoinFailure::OwnerOffline,
            "the inviting device is not running the join",
        ),
        JoinDevicePairingError::Abandoned => join_failed(
            BridgeDeviceJoinFailure::OwnerEnded,
            "the inviting device ended the join",
        ),
        error => BridgeError::diagnostic(BridgeErrorCategory::from_core(error.category()), error),
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
    runtime: tokio::runtime::Handle,
    cancel: CancellationToken,
}

enum JoinDevicePairingOperationState {
    Prepared(bae_core::library::PreparedDevicePairingJoin),
    Started,
    Abandoned,
}

impl JoinDevicePairingOperation {
    pub(crate) async fn prepare(
        app_dir: &bae_core::config::AppDir,
        pairing_code: &str,
        oauth_clients: coven::OAuthClients,
        oauth_tokens: Option<coven::OAuthTokens>,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        runtime: tokio::runtime::Handle,
    ) -> Result<Self, BridgeError> {
        let prepared = bae_core::library::prepare_device_pairing_join(
            app_dir,
            pairing_code,
            oauth_clients,
            oauth_tokens,
            cloudkit_ops,
        )
        .await
        .map_err(join_error_to_bridge)?;
        Ok(Self {
            fingerprint: prepared.fingerprint(),
            state: Mutex::new(JoinDevicePairingOperationState::Prepared(prepared)),
            runtime,
            cancel: CancellationToken::new(),
        })
    }
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
            let mut state = self.state.lock().expect(OPERATION_LOCK);
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
        on_worker(&self.runtime, move || async move {
            let config = join_device_pairing_config(prepared, cancel, progress).await?;

            BridgeLibrary::from_core(&config)
        })
    }

    pub fn cancel(&self) -> Result<(), BridgeError> {
        let mut state = self.state.lock().expect(OPERATION_LOCK);
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

    #[test]
    fn scanned_pairing_code_is_classified_by_its_envelope() {
        assert!(is_device_pairing_code(
            "  coven:device-pairing:not-yet-decoded  ".to_string()
        ));
        assert!(!is_device_pairing_code("coven:restore-payload".to_string()));
    }

    #[test]
    fn on_worker_returns_bridge_error_for_panicked_task() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build worker runtime");
        let result: Result<(), BridgeError> = on_worker(runtime.handle(), || async {
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

    /// A restore code that does not decode is the person's to fix, so it
    /// reaches the UI as a configuration line rather than an internal fault.
    #[tokio::test]
    async fn an_undecodable_restore_code_is_a_configuration_error() {
        let root = tempfile::TempDir::new().unwrap();
        let error = restore_from_code_config(
            bae_core::config::AppDir::under_home(root.path()),
            "placeholder-code-that-does-not-decode".to_string(),
            coven::OAuthClients::empty(),
            None,
            None,
            None,
        )
        .await
        .expect_err("an undecodable code does not restore");

        match error {
            BridgeError::Diagnostic { category, .. } => {
                assert_eq!(category, BridgeErrorCategory::Config)
            }
            other => panic!("expected a diagnostic bridge error, got {other:?}"),
        }
    }

    /// Likewise a pairing code that does not decode: the person scans a fresh
    /// one.
    #[test]
    fn an_undecodable_pairing_code_is_a_configuration_error() {
        let decode = coven::DevicePairingOffer::decode("placeholder-pairing-code")
            .expect_err("the placeholder does not decode");

        match join_error_to_bridge(JoinDevicePairingError::from(decode)) {
            BridgeError::Diagnostic { category, .. } => {
                assert_eq!(category, BridgeErrorCategory::Config)
            }
            other => panic!("expected a diagnostic bridge error, got {other:?}"),
        }
    }

    #[test]
    fn on_worker_constructs_the_future_on_its_runtime() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build worker runtime");
        let caller = std::thread::current().id();
        let constructed = on_worker(runtime.handle(), move || {
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
