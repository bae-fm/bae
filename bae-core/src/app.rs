//! Composition root: builds and starts the whole application for a library and
//! injects its running services into the frontend's final owner. Frontends reach
//! it through the uniffi bridge (`bae-bridge`, via its `AppHandle`). One place opens the DB, unlocks
//! encryption, starts sync, playback, and (on desktop) the import/identify/
//! extraction services — no per-frontend copy to drift.

use std::{sync::Arc, time::Instant};

use tracing::{info, warn};

use crate::config::{Config, ConfigHandle};
use crate::diagnostics::{AnomalyKind, Diagnostics, TelemetryEvent};
use crate::library::AppServices;
use coven::{ClockRef, SystemClock};
use coven::{IdSourceRef, UuidIds};

struct BootstrapTiming {
    started: Instant,
}

impl BootstrapTiming {
    fn start(library_id: &str) -> Self {
        info!(%library_id, "Application bootstrap started");
        Self {
            started: Instant::now(),
        }
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

/// Why reading the cloud-home key state failed, as a bootstrap outcome.
///
/// Coven types a keychain that refused *right now* apart from every other
/// keyring failure precisely so a host can ask again after unlock. Flattening it
/// into `Config` aborted the boot as if the library were misconfigured — which
/// no amount of unlocking fixes, and which sent the user to a restore wall for a
/// library that was fine.
pub(crate) fn classify_key_state_error(error: crate::library::LibraryError) -> BootstrapError {
    match error {
        crate::library::LibraryError::Keyring(coven::KeyError::KeychainTemporarilyUnavailable(
            _,
        )) => BootstrapError::KeyringUnavailable,
        crate::library::LibraryError::Keyring(coven::KeyError::MissingKeychainEntitlement(
            error,
        )) => BootstrapError::KeychainEntitlementMissing(error),
        error => BootstrapError::Config(error.to_string()),
    }
}

fn classify_open_error(error: crate::db::DbError) -> BootstrapError {
    match error {
        crate::db::DbError::Coven(coven::CovenError::Key(
            coven::KeyError::KeychainTemporarilyUnavailable(_),
        )) => BootstrapError::KeyringUnavailable,
        crate::db::DbError::Coven(coven::CovenError::Key(
            coven::KeyError::MissingKeychainEntitlement(error),
        )) => BootstrapError::KeychainEntitlementMissing(error),
        error @ crate::db::DbError::Coven(coven::CovenError::Database(
            coven::DbError::DamagedDatabase,
        )) => BootstrapError::DeviceResetRequired(error.to_string()),
        crate::db::DbError::Coven(coven::CovenError::Lock(
            coven::StoreLockError::ReplacementUnfinished { by, .. },
        )) => BootstrapError::ReplacementUnfinished(by),
        error => BootstrapError::Database(format!("Failed to open database: {error}")),
    }
}

/// Build and start the application for `library_id`, the library registered
/// under `app_dir`.
///
/// `position_update_interval_ms` controls how often playback emits a position
/// tick, and `audio_focus` is how the platform shares the speaker, asked before
/// every start of playback. Returns once the DB is open and playback (plus the desktop import
/// pipeline) is running. For an unlocked cloud library, sync attachment starts
/// on the returned runtime and reports readiness or failure through sync status.
///
/// `builder` supplies the installation's custody, OAuth presentation, and storage.
///
/// Opening by registered id records the library as this device's active library
/// only once the open fully completes and the library is unlocked on this device;
/// a locked or failed open leaves the pointer unchanged.
///
/// A step that fails before `compose` closes the library before the runtime
/// ends. One that fails after drops the owner `compose` built, whose drop
/// closes the library first, as the bridge's `AppHandle` does.
pub fn bootstrap<T, F>(
    app_dir: crate::config::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    audio_focus: crate::playback::AudioFocus,
    diagnostics: Diagnostics,

    builder: coven::CovenBuilder,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError> + Send + 'static,
{
    bootstrap_opening(
        Opening::Stored,
        app_dir,
        library_id,
        position_update_interval_ms,
        restore_playback,
        audio_focus,
        diagnostics,
        builder.into(),
        compose,
    )
}

/// Start services on the live store returned by creation, restore, or device reset.
/// The caller retains the runtime on which that store was prepared.
pub fn bootstrap_prepared<T, F>(
    app_dir: crate::config::AppDir,
    handle: coven::CovenHandle,
    position_update_interval_ms: u32,
    restore_playback: bool,
    audio_focus: crate::playback::AudioFocus,
    diagnostics: Diagnostics,
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
        audio_focus,
        diagnostics,
        crate::library::manager::LibraryOpening::Open(handle),
        compose,
    )
}

/// [`bootstrap`] with the [`crate::library::LibraryFixture`] at `fixture`
/// written into the library before any of its services starts, so each starts
/// on the state the fixture names: what a debug UI test opens the app on.
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub fn bootstrap_on_fixture<T, F>(
    fixture: std::path::PathBuf,
    app_dir: crate::config::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    audio_focus: crate::playback::AudioFocus,
    diagnostics: Diagnostics,

    builder: coven::CovenBuilder,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError> + Send + 'static,
{
    bootstrap_opening(
        Opening::Fixture(fixture),
        app_dir,
        library_id,
        position_update_interval_ms,
        restore_playback,
        audio_focus,
        diagnostics,
        builder.into(),
        compose,
    )
}

/// What the library holds as it is brought up.
enum Opening {
    /// What it stored, with every watched folder read again for what
    /// changed on disk while it was closed.
    Stored,
    /// What it stored with the fixture at this path written over it first.
    /// No watched folder is read: the fixture states what each one holds.
    #[cfg(all(
        any(test, feature = "test-utils", debug_assertions),
        not(any(target_os = "ios", target_os = "android"))
    ))]
    Fixture(std::path::PathBuf),
}

fn bootstrap_opening<T, F>(
    opening: Opening,
    app_dir: crate::config::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    audio_focus: crate::playback::AudioFocus,
    diagnostics: Diagnostics,

    builder: crate::library::manager::LibraryOpening,
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
    let bootstrap_thread = std::thread::Builder::new()
        .name("bae-bootstrap".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            bootstrap_inner(
                opening,
                app_dir,
                library_id,
                position_update_interval_ms,
                restore_playback,
                audio_focus,
                diagnostics,
                builder,
                compose,
            )
        })
        .map_err(|error| {
            BootstrapError::Internal(format!("Failed to spawn bootstrap thread: {error}"))
        })?;

    bootstrap_thread.join().map_err(|panic| {
        let message = if let Some(message) = panic.downcast_ref::<&str>() {
            *message
        } else if let Some(message) = panic.downcast_ref::<String>() {
            message.as_str()
        } else {
            "non-string panic payload"
        };
        BootstrapError::Internal(format!("Bootstrap thread panicked: {message}"))
    })?
}

fn bootstrap_inner<T, F>(
    opening: Opening,
    app_dir: crate::config::AppDir,
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    audio_focus: crate::playback::AudioFocus,
    diagnostics: Diagnostics,

    builder: crate::library::manager::LibraryOpening,
    compose: F,
) -> Result<T, BootstrapError>
where
    F: FnOnce(AppServices, tokio::runtime::Runtime) -> Result<T, BootstrapError>,
{
    let timing = BootstrapTiming::start(&library_id);

    let clock: ClockRef = Arc::new(SystemClock);
    let ids: IdSourceRef = Arc::new(UuidIds);

    let config = timing
        .stage(&library_id, "load config", || {
            Config::load_registered_library(&app_dir, &library_id)
        })
        .map_err(|error| match error {
            crate::config::ConfigError::Config(_) => {
                BootstrapError::LibraryNotFound(library_id.clone())
            }
            other => BootstrapError::Config(other.to_string()),
        })?;
    let library_id = config.store_id.to_string();

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

    let config_handle = Arc::new(ConfigHandle::new(config));

    // The one transport every outbound request goes out on, and the providers
    // that ask over it: each keeps its own rate limit and answers for as long
    // as this library is open.
    let http = bae_util::http::Http::new()
        .map_err(|e| BootstrapError::Internal(format!("Failed to build the HTTP client: {e}")))?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let providers = crate::providers::Providers::new(http.clone());

    let library_manager = timing
        .stage(&library_id, "open library manager", || {
            runtime.block_on(crate::library::LibraryManager::open(
                app_dir.clone(),
                Arc::clone(&config_handle),
                Arc::clone(&clock),
                ids,
                diagnostics.clone(),
                runtime.handle().clone(),
                builder,
                bae_catalogs::remote_image::RemoteImageCache::new(
                    config_handle.config().library_path(),
                    http,
                ),
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                providers,
            ))
        })
        .map_err(classify_open_error)?;

    // From here the library is open on `runtime`. A step that fails closes it
    // before the runtime ends, so nothing of it is left running for the
    // runtime's shutdown to cancel under it.
    let opened = library_manager.clone();
    let started = (|| {
        let provider_configured = runtime
            .block_on(library_manager.is_sync_configured())
            .map_err(|error| BootstrapError::Config(error.to_string()))?;
        let key_state = library_manager
            .store_key_state()
            .map_err(classify_key_state_error)?;
        let locked = provider_configured && key_state == coven::StoreKeyState::Locked;
        let advance_active_pointer = !locked;
        if locked {
            warn!("store key unavailable at startup; opening library locked");
            diagnostics.event(TelemetryEvent::Anomaly {
                kind: AnomalyKind::EncryptionKeyMissing,
            });
        }

        // Configure coven's per-namespace cache budgets (device-local, idempotent):
        // the bulk for audio, a small reserved slice each for covers / artist images.
        timing
            .stage(&library_id, "configure cache budgets", || {
                runtime.block_on(library_manager.configure_cache_budgets())
            })
            .map_err(|e| BootstrapError::Database(e.to_string()))?;

        // Install the sync-status and outbox subscriptions before connection starts.
        // coven's status receiver may be subscribed before a provider connects, so the
        // attachment's first events cannot race the library/UI observers.
        timing
            .stage(&library_id, "start library observation", || {
                library_manager.start()
            })
            .map_err(|e| BootstrapError::Internal(e.to_string()))?;

        let startup_sync_manager = if provider_configured && !locked {
            Some(library_manager.clone())
        } else {
            None
        };

        // A fixture states what its watched folders hold, and nothing on disk
        // backs it, so reading them would replace it with what is there.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let reads_watched_folders = matches!(opening, Opening::Stored);
        match opening {
            Opening::Stored => {}
            #[cfg(all(
                any(test, feature = "test-utils", debug_assertions),
                not(any(target_os = "ios", target_os = "android"))
            ))]
            Opening::Fixture(path) => timing
                .stage(&library_id, "write the fixture", || {
                    let fixture = crate::library::LibraryFixture::read(&path)?;
                    runtime.block_on(library_manager.write_fixture(&fixture))
                })
                .map_err(|error| BootstrapError::Internal(error.to_string()))?,
        }

        // The in-core cpal/ffmpeg audio engine. cpal drives the sink on desktop and
        // iOS, AAudio on Android.
        let playback_handle = timing
            .stage(&library_id, "start playback service", || {
                runtime.block_on(library_manager.start_playback_service(
                    runtime.handle().clone(),
                    position_update_interval_ms,
                    restore_playback,
                    audio_focus,
                ))
            })
            .map_err(|e| BootstrapError::Database(e.to_string()))?;

        // The import pipeline (scanning, transcoding, identify) is desktop-only.
        // Mobile is a sync/playback client, so its `AppServices` carries just the
        // library manager and the in-core player.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let app_services = {
            let import_handle = timing
                .stage(&library_id, "start import service", || {
                    runtime.block_on(library_manager.start_import_service(runtime.handle().clone()))
                })
                .map_err(|error| BootstrapError::Database(error.to_string()))?;

            AppServices::new(library_manager, playback_handle, import_handle)
        };

        #[cfg(any(target_os = "ios", target_os = "android"))]
        let app_services = AppServices::new(library_manager, playback_handle);

        info!("Application services initialized for library '{library_id}'");
        diagnostics.event(TelemetryEvent::AppStarted {});

        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if reads_watched_folders {
            timing
                .stage(&library_id, "request watched-folder scan", || {
                    app_services.import_scan_watched_folders()
                })
                .map_err(|error| BootstrapError::Database(error.to_string()))?;
        }

        Ok((app_services, advance_active_pointer, startup_sync_manager))
    })();
    let (app_services, advance_active_pointer, startup_sync_manager) = match started {
        Ok(started) => started,
        Err(error) => {
            if let Err(closing) = runtime.block_on(opened.close()) {
                diagnostics.report_bug("close the library a failed open left open", &closing);
            }
            return Err(error);
        }
    };
    drop(opened);

    let owner = timing.stage(&library_id, "compose frontend owner", || {
        compose(app_services, runtime)
    })?;

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
                config_handle.config().save_active_library(&app_dir)
            })
            .map_err(|e| BootstrapError::Config(e.to_string()))?;
    }

    // Local services and the frontend owner are fully initialized before provider
    // attachment begins. The task runs on the runtime retained by that owner, and
    // failures flow through the sync-status subscription installed above. A later
    // reconnect still uses the direct awaited attachment path.
    if let Some(manager) = startup_sync_manager {
        timing
            .stage(&library_id, "schedule startup sync attachment", || {
                manager.start_sync_at_startup()
            })
            .map_err(|e| BootstrapError::Internal(e.to_string()))?;
    }

    timing.complete(&library_id);
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::{classify_key_state_error, BootstrapError};
    use crate::library::LibraryError;

    #[tokio::test]
    async fn a_keychain_that_refused_right_now_is_not_a_config_failure() {
        let temp = tempfile::tempdir().unwrap();
        let installation = coven::TestCoven::new();
        installation.fail_next_keychain_operation();
        let failure = crate::library::configure_store(
            installation.builder(
                crate::config::AppDir::at(temp.path())
                    .unwrap()
                    .store_layout(),
            ),
        )
        .create_store("Unavailable Keychain")
        .await
        .err()
        .unwrap();
        let coven::StoreCreationError::Store(coven::CovenError::Key(key)) = failure else {
            panic!("unexpected failure: {failure}")
        };
        let error = LibraryError::Keyring(key);

        assert!(matches!(
            classify_key_state_error(error),
            BootstrapError::KeyringUnavailable
        ));
    }

    #[tokio::test]
    async fn opening_with_a_locked_keychain_remains_retryable() {
        let temp = tempfile::tempdir().unwrap();
        let installation = coven::TestCoven::new();
        installation.fail_next_keychain_operation();
        let failure = crate::library::configure_store(
            installation.builder(
                crate::config::AppDir::at(temp.path())
                    .unwrap()
                    .store_layout(),
            ),
        )
        .create_store("Keychain test")
        .await
        .err()
        .unwrap();
        let coven::StoreCreationError::Store(error) = failure else {
            panic!("unexpected failure: {failure}")
        };
        assert!(matches!(
            super::classify_open_error(error.into()),
            BootstrapError::KeyringUnavailable
        ));
    }

    fn missing_entitlement() -> coven::KeyError {
        coven::KeyError::MissingKeychainEntitlement(
            keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into(),
        )
    }

    /// A build signed without its keychain entitlement fails every keychain
    /// read the same way. Opening reports that, not a broken library or a
    /// locked keychain to wait out.
    #[test]
    fn a_missing_keychain_entitlement_is_reported_as_itself() {
        assert!(matches!(
            classify_key_state_error(LibraryError::Keyring(missing_entitlement())),
            BootstrapError::KeychainEntitlementMissing(_)
        ));
        assert!(matches!(
            super::classify_open_error(crate::db::DbError::Coven(coven::CovenError::Key(
                missing_entitlement()
            ))),
            BootstrapError::KeychainEntitlementMissing(_)
        ));
    }

    /// A step that fails once the library manager has started closes the
    /// library before the bootstrap's runtime ends: nothing of it is left
    /// running for the runtime's shutdown to cancel under it.
    #[test]
    fn a_bootstrap_that_fails_after_the_library_starts_closes_it() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let app_dir = crate::config::AppDir::at(temp.path()).unwrap();
        let installation = coven::TestCoven::new();
        let handle = runtime
            .block_on(
                crate::library::configure_store(installation.builder(app_dir.store_layout()))
                    .create_store("Failing Bootstrap"),
            )
            .unwrap();
        let library_id = handle.store_id().to_string();

        let failed = super::bootstrap_opening(
            super::Opening::Fixture(temp.path().join("no-such-fixture.json")),
            app_dir,
            library_id,
            50,
            false,
            crate::playback::AudioFocus::Unshared,
            crate::diagnostics::Diagnostics::noop(),
            crate::library::manager::LibraryOpening::Open(handle.clone()),
            |_services, _runtime| -> Result<(), BootstrapError> {
                unreachable!("the bootstrap fails before it composes an owner")
            },
        );

        assert!(
            matches!(failed, Err(BootstrapError::Internal(_))),
            "the fixture step fails: {failed:?}"
        );
        let read = runtime.block_on(handle.pending());
        assert!(
            matches!(
                read,
                Err(coven::CovenError::Database(coven::DbError::StoreClosed))
            ),
            "the library's store is closed: {read:?}"
        );
    }

    #[test]
    fn every_other_keyring_failure_still_reads_as_config() {
        // A keyring that is genuinely broken is not something unlocking fixes,
        // so it must not join the retry path.
        let error = LibraryError::Keyring(coven::KeyError::DeviceIdEncoding);

        assert!(matches!(
            classify_key_state_error(error),
            BootstrapError::Config(_)
        ));
    }
}
