//! Composition root: builds and starts the whole application for a library and
//! hands back a [`RunningApp`]. Shared by the Rust frontends that wrap it — the
//! uniffi bridge (`bae-bridge`, via its `AppHandle`) and the Windows FFI
//! (`bae-windows-ffi`). Keeping the wiring here means there is one place that
//! opens the DB, unlocks encryption, starts sync, playback, and (on desktop)
//! the import/identify/extraction services, and wires the UI event bus — no
//! per-frontend duplicate to drift.

use std::sync::Arc;

use tracing::info;

use crate::clock::{ClockRef, SystemClock};
use crate::config::{Config, ConfigHandle};
use crate::db::Database;
use crate::id_provider::{IdRef, UuidProvider};
use crate::keys::KeyService;
use crate::library::AppServices;
use crate::playback::PlaybackService;
use crate::ui::UiEventBus;

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
pub fn bootstrap(
    library_id: String,
    position_update_interval_ms: u32,
) -> Result<RunningApp, BootstrapError> {
    // Building the sync manager (loading keys, opening the synced DB) and
    // `block_on`-ing the async setup uses a deep stack — especially in debug
    // builds, where async state machines aren't collapsed. Callers may invoke us
    // from small-stack threads (Swift cooperative Tasks, Android coroutine
    // workers; ~0.5 MB), which overflow and crash ("Thread stack size
    // exceeded"). Run the whole thing on a thread with a generous stack and hand
    // the result back.
    std::thread::Builder::new()
        .name("bae-bootstrap".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || bootstrap_inner(library_id, position_update_interval_ms))
        .expect("spawn bae-bootstrap thread")
        .join()
        .expect("bae-bootstrap thread panicked")
}

fn bootstrap_inner(
    library_id: String,
    position_update_interval_ms: u32,
) -> Result<RunningApp, BootstrapError> {
    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or(BootstrapError::LibraryNotFound(library_id.clone()))?;

    let home_dir = dirs::home_dir()
        .ok_or_else(|| BootstrapError::Config("Failed to get home directory".to_string()))?;
    let bae_dir = home_dir.join(".bae");
    std::fs::create_dir_all(&bae_dir)
        .map_err(|e| BootstrapError::Config(format!("Failed to create .bae directory: {e}")))?;
    std::fs::write(bae_dir.join("active-library"), &lib_info.id).map_err(|e| {
        BootstrapError::Config(format!("Failed to write active-library pointer: {e}"))
    })?;

    // Composition root for the injected wall clock + id source. Production wires
    // the real implementations; both are passed down to the data layer. Built
    // before `Config::load` so the device-id auto-gen reads from the injected
    // source too.
    let clock: ClockRef = Arc::new(SystemClock);
    let ids: IdRef = Arc::new(UuidProvider);

    let config = Config::load(ids.as_ref());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        // The sync cycle (snapshot creation, changeset apply) runs on these
        // workers and is deep in debug builds; the default 2 MB stack can
        // overflow. Give them room.
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .map_err(|e| BootstrapError::Internal(format!("Failed to create runtime: {e}")))?;

    crate::audio_codec::init();

    // Open the database. coven owns the connection, runs its bookkeeping
    // migration plus bae's schema, seeds the `_updated_at` register off the rows
    // on disk, attaches the capture session over the synced-table set, and hands
    // back the non-optional stamper every synced-row write binds. It's built from
    // the device id, the database, and the synced-table set alone — independent of
    // encryption, keys, or a cloud provider — so a fresh local-only library writes
    // stamped synced rows without minting an encryption key or standing up a
    // `SyncManager`.
    let db_path = config.library_dir.db_path();
    let database = runtime
        .block_on(Database::new(
            db_path.to_str().expect("database path must be valid UTF-8"),
            Arc::clone(&clock),
            config.device_id.clone(),
            crate::sync::synced_tables(),
        ))
        .map_err(|e| BootstrapError::Database(format!("Failed to open database: {e}")))?;

    // Dev mode keeps bae's secrets in `BAE_*` env vars; coven's keyring-only
    // KeyService can't see those, so bridge them into the keyring it reads.
    // No-op in production.
    crate::config::seed_dev_keyring(&config.library_id);
    let key_service = KeyService::new(config.library_id.clone());

    // Resolve the encryption service only when this library already has a key on
    // this device — a returning user with a configured provider. A local-only
    // library has no key and needs none; encryption is created lazily, only when a
    // provider is actually connected (`ensure_sync_manager_and_start`). The stamper
    // above is already in place regardless, so local imports write synced rows
    // without any key.
    //
    // The locked case: encryption was set up but the keyring lacks the key on this
    // device (OS keychain wiped, fresh install with config preserved). Minting a
    // new key would orphan the cloud data, so we leave sync unbuilt and let the
    // caller prompt the user to unlock.
    let pending_enc = if config.encryption_key_stored {
        match key_service.get_encryption_key() {
            Ok(Some(key_hex)) => Some(
                crate::encryption::EncryptionService::new(&key_hex).map_err(|e| {
                    BootstrapError::Config(format!("Failed to initialize encryption: {e}"))
                })?,
            ),
            Ok(None) => {
                tracing::warn!(
                    "encryption key marked stored but not found in keyring; deferring sync until unlocked"
                );
                None
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to read encryption key from keyring; deferring sync until unlocked"
                );
                None
            }
        }
    } else {
        None
    };

    // A browsable home (a provider is configured but the home is stored in the
    // clear) has no key, so the opaque/locked resolution above leaves
    // `pending_enc` None. It still needs a keyless sync manager built at startup
    // so a returning user resumes syncing.
    let cloud_home_is_browsable =
        config.cloud_home.provider.is_some() && config.cloud_home.storage.is_browsable();

    let config_handle = Arc::new(ConfigHandle::new(config));

    let library_manager = crate::library::LibraryManager::new(
        database.clone(),
        config_handle.config().library_dir.clone(),
        Arc::clone(&config_handle),
        key_service.clone(),
        Arc::clone(&clock),
        ids,
        runtime.handle().clone(),
        None,
    );

    // Configure coven's per-namespace cache budgets (device-local, idempotent):
    // the bulk for audio, a small reserved slice each for covers / artist images.
    runtime
        .block_on(library_manager.configure_cache_budgets())
        .map_err(|e| BootstrapError::Database(e.to_string()))?;

    // Now that the manager owns the outbox in-flight set and event channel, build
    // and start the sync manager (if unlocked) so the upload observer shares
    // them. Must precede `library_manager.start()`, which subscribes to the sync
    // loop. The database already holds the seeded `_updated_at` stamper from
    // `open` above; building the manager here only wires the cloud loop.
    if let Some(enc) = pending_enc {
        // Opaque home, unlocked: build the sync manager with the library key.
        runtime
            .block_on(library_manager.attach_and_start_sync(Some(enc)))
            .map_err(BootstrapError::Database)?;
    } else if cloud_home_is_browsable {
        // Browsable home: no key, build a keyless sync manager (an opaque-but-
        // locked home stays unbuilt above, awaiting unlock).
        runtime
            .block_on(library_manager.attach_and_start_sync(None))
            .map_err(BootstrapError::Database)?;
    }

    // Forward the sync loop's row changes + errors as library/UI events. This is
    // the heart of "sync from cloud" on every platform: the synced DB drives the
    // UI.
    library_manager.start();

    // The in-core cpal/ffmpeg audio engine. Runs on every platform: cpal drives
    // the sink on desktop and iOS, AAudio on Android.
    let playback_handle = PlaybackService::start(
        library_manager.clone(),
        runtime.handle().clone(),
        position_update_interval_ms,
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

        let identify_handle = crate::identify::IdentifyService::start(
            library_manager.clone(),
            runtime.handle().clone(),
            import_handle.event_tx.clone(),
            cover_art_archive,
        );

        let extraction_handle = crate::signals::ExtractionService::start(
            runtime.handle().clone(),
            import_handle.event_tx.clone(),
            clock,
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

    let ui_event_bus = UiEventBus::new();
    ui_event_bus.wire(&app_services, runtime.handle());

    Ok(RunningApp {
        runtime,
        services: app_services,
        ui_event_bus,
    })
}
