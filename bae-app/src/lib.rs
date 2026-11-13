//! Composition root: builds and starts the whole application for a library and
//! injects its running services into the frontend's final owner, the
//! [`AppServices`] every surface calls. Frontends reach it through the uniffi
//! bridge (`bae-bridge`, via its `AppHandle`). One place opens the DB, unlocks
//! encryption, starts sync, playback, and (on desktop) the import/identify/
//! extraction services — no per-frontend copy to drift.

use std::{sync::Arc, time::Instant};

use tracing::{info, warn};

mod app_services;
mod assembly;
pub mod live_reads;
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub mod test_services;

pub use app_services::*;
pub use assembly::{start_library_services, PlaybackOutput, ServicesStartError};
use bae_diagnostics::{AnomalyKind, Diagnostics, TelemetryEvent};
use coven::ClockRef;
use coven::IdSourceRef;

struct BootstrapTiming {
    started: Instant,
}

impl BootstrapTiming {
    fn start(library_id: &str) -> Self {
        info!(%library_id, "Application bootstrap started");
        Self { started: Instant::now() }
    }

    fn stage<T>(&self, library_id: &str, stage: &'static str, run: impl FnOnce() -> T) -> T {
        info!(%library_id, stage, "Application bootstrap stage started");
        let started = Instant::now();
        let output = run();
        info!(
            %library_id,
            stage,
            stage_ms = %started.elapsed().as_millis(),
            total_ms = %self.started.elapsed().as_millis(),
            "Application bootstrap stage returned"
        );
        output
    }

    fn complete(&self, library_id: &str) {
        info!(
            %library_id,
            total_ms = %self.started.elapsed().as_millis(),
            "Application bootstrap completed"
        );
    }
}

/// Why [`bootstrap`] could not bring the application up. Frontends map these
/// onto their own error surface (the bridge onto `BridgeError`, the desktop app
/// onto a UI error screen).
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("Library '{0}' not found")]
    LibraryNotFound(String),
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Database(String),
    #[error("{0}")]
    DeviceResetRequired(String),
    /// A reset or a restore of this device's copy stopped after the new copy
    /// took the old one's place. Only running the named call again finishes
    /// it; until then the library doesn't open.
    #[error("the library opens once {0} finishes replacing this device's copy")]
    ReplacementUnfinished(coven::ReplacedBy),
    /// This device is removing the library, and its removal stopped partway.
    /// The library doesn't open; only finishing the removal ends it.
    #[error("the library is being removed from this device")]
    DeletionUnfinished,
    #[error("{0}")]
    Internal(String),
    /// The OS keychain refused the read *right now* — the session is locked, the
    /// display is asleep, or there is no UI session to prompt in. The key is not
    /// missing and nothing is misconfigured; the same call succeeds once the
    /// device is unlocked, so this is the one bootstrap failure a host should
    /// retry rather than report as broken.
    #[error("the OS keychain is not available right now")]
    KeyringUnavailable,
    /// The app was signed without its keychain entitlement, so no keychain
    /// read succeeds. A packaging fault that neither retrying nor unlocking
    /// fixes.
    #[error(
        "the app lacks its keychain-access-groups entitlement (errSecMissingEntitlement): {0}"
    )]
    KeychainEntitlementMissing(coven::KeychainError),
}

/// Why reading the storage key state failed, as a bootstrap outcome.
///
/// Coven types a keychain that refused *right now* apart from every other
/// keyring failure precisely so a host can ask again after unlock. Flattening it
/// into `Config` aborted the boot as if the library were misconfigured — which
/// no amount of unlocking fixes, and which sent the user to a restore wall for a
/// library that was fine.
pub(crate) fn classify_key_state_error(error: bae_library::LibraryError) -> BootstrapError {
    match error {
        bae_library::LibraryError::Keyring(key) => match BootstrapError::from_keychain_refusal(key)
        {
            Ok(refusal) => refusal,
            Err(key) => BootstrapError::Config(bae_library::LibraryError::Keyring(key).to_string()),
        },
        error => BootstrapError::Config(error.to_string()),
    }
}

impl BootstrapError {
    /// This failure as a surface states it: its class, with its own text as
    /// the untranslated detail, or for a library that isn't there, that
    /// library's id, which a surface says as not found rather than as a class.
    pub fn ui_error(&self) -> Result<bae_diagnostics::UiError, &str> {
        use bae_diagnostics::UiErrorCategory as C;
        let category = match self {
            Self::LibraryNotFound(id) => return Err(id),
            Self::Config(_) => C::Config,
            Self::Database(_) => C::Database,
            Self::DeviceResetRequired(_) => C::DeviceResetRequired,
            // The class names the one call that finishes the replacement, so
            // the host offers that call and never the other.
            Self::ReplacementUnfinished(by) => C::ReplacementUnfinished(*by),
            Self::DeletionUnfinished => C::DeletionUnfinished,
            Self::Internal(_) => C::Internal,
            // Its own class, not `Keyring`: that one reads as "the keyring
            // broke", and this one is "it is locked this second, try again
            // after you unlock". The host branches on the class to keep the
            // handle path open and retry.
            Self::KeyringUnavailable => C::KeyringLocked,
            Self::KeychainEntitlementMissing(_) => C::KeychainEntitlementMissing,
        };
        Ok(bae_diagnostics::UiError::diagnostic(category, self))
    }

    /// The bootstrap outcome of a keychain that refused the read, or `key`
    /// back when it failed some other way.
    fn from_keychain_refusal(key: coven::KeyError) -> Result<Self, coven::KeyError> {
        match key {
            coven::KeyError::KeychainTemporarilyUnavailable(_) => Ok(Self::KeyringUnavailable),
            coven::KeyError::MissingKeychainEntitlement(error) => {
                Ok(Self::KeychainEntitlementMissing(error))
            }
            key => Err(key),
        }
    }
}

fn classify_open_error(error: bae_store::DbError) -> BootstrapError {
    use bae_util::task_failure::ReportedBug;
    // A bug in the open's work, or in the manager's first calls, reported as
    // it arrived: nothing the person can do but report it.
    if error.reported_bug().is_some() {
        return BootstrapError::Internal(format!("Failed to open database: {error}"));
    }
    let bae_store::DbError::Coven(coven_error) = error else {
        return BootstrapError::Database(format!("Failed to open database: {error}"));
    };
    match *coven_error {
        coven::CovenError::Key(key) => match BootstrapError::from_keychain_refusal(key) {
            Ok(refusal) => refusal,
            Err(key) => {
                let error = bae_store::DbError::from(coven::CovenError::Key(key));
                BootstrapError::Database(format!("Failed to open database: {error}"))
            }
        },
        error @ coven::CovenError::Database(coven::DbError::DamagedDatabase) => {
            BootstrapError::DeviceResetRequired(error.to_string())
        }
        coven::CovenError::Lock(coven::StoreLockError::ReplacementUnfinished { by, .. }) => {
            BootstrapError::ReplacementUnfinished(by)
        }
        coven::CovenError::Lock(coven::StoreLockError::DeletionUnfinished(_)) => {
            BootstrapError::DeletionUnfinished
        }
        // The platform refused coven's watch of the network, as an Android
        // app packaged without coven's network library is: no retry or
        // setting fixes it.
        error @ coven::CovenError::NetworkWatch(_) => {
            BootstrapError::Internal(format!("Failed to open database: {error}"))
        }
        error => BootstrapError::Database(format!("Failed to open database: {error}")),
    }
}

/// Build and start the application for `library_id`, the library registered
/// under `app_dir`.
///
/// `position_update_interval_ms` controls how often playback emits a position
/// tick, and `playback_output` is where playback plays: the system's output,
/// shared as its `AudioFocus` says and asked before every start of playback,
/// or a test's device. Returns once the DB is open and playback (plus the desktop import
/// pipeline) is running. For an unlocked cloud library, sync attachment starts
/// on the returned runtime and reports readiness or failure through sync status.
///
/// `builder` supplies the installation's custody, OAuth presentation, and storage.
/// `on_open_progress` hears each step opening the store runs (rebuilding
/// its indexes, then migrating it) with how much of the step is done, on the
/// bootstrap's own thread; an open with neither step calls it never.
///
/// Opening by registered id records the library as this device's active library
/// only once the open fully completes and the library is unlocked on this device;
/// a locked or failed open leaves the pointer unchanged.
///
/// A step that fails before `compose` closes the library before the runtime
/// ends. One that fails after drops the owner `compose` built, whose drop
/// closes the library first, as the bridge's `AppHandle` does.
pub fn bootstrap<T, F>(
    app_dir: bae_app_dir::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_output: PlaybackOutput,
    diagnostics: Diagnostics,
    clock: ClockRef,
    ids: IdSourceRef,

    builder: coven::CovenBuilder,
    on_open_progress: impl Fn(coven::Progress<coven::OpenStep>) + Send + 'static,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError> + Send + 'static,
{
    let store = registered_store(&library_id)?;
    bootstrap_opening(
        Opening::Stored,
        app_dir,
        store.to_string(),
        position_update_interval_ms,
        restore_playback,
        playback_output,
        diagnostics,
        clock,
        ids,
        LibraryOpening::Registered { builder, store, on_progress: Box::new(on_open_progress) },
        compose,
    )
}

/// Start services on the live store returned by creation, restore, or device reset.
/// The caller retains the runtime on which that store was prepared.
pub fn bootstrap_prepared<T, F>(
    app_dir: bae_app_dir::AppDir,
    handle: coven::CovenHandle,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_output: PlaybackOutput,
    diagnostics: Diagnostics,
    clock: ClockRef,
    ids: IdSourceRef,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError> + Send + 'static,
{
    let id = handle.store_id().to_string();
    bootstrap_opening(
        Opening::Stored,
        app_dir,
        id,
        position_update_interval_ms,
        restore_playback,
        playback_output,
        diagnostics,
        clock,
        ids,
        LibraryOpening::Open(handle),
        compose,
    )
}

/// [`bootstrap`] with the [`bae_library::LibraryFixture`] at `fixture`
/// written into the library before any of its services starts, so each starts
/// on the state the fixture names: what a debug UI test opens the app on.
/// The store's open reports its steps to `on_open_progress`, as for
/// [`bootstrap`].
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
pub fn bootstrap_on_fixture<T, F>(
    fixture: std::path::PathBuf,
    app_dir: bae_app_dir::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_output: PlaybackOutput,
    diagnostics: Diagnostics,
    clock: ClockRef,
    ids: IdSourceRef,

    builder: coven::CovenBuilder,
    on_open_progress: impl Fn(coven::Progress<coven::OpenStep>) + Send + 'static,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError> + Send + 'static,
{
    let store = registered_store(&library_id)?;
    bootstrap_opening(
        Opening::Fixture(fixture),
        app_dir,
        store.to_string(),
        position_update_interval_ms,
        restore_playback,
        playback_output,
        diagnostics,
        clock,
        ids,
        LibraryOpening::Registered { builder, store, on_progress: Box::new(on_open_progress) },
        compose,
    )
}

/// Where an open of a library reports each step it runs, with how much of
/// it is done: rebuilding indexes, then migrating. Most opens run neither.
type OpenProgressReport = Box<dyn Fn(coven::Progress<coven::OpenStep>) + Send>;

/// Either open an existing store or retain a handle returned by creation or restore.
enum LibraryOpening {
    /// Open the registered store `store`, reporting the open's steps to
    /// `on_progress`.
    Registered {
        builder: coven::CovenBuilder,
        store: coven::StoreId,
        on_progress: OpenProgressReport,
    },
    Open(coven::CovenHandle),
}

/// The library's store as `opening` finds it: the registered store opened
/// through coven's top-level builder, or the handle already open. An open
/// that fails on a bug of its own work reports it to `diagnostics`, as the
/// store reports each of its calls'.
async fn open_store(
    opening: LibraryOpening,
    clock: ClockRef,
    diagnostics: &Diagnostics,
) -> Result<bae_store::Store, bae_store::DbError> {
    let handle = match opening {
        LibraryOpening::Registered { builder, store, on_progress } => {
            bae_store::configure_store(builder, bae_library::SCHEMA)
                .open(store, on_progress)
                .await
                .map_err(|error| coven::task::reported(diagnostics, error))?
        }
        LibraryOpening::Open(handle) => handle,
    };
    Ok(bae_store::Store::new(handle, clock, diagnostics.bug_sink()))
}

/// The library a registered id names; an id that is no library's is not
/// found.
fn registered_store(library_id: &str) -> Result<coven::StoreId, BootstrapError> {
    bae_app_dir::parse_library_id(library_id).map_err(|error| {
        warn!(%library_id, %error, "no library is registered under this id");
        BootstrapError::LibraryNotFound(library_id.to_string())
    })
}

/// What the library holds as it is brought up.
enum Opening {
    /// What it stored, with every watched folder read again for what
    /// changed on disk while it was closed.
    Stored,
    /// What it stored with the fixture at this path written over it first.
    /// No watched folder is read: the fixture states what each one holds.
    #[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
    Fixture(std::path::PathBuf),
}

fn bootstrap_opening<T, F>(
    opening: Opening,
    app_dir: bae_app_dir::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_output: PlaybackOutput,
    diagnostics: Diagnostics,
    clock: ClockRef,
    ids: IdSourceRef,

    builder: LibraryOpening,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError> + Send + 'static,
{
    // Building the sync manager and `block_on`-ing the async setup uses a deep
    // stack, especially in debug builds. Callers may invoke us from small-stack
    // threads (Swift cooperative Tasks, Android coroutine workers; ~0.5 MB), which
    // overflow there — so run the whole thing on a thread with a large stack.
    // A panic fails the open as something that went wrong, which the panic
    // hook reported as a bug; what it had built was dropped as the thread
    // unwound.
    let reporter = diagnostics.clone();
    reporter
        .run_on_thread("bae-bootstrap", 32 * 1024 * 1024, move || {
            bootstrap_inner(
                opening,
                app_dir,
                library_id,
                position_update_interval_ms,
                restore_playback,
                playback_output,
                diagnostics,
                clock,
                ids,
                builder,
                compose,
            )
        })
        .map_err(|failed| BootstrapError::Internal(failed.to_string()))?
}

fn bootstrap_inner<T, F>(
    opening: Opening,
    app_dir: bae_app_dir::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_output: PlaybackOutput,
    diagnostics: Diagnostics,
    clock: ClockRef,
    ids: IdSourceRef,

    builder: LibraryOpening,
    compose: F,
) -> Result<T, BootstrapError>
where
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError>,
{
    let timing = BootstrapTiming::start(&library_id);

    let install_id = app_dir
        .install_id(ids.as_ref())
        .map_err(|error| BootstrapError::Config(error.to_string()))?;
    diagnostics.set_device_id(install_id);

    let runtime = timing
        .stage(&library_id, "build async runtime", || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                // The sync cycle (snapshot creation, changeset apply) runs on these
                // workers and overflows the default 2 MB stack in debug builds.
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
        })
        .map_err(|e| BootstrapError::Internal(format!("Failed to create runtime: {e}")))?;

    timing.stage(&library_id, "initialize audio codecs", bae_audio::init);

    #[cfg(unix)]
    timing.stage(&library_id, "raise open-file limit", || {
        bae_util::open_file_limit::raise_open_file_limit()
    });

    // The one transport every outbound request goes out on, and the providers
    // that ask over it: each keeps its own rate limit and answers for as long
    // as this library is open. Only the desktop's import asks the catalogs.
    #[cfg(desktop)]
    let http = bae_util::http::Http::new(Arc::clone(&clock))
        .map_err(|e| BootstrapError::Internal(format!("Failed to build the HTTP client: {e}")))?;
    #[cfg(desktop)]
    let providers = bae_catalogs::providers::Providers::new(http.clone());

    // The library's store, opened once; each part of the open library is
    // built from it here, once, and handed to what holds it.
    let store = timing
        .stage(&library_id, "open the library's store", || {
            runtime.block_on(open_store(builder, Arc::clone(&clock), &diagnostics))
        })
        .map_err(classify_open_error)?;
    let preferences = bae_preferences::PreferencesRow::new(store.clone());
    let records = bae_records::Records::new(store.clone(), ids.clone());
    let people = bae_people::People::new(store.clone());
    let membership = bae_people::Membership::new(store.clone());
    let sync_status = bae_sync_status::SyncStatus::new(store.clone());
    #[cfg(desktop)]
    let remote_images = bae_catalogs::remote_image::RemoteImageCache::new(
        &app_dir.registered_library(&library_id),
        http,
        diagnostics.clone(),
    );
    #[cfg(desktop)]
    let catalogs = bae_import::CatalogSessions::new(
        providers,
        store.clone(),
        preferences.clone(),
        diagnostics.clone(),
        runtime.handle().clone(),
    );
    #[cfg(desktop)]
    let import_tables = bae_import::tables::ImportTables::new(store.clone());
    #[cfg(desktop)]
    let release_reads = bae_import::ReleaseReads::new(import_tables.clone(), catalogs.clone());
    // Closing the library ends what runs while it is open: the manager's
    // tasks and the import service's.
    let closing = bae_library::CancellationToken::new();
    let library_manager = timing
        .stage(&library_id, "open library manager", || {
            runtime.block_on(bae_library::open_library_manager(
                store.clone(),
                preferences,
                records.clone(),
                people.clone(),
                membership.clone(),
                sync_status.clone(),
                #[cfg(desktop)]
                release_reads,
                closing.clone(),
                app_dir.clone(),
                Arc::clone(&clock),
                ids.clone(),
                diagnostics.clone(),
                runtime.handle().clone(),
                #[cfg(desktop)]
                remote_images.clone(),
            ))
        })
        .map_err(classify_open_error)?;

    let store_id = store.store_id();

    // From here the library is open on `runtime`. A step that fails closes it
    // before the runtime ends, so nothing of it is left running for the
    // runtime's shutdown to cancel under it.
    let opened = library_manager.clone();
    let started = (|| {
        let provider_configured = runtime
            .block_on(library_manager.is_sync_configured())
            .map_err(|error| BootstrapError::Config(error.to_string()))?;
        // A storage setup running as the library opens was cut past its
        // point of no return (opening undid any before it), and commits its
        // location only once sync's connection finishes it.
        let setup_finishing = runtime
            .block_on(library_manager.storage_setup_running())
            .map_err(|error| BootstrapError::Database(error.to_string()))?;
        runtime
            .block_on(library_manager.acknowledge_settled_storage_setups())
            .map_err(|error| BootstrapError::Database(error.to_string()))?;
        let connects = provider_configured || setup_finishing;
        let key_state = store
            .store_key_state()
            .map_err(bae_library::LibraryError::from)
            .map_err(classify_key_state_error)?;
        let locked = connects && key_state == coven::StoreKeyState::Locked;
        let advance_active_pointer = !locked;
        if locked {
            warn!("store key unavailable at startup; opening library locked");
            diagnostics.event(TelemetryEvent::Anomaly { kind: AnomalyKind::EncryptionKeyMissing });
        }

        // Install the sync-status and pending subscriptions before connection starts.
        // coven's status receiver may be subscribed before a provider connects, so the
        // attachment's first events cannot race the library/UI observers.
        timing
            .stage(&library_id, "start library observation", || library_manager.start())
            .map_err(|e| BootstrapError::Internal(e.to_string()))?;

        // Sync starts before the frontend owner exists, so no read that needs
        // storage can come first: coven's storage is connecting from here, and
        // a cover the first screen asks for waits for the connection instead
        // of failing as a library with no storage does. Starting never waits
        // on the network, and its failures flow through the sync-status
        // subscription installed above. A later reconnect uses the direct
        // awaited attachment path.
        if connects && !locked {
            timing
                .stage(&library_id, "start sync", || {
                    runtime.block_on(library_manager.start_sync_at_startup())
                })
                .map_err(|e| BootstrapError::Internal(e.to_string()))?;
        }

        // A fixture states what its watched folders hold, and nothing on disk
        // backs it, so reading them would replace it with what is there.
        #[cfg(desktop)]
        let reads_watched_folders = matches!(opening, Opening::Stored);
        match opening {
            Opening::Stored => {}
            #[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
            Opening::Fixture(path) => timing.stage(&library_id, "write the fixture", || {
                let fixture = bae_library::LibraryFixture::read(&path)
                    .map_err(|error| BootstrapError::Internal(error.to_string()))?;
                runtime
                    .block_on(library_manager.write_fixture(&fixture))
                    .map_err(|error| BootstrapError::Internal(error.to_string()))
            })?,
        }

        // The in-core cpal/ffmpeg audio engine (cpal drives the sink on
        // desktop and iOS, AAudio on Android) and, on desktop, the import
        // pipeline, under the one owner of the opened library.
        let app_services = timing
            .stage(&library_id, "start library services", || {
                runtime.block_on(start_library_services(
                    library_manager,
                    store,
                    records,
                    people,
                    membership,
                    sync_status,
                    #[cfg(desktop)]
                    catalogs,
                    #[cfg(desktop)]
                    import_tables,
                    #[cfg(desktop)]
                    closing,
                    #[cfg(desktop)]
                    Arc::clone(&clock),
                    ids,
                    diagnostics.clone(),
                    runtime.handle().clone(),
                    #[cfg(desktop)]
                    remote_images,
                    position_update_interval_ms,
                    restore_playback,
                    Arc::new(bae_playback::WallPlaybackClock::new(Arc::clone(&clock))),
                    playback_output,
                ))
            })
            .map_err(|error| BootstrapError::Database(error.to_string()))?;

        info!("Application services initialized for library '{library_id}'");
        diagnostics.event(TelemetryEvent::AppStarted {});

        #[cfg(desktop)]
        if reads_watched_folders {
            timing
                .stage(&library_id, "request watched-folder scan", || {
                    runtime.block_on(app_services.import_scan_watched_folders())
                })
                .map_err(|error| BootstrapError::Database(error.to_string()))?;
        }

        Ok((app_services, advance_active_pointer))
    })();
    let (app_services, advance_active_pointer) = match started {
        Ok(started) => started,
        Err(error) => {
            if let Err(closing) = runtime.block_on(opened.close()) {
                diagnostics.report_failure("close the library a failed open left open", &closing);
            }
            return Err(error);
        }
    };
    drop(opened);

    let owner =
        timing.stage(&library_id, "compose frontend owner", || compose(app_services, runtime))?;

    // The durable active-library pointer names the library the user last actually
    // landed in, so launch ordering (discovery sorts active-first) and
    // forget_library's pointer check both refer to a library that opens. It
    // advances only over a fully-realized open: written after the frontend owner
    // has started, and never for a locked library — cancelling the unlock screen
    // must leave the previously-active library in charge. A successful unlock
    // re-runs bootstrap unlocked and advances the pointer then.
    if advance_active_pointer {
        timing
            .stage(&library_id, "save active-library pointer", || {
                app_dir.save_active_library(store_id)
            })
            .map_err(|e| BootstrapError::Config(e.to_string()))?;
    }

    timing.complete(&library_id);
    Ok(owner)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
