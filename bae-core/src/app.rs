//! Composition root: builds and starts the whole application for a library and
//! hands back a [`RunningApp`]. The frontends reach it through the uniffi bridge
//! (`bae-bridge`, via its `AppHandle`). One place opens the DB, unlocks
//! encryption, starts sync, playback, and (on desktop) the import/identify/
//! extraction services, and wires the UI event bus — no per-frontend copy to
//! drift.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::info;

use crate::config::{Config, ConfigHandle};
use crate::diagnostics::{AnomalyKind, Diagnostics, TelemetryEvent};
use crate::keys::StoreKeys;
use crate::library::AppServices;
use crate::playback::PlaybackService;
use crate::ui::UiEventBus;
use coven::{ClockRef, SystemClock};
use coven::{IdRef, UuidProvider};

/// A fully constructed, running application: the tokio runtime that owns all
/// background work, the composed service layer, and the UI event bus already
/// wired to every service channel. Each frontend wraps this in its own handle.
pub struct RunningApp {
    pub runtime: tokio::runtime::Runtime,
    pub services: AppServices,
    pub ui_event_bus: UiEventBus,
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
pub fn bootstrap(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<crate::CloudKitOpsRef>,
) -> Result<RunningApp, BootstrapError> {
    bootstrap_on_thread(
        BootstrapTarget::RegisteredId(library_id),
        position_update_interval_ms,
        restore_playback,
        diagnostics,
        cloudkit_ops,
    )
}

pub fn bootstrap_library_path(
    library_path: PathBuf,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
) -> Result<RunningApp, BootstrapError> {
    bootstrap_on_thread(
        BootstrapTarget::LibraryPath(library_path),
        position_update_interval_ms,
        restore_playback,
        diagnostics,
        None,
    )
}

fn bootstrap_on_thread(
    target: BootstrapTarget,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<crate::CloudKitOpsRef>,
) -> Result<RunningApp, BootstrapError> {
    // Building the sync manager and `block_on`-ing the async setup uses a deep
    // stack, especially in debug builds. Callers may invoke us from small-stack
    // threads (Swift cooperative Tasks, Android coroutine workers; ~0.5 MB), which
    // overflow there — so run the whole thing on a thread with a large stack.
    std::thread::Builder::new()
        .name("bae-bootstrap".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            bootstrap_inner(
                target,
                position_update_interval_ms,
                restore_playback,
                diagnostics,
                cloudkit_ops,
            )
        })
        .expect("spawn bae-bootstrap thread")
        .join()
        .expect("bae-bootstrap thread panicked")
}

enum BootstrapTarget {
    RegisteredId(String),
    LibraryPath(PathBuf),
}

fn bootstrap_inner(
    target: BootstrapTarget,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<crate::CloudKitOpsRef>,
) -> Result<RunningApp, BootstrapError> {
    // The injected wall clock + id source, built before `load_bootstrap_config` so
    // the device-id auto-generation draws from the injected source too.
    let clock: ClockRef = Arc::new(SystemClock);
    let ids: IdRef = Arc::new(UuidProvider);

    let (config, save_active_library) = load_bootstrap_config(target, ids.as_ref())?;
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

    // Dev mode keeps bae's secrets in `BAE_*` env vars; coven's keyring-only
    // StoreKeys can't see those, so bridge them into the keyring it reads.
    // No-op in production.
    crate::config::seed_dev_keyring(&config.store_id);
    let key_service = StoreKeys::new(config.store_id.clone());

    // Whether this library already has its key on this device — a returning
    // user with a configured opaque provider. A local-only library has no key
    // and needs none; the master key is established lazily, only when a
    // provider is actually connected (`ensure_sync_manager_and_start`). coven
    // resolves the actual cipher from the master-key custody itself once sync
    // connects (below), so this is only the presence check that decides
    // whether to attempt that connect at all. The stamper above is already in
    // place regardless, so local imports write synced rows without any key.
    //
    // The locked case: encryption was set up but the keyring lacks the key on
    // this device (OS keychain wiped, fresh install with config preserved).
    // Attempting to connect would mint a new key over cloud data that key
    // never sealed, so we leave sync unbuilt and let the caller prompt the
    // user to unlock.
    let key_established = config.encryption_key_stored
        && match key_service.get_encryption_key() {
            Ok(Some(_)) => true,
            Ok(None) => {
                tracing::warn!(
                    "encryption key marked stored but not found in keyring; deferring sync until unlocked"
                );
                diagnostics.event(TelemetryEvent::Anomaly {
                    kind: AnomalyKind::EncryptionKeyMissing,
                });
                false
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to read encryption key from keyring; deferring sync until unlocked"
                );
                diagnostics.event(TelemetryEvent::Anomaly {
                    kind: AnomalyKind::EncryptionKeyMissing,
                });
                false
            }
        };

    // Locked: encryption was set up but this device's keyring lacks the key.
    // Bootstrap still completes (sync deferred); the caller diverts to an unlock
    // screen the user may cancel without ever entering this library.
    let locked = config.encryption_key_stored && !key_established;
    let advance_active_pointer = save_active_library && !locked;

    // A browsable home (a provider is configured but the home is stored in the
    // clear) has no key, so the opaque/locked resolution above leaves
    // `key_established` false. It still needs a keyless sync manager built at
    // startup so a returning user resumes syncing.
    let cloud_home_is_browsable =
        config.cloud_home.provider.is_some() && config.cloud_home.storage.is_browsable();

    let config_handle = Arc::new(ConfigHandle::new(config));

    let library_manager = crate::library::LibraryManager::open(
        Arc::clone(&config_handle),
        key_service.clone(),
        Arc::clone(&clock),
        ids,
        diagnostics.clone(),
        runtime.handle().clone(),
        cloudkit_ops,
    )
    .map_err(|e| BootstrapError::Database(format!("Failed to open database: {e}")))?;

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
    if key_established || cloud_home_is_browsable {
        runtime
            .block_on(library_manager.attach_and_start_sync())
            .map_err(|e| BootstrapError::Database(e.to_string()))?;
    }

    // Forward the sync loop's row changes + errors as library/UI events: the
    // synced DB is what drives the UI on every platform.
    library_manager.start();

    // The in-core cpal/ffmpeg audio engine. cpal drives the sink on desktop and
    // iOS, AAudio on Android.
    let playback_handle = PlaybackService::start(
        library_manager.clone(),
        runtime.handle().clone(),
        position_update_interval_ms,
        restore_playback,
    );

    // The import pipeline (scanning, transcoding, identify) is desktop-only.
    // Mobile is a sync/playback client, so its `AppServices` carries just the
    // library manager and the in-core player.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let app_services = {
        let cover_art_archive = crate::import::cover_art::CoverArtArchiveClient::new();
        let import_handle = crate::import::ImportService::start(
            runtime.handle().clone(),
            library_manager.clone(),
            cover_art_archive.clone(),
        );

        let identify_handle = crate::identify::IdentifyServiceHandle::new(
            library_manager.clone(),
            runtime.handle().clone(),
            import_handle.event_tx.clone(),
            cover_art_archive,
        );

        let extraction_handle = crate::signals::ExtractionService::start(
            runtime.handle().clone(),
            import_handle.event_tx.clone(),
            library_manager.clone(),
        );

        AppServices::new(
            library_manager,
            playback_handle,
            import_handle,
            identify_handle,
            extraction_handle,
        )
    };

    #[cfg(any(target_os = "ios", target_os = "android"))]
    let app_services = AppServices::new(library_manager, playback_handle);

    info!("RunningApp initialized for library '{library_id}'");
    diagnostics.event(TelemetryEvent::AppStarted {});

    let ui_event_bus = UiEventBus::new();
    ui_event_bus.wire(&app_services, runtime.handle());

    // The durable active-library pointer names the library the user last actually
    // landed in, so launch ordering (discovery sorts active-first), the CLI's
    // active-library selector, and forget_library's pointer check all refer to a
    // library that opens. It advances only over a fully-realized open: written
    // after every fallible step above has succeeded, and never for a locked
    // library — cancelling the unlock screen must leave the previously-active
    // library in charge. A successful unlock re-runs bootstrap unlocked and
    // advances the pointer then.
    if advance_active_pointer {
        config_handle
            .config()
            .save_active_library()
            .map_err(|e| BootstrapError::Config(e.to_string()))?;
    }

    Ok(RunningApp {
        runtime,
        services: app_services,
        ui_event_bus,
    })
}

/// Load the config for a bootstrap target. The returned bool is whether this
/// open should advance the active-library pointer once fully realized:
/// registered-id targets yes (they name a device library), explicit-path targets
/// no.
fn load_bootstrap_config(
    target: BootstrapTarget,
    ids: &dyn coven::IdProvider,
) -> Result<(Config, bool), BootstrapError> {
    match target {
        BootstrapTarget::RegisteredId(library_id) => {
            let config =
                Config::load_registered_library(&library_id, ids).map_err(|e| match e {
                    coven::ConfigError::Config(_) => {
                        BootstrapError::LibraryNotFound(library_id.clone())
                    }
                    other => BootstrapError::Config(other.to_string()),
                })?;
            Ok((config, true))
        }
        BootstrapTarget::LibraryPath(path) => Config::load_from_library_path(path, ids)
            .map(|config| (config, false))
            .map_err(|e| BootstrapError::Config(e.to_string())),
    }
}
