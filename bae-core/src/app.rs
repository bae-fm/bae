//! Composition root: builds and starts the whole application for a library and
//! injects its running services into the frontend's final owner. Frontends reach
//! it through the uniffi bridge (`bae-bridge`, via its `AppHandle`). One place opens the DB, unlocks
//! encryption, starts sync, playback, and (on desktop) the import/identify/
//! extraction services, and wires the UI event bus — no per-frontend copy to
//! drift.

use std::{sync::Arc, time::Instant};

use tracing::{info, warn};

use crate::config::{Config, ConfigHandle};
use crate::diagnostics::{AnomalyKind, Diagnostics, TelemetryEvent};
use crate::library::AppServices;
use crate::ui::UiEventBus;
use coven::{ClockRef, SystemClock};
use coven::{IdRef, UuidProvider};

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
    Internal(String),
    /// The OS keychain refused the read *right now* — the session is locked, the
    /// display is asleep, or there is no UI session to prompt in. The key is not
    /// missing and nothing is misconfigured; the same call succeeds once the
    /// device is unlocked, so this is the one bootstrap failure a host should
    /// retry rather than report as broken.
    #[error("the OS keychain is not available right now")]
    KeyringUnavailable,
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
        crate::library::LibraryError::Keyring(coven::KeyError::KeychainTemporarilyUnavailable) => {
            BootstrapError::KeyringUnavailable
        }
        error => BootstrapError::Config(error.to_string()),
    }
}

/// Build and start the application for `library_id`.
///
/// `position_update_interval_ms` controls how often playback emits a position
/// tick. Returns once the DB is open and playback (plus the desktop import
/// pipeline) is running. For an unlocked cloud library, sync attachment starts
/// on the returned runtime and reports readiness or failure through sync status.
///
/// Opening by registered id records the library as this device's active library
/// only once the open fully completes and the library is unlocked on this device;
/// a locked or failed open leaves the pointer unchanged.
pub fn bootstrap<T, F>(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<crate::CloudKitOpsRef>,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, UiEventBus, tokio::runtime::Runtime) -> Result<T, BootstrapError>
        + Send
        + 'static,
{
    bootstrap_on_thread(
        library_id,
        position_update_interval_ms,
        restore_playback,
        diagnostics,
        cloudkit_ops,
        compose,
    )
}

fn bootstrap_on_thread<T, F>(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<crate::CloudKitOpsRef>,
    compose: F,
) -> Result<T, BootstrapError>
where
    T: Send + 'static,
    F: FnOnce(AppServices, UiEventBus, tokio::runtime::Runtime) -> Result<T, BootstrapError>
        + Send
        + 'static,
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
                library_id,
                position_update_interval_ms,
                restore_playback,
                diagnostics,
                cloudkit_ops,
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
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<crate::CloudKitOpsRef>,
    compose: F,
) -> Result<T, BootstrapError>
where
    F: FnOnce(AppServices, UiEventBus, tokio::runtime::Runtime) -> Result<T, BootstrapError>,
{
    let timing = BootstrapTiming::start(&library_id);

    // The injected wall clock + id source are built before loading the config so
    // device-id auto-generation draws from the injected source too.
    let clock: ClockRef = Arc::new(SystemClock);
    let ids: IdRef = Arc::new(UuidProvider);

    let config = timing
        .stage(&library_id, "load config", || {
            Config::load_registered_library(&library_id, ids.as_ref())
        })
        .map_err(|error| match error {
            crate::config::ConfigError::Config(_) => {
                BootstrapError::LibraryNotFound(library_id.clone())
            }
            other => BootstrapError::Config(other.to_string()),
        })?;
    let library_id = config.store_id.clone();

    // Telemetry was built by the host at process start (from compiled-in values
    // only) and handed in. Enrich it now with the stable per-device id the config
    // mints on first run — a set-once post-construction step, the one place late
    // mutability is accepted. Events before this point (host launch, keyring,
    // config load) ship without the field rather than a placeholder.
    diagnostics.set_device_id(config.device_id.clone());

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

    timing.stage(&library_id, "initialize audio codecs", || {
        crate::audio_codec::init()
    });

    let config_handle = Arc::new(ConfigHandle::new(config));

    let library_manager = timing
        .stage(&library_id, "open library manager", || {
            crate::library::LibraryManager::open(
                Arc::clone(&config_handle),
                Arc::clone(&clock),
                ids,
                diagnostics.clone(),
                runtime.handle().clone(),
                cloudkit_ops,
                crate::import::cover_art::RemoteImageCache::new(
                    config_handle.config().library_path(),
                ),
            )
        })
        .map_err(|e| BootstrapError::Database(format!("Failed to open database: {e}")))?;

    let provider_configured = config_handle.config().cloud_home.provider.is_some();
    let key_state = timing
        .stage(&library_id, "read cloud-home key state", || {
            library_manager.cloud_home_key_state()
        })
        .map_err(classify_key_state_error)?;
    let locked = provider_configured && key_state == coven::CloudHomeKeyState::Locked;
    let advance_active_pointer = !locked;
    if locked {
        // Reads as Locked both for a genuinely missing master key and when the
        // OS keychain refuses the read — e.g. an app relaunched by a launch
        // agent while the screen is locked. Without this line such a boot is
        // indistinguishable in the log from a healthy one: sync silently never
        // starts and the first visible symptom is the absence of cycles.
        warn!(
            "cloud home key unavailable at startup; opening library locked, sync not started \
             (provider {provider:?})",
            provider = config_handle.config().cloud_home.provider
        );
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
    timing.stage(&library_id, "start library observation", || {
        library_manager.start()
    });

    let startup_sync_manager = if provider_configured && !locked {
        Some(library_manager.clone())
    } else {
        None
    };

    // The in-core cpal/ffmpeg audio engine. cpal drives the sink on desktop and
    // iOS, AAudio on Android.
    let playback_handle = timing.stage(&library_id, "start playback service", || {
        library_manager.start_playback_service(
            runtime.handle().clone(),
            position_update_interval_ms,
            restore_playback,
        )
    });

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

    let ui_event_bus = UiEventBus::new();
    timing.stage(&library_id, "wire UI event bus", || {
        ui_event_bus.wire(&app_services, runtime.handle())
    });
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    timing
        .stage(&library_id, "request watched-folder scan", || {
            app_services.import_scan_watched_folders()
        })
        .map_err(|error| BootstrapError::Database(error.to_string()))?;

    let owner = timing.stage(&library_id, "compose frontend owner", || {
        compose(app_services, ui_event_bus, runtime)
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
                config_handle.config().save_active_library()
            })
            .map_err(|e| BootstrapError::Config(e.to_string()))?;
    }

    // Local services and the frontend owner are fully initialized before provider
    // attachment begins. The task runs on the runtime retained by that owner, and
    // failures flow through the sync-status subscription installed above. A later
    // reconnect still uses the direct awaited attachment path.
    if let Some(manager) = startup_sync_manager {
        timing.stage(&library_id, "schedule startup sync attachment", || {
            manager.start_sync_at_startup()
        });
    }

    timing.complete(&library_id);
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::{classify_key_state_error, BootstrapError};
    use crate::library::LibraryError;

    #[test]
    fn a_keychain_that_refused_right_now_is_not_a_config_failure() {
        let error = LibraryError::Keyring(coven::KeyError::KeychainTemporarilyUnavailable);

        assert!(matches!(
            classify_key_state_error(error),
            BootstrapError::KeyringUnavailable
        ));
    }

    #[test]
    fn every_other_keyring_failure_still_reads_as_config() {
        // A keyring that is genuinely broken is not something unlocking fixes,
        // so it must not join the retry path.
        let error = LibraryError::Keyring(coven::KeyError::InvalidLength {
            subject: "cloud home master key",
            expected: 32,
            actual: 3,
        });

        assert!(matches!(
            classify_key_state_error(error),
            BootstrapError::Config(_)
        ));
    }
}
