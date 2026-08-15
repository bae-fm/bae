//! Composition root: builds and starts the whole application for a library and
//! injects its running services into the frontend's final owner. Frontends reach
//! it through the uniffi bridge (`bae-bridge`, via its `AppHandle`). One place opens the DB, unlocks
//! encryption, starts sync, playback, and (on desktop) the import/identify/
//! extraction services, and wires the UI event bus — no per-frontend copy to
//! drift.

use std::sync::Arc;

use tracing::info;

use crate::config::{Config, ConfigHandle};
use crate::diagnostics::{AnomalyKind, Diagnostics, TelemetryEvent};
use crate::library::AppServices;
use crate::ui::UiEventBus;
use coven::{ClockRef, SystemClock};
use coven::{IdRef, UuidProvider};

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
}

/// Build and start the application for `library_id`.
///
/// `position_update_interval_ms` controls how often playback emits a position
/// tick. Returns once the DB is open, sync is attached (if the library is
/// unlocked on this device), and playback (plus the desktop import pipeline) is
/// running; background work continues on the returned runtime.
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
    // The injected wall clock + id source are built before loading the config so
    // device-id auto-generation draws from the injected source too.
    let clock: ClockRef = Arc::new(SystemClock);
    let ids: IdRef = Arc::new(UuidProvider);

    let config =
        Config::load_registered_library(&library_id, ids.as_ref()).map_err(
            |error| match error {
                crate::config::ConfigError::Config(_) => {
                    BootstrapError::LibraryNotFound(library_id.clone())
                }
                other => BootstrapError::Config(other.to_string()),
            },
        )?;
    let library_id = config.store_id.clone();

    // Telemetry was built by the host at process start (from compiled-in values
    // only) and handed in. Enrich it now with the stable per-device id the config
    // mints on first run — a set-once post-construction step, the one place late
    // mutability is accepted. Events before this point (host launch, keyring,
    // config load) ship without the field rather than a placeholder.
    diagnostics.set_device_id(config.device_id.clone());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        // The sync cycle (snapshot creation, changeset apply) runs on these
        // workers and overflows the default 2 MB stack in debug builds.
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .map_err(|e| BootstrapError::Internal(format!("Failed to create runtime: {e}")))?;

    crate::audio_codec::init();

    let dev_secrets = crate::config::dev_secrets();
    let config_handle = Arc::new(ConfigHandle::new(config));

    let library_manager = crate::library::LibraryManager::open(
        Arc::clone(&config_handle),
        Arc::clone(&clock),
        ids,
        diagnostics.clone(),
        runtime.handle().clone(),
        cloudkit_ops,
        crate::import::cover_art::RemoteImageCache::new(
            Arc::clone(&clock),
            config_handle.config().library_path(),
        ),
    )
    .map_err(|e| BootstrapError::Database(format!("Failed to open database: {e}")))?;

    if let Some(token) = dev_secrets.discogs_api_key.as_deref() {
        library_manager
            .set_discogs_key(token, crate::config::DiscogsValidation::Unvalidated)
            .map_err(|error| BootstrapError::Config(error.to_string()))?;
    }

    let provider_configured = config_handle.config().cloud_home.provider.is_some();
    let mut key_state = library_manager
        .cloud_home_key_state()
        .map_err(|error| BootstrapError::Config(error.to_string()))?;
    let mut connected_during_unlock = false;
    if provider_configured && key_state == coven::CloudHomeKeyState::Locked {
        if let Some(master_key) = dev_secrets.master_key.as_deref() {
            runtime
                .block_on(library_manager.unlock_cloud_home(master_key))
                .map_err(|error| BootstrapError::Config(error.to_string()))?;
            key_state = coven::CloudHomeKeyState::Available;
            connected_during_unlock = true;
        }
    }
    let locked = provider_configured && key_state == coven::CloudHomeKeyState::Locked;
    let advance_active_pointer = !locked;
    if locked {
        diagnostics.event(TelemetryEvent::Anomaly {
            kind: AnomalyKind::EncryptionKeyMissing,
        });
    }

    // Configure coven's per-namespace cache budgets (device-local, idempotent):
    // the bulk for audio, a small reserved slice each for covers / artist images.
    runtime
        .block_on(library_manager.configure_cache_budgets())
        .map_err(|e| BootstrapError::Database(e.to_string()))?;

    // Now that the manager owns the outbox in-flight set and event channel, build
    // and start the sync manager (if unlocked, or keyless) so the upload observer
    // shares them. Must precede `library_manager.start()`, which subscribes to the
    // sync loop. The database already holds the seeded `_updated_at` stamper from
    // `open` above; building the manager here only wires the cloud loop. coven
    // resolves the at-rest cipher itself from the master-key custody (an
    // opaque-but-locked home stays unbuilt above, awaiting unlock), so there is no
    // key material left to thread through here — an established opaque key and a
    // keyless browsable home now take the identical call.
    // A connect failure here (the network is down, the provider is unreachable) must
    // not abort the launch: the library opens, and sync reports itself not connected
    // so the UI shows its reconnect banner. Local browse and pinned playback need no
    // network, and the next launch retries the connect.
    if provider_configured && !locked && !connected_during_unlock {
        runtime.block_on(library_manager.attach_and_start_sync_at_startup());
    }

    // Forward the sync loop's row changes + errors as library/UI events: the
    // synced DB is what drives the UI on every platform.
    library_manager.start();

    // The in-core cpal/ffmpeg audio engine. cpal drives the sink on desktop and
    // iOS, AAudio on Android.
    let playback_handle = library_manager.start_playback_service(
        runtime.handle().clone(),
        position_update_interval_ms,
        restore_playback,
    );

    // The import pipeline (scanning, transcoding, identify) is desktop-only.
    // Mobile is a sync/playback client, so its `AppServices` carries just the
    // library manager and the in-core player.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let app_services = {
        let import_handle = runtime
            .block_on(library_manager.start_import_service(runtime.handle().clone()))
            .map_err(|error| BootstrapError::Database(error.to_string()))?;

        AppServices::new(library_manager, playback_handle, import_handle)
    };

    #[cfg(any(target_os = "ios", target_os = "android"))]
    let app_services = AppServices::new(library_manager, playback_handle);

    info!("Application services initialized for library '{library_id}'");
    diagnostics.event(TelemetryEvent::AppStarted {});

    let ui_event_bus = UiEventBus::new();
    ui_event_bus.wire(&app_services, runtime.handle());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    app_services
        .import_scan_watched_folders()
        .map_err(|error| BootstrapError::Database(error.to_string()))?;

    let owner = compose(app_services, ui_event_bus, runtime)?;

    // The durable active-library pointer names the library the user last actually
    // landed in, so launch ordering (discovery sorts active-first) and
    // forget_library's pointer check both refer to a library that opens. It
    // advances only over a fully-realized open: written after the frontend owner
    // has started, and never for a locked library — cancelling the unlock screen
    // must leave the previously-active library in charge. A successful unlock
    // re-runs bootstrap unlocked and advances the pointer then.
    if advance_active_pointer {
        config_handle
            .config()
            .save_active_library()
            .map_err(|e| BootstrapError::Config(e.to_string()))?;
    }

    Ok(owner)
}
