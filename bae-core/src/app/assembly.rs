//! The open library's parts, each built once from its opened store. The
//! bootstrap and every test that opens a library call these, so tests run
//! the app's wiring. The caller opens the store and builds what both halves
//! share — the catalog sessions and the closing token — and passes them to
//! each by name.

use crate::library::{AppServices, CancellationToken, LibraryError, LibraryManager};
use bae_db::{Database, DbError};
use bae_diagnostics::Diagnostics;
use coven::{ClockRef, IdSourceRef};

/// Where playback plays.
pub enum PlaybackOutput {
    /// The system's audio output, shared with other apps as `AudioFocus`
    /// says.
    System(crate::playback::AudioFocus),
    /// An output a test supplies: every output the service opens, the main
    /// player's and any preview's, comes from it.
    #[cfg(any(test, feature = "test-utils"))]
    Device(Box<dyn crate::playback::AudioOutputDevice>),
}

/// Open the library manager over the library's store, with the services it
/// drives built from that store. `closing` ends its tasks when it closes.
pub async fn open_library_manager(
    database: Database,
    #[cfg(desktop)] catalogs: crate::import::CatalogSessions,
    closing: CancellationToken,
    app_dir: bae_app_dir::AppDir,
    clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    remote_images: bae_catalogs::remote_image::RemoteImageCache,
) -> Result<LibraryManager, DbError> {
    let sync =
        crate::library::sync_controller::SyncController::new(database.clone(), diagnostics.clone());
    let downloads = crate::library::Downloads::new(database.clone());
    #[cfg(desktop)]
    let outputs = crate::library::Outputs::new(database.clone());
    LibraryManager::open(
        database,
        app_dir,
        clock,
        ids,
        diagnostics,
        runtime_handle,
        remote_images,
        #[cfg(desktop)]
        catalogs,
        sync,
        downloads,
        #[cfg(desktop)]
        outputs,
        closing,
    )
    .await
}

/// Start the services that run while the library is open — playback, and on
/// desktop the import service and queue-wide identification — and the one
/// owner of the opened library over them. `restore_playback` ("Restore on
/// launch") keeps the stored queue and resumes its track; off, the queue and
/// the resume point are forgotten before playback starts, so no screen reads
/// a queue the library is about to drop. `playback_clock` is playback's time
/// source for side-pause countdowns.
pub async fn start_library_services(
    manager: LibraryManager,
    database: Database,
    #[cfg(desktop)] catalogs: crate::import::CatalogSessions,
    #[cfg(desktop)] closing: CancellationToken,
    #[cfg(desktop)] clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    #[cfg(desktop)] remote_images: bae_catalogs::remote_image::RemoteImageCache,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_clock: crate::playback::PlaybackClockRef,
    playback_output: PlaybackOutput,
) -> Result<AppServices, ServicesStartError> {
    manager.settle_playback_queue(restore_playback).await?;
    let queue_tables = crate::playback::tables::PlaybackQueueTables::new(database.clone());
    let now_row = crate::playback::tables::PlaybackNowRow::new(database.clone());
    let timeline_row = crate::playback::tables::PlaybackTimelineRow::new(database.clone());
    let track_deletions = manager.subscribe_track_deletions();
    let views = crate::library::views::LibraryViews::new(database.clone());
    // No discovery outlives the process that ran it: the cast picker's list
    // starts empty, before anything that writes or reads it can.
    let cast_picker = crate::renderer::CastPicker::new(database.clone());
    cast_picker.clear().await?;
    let playback = match playback_output {
        PlaybackOutput::System(audio_focus) => crate::playback::PlaybackService::start(
            database.clone(),
            ids.clone(),
            queue_tables,
            now_row,
            timeline_row,
            track_deletions,
            diagnostics.clone(),
            runtime_handle.clone(),
            position_update_interval_ms,
            restore_playback,
            playback_clock,
            audio_focus,
        ),
        #[cfg(any(test, feature = "test-utils"))]
        PlaybackOutput::Device(device) => {
            crate::playback::PlaybackService::start_with_audio_device(
                database.clone(),
                ids.clone(),
                queue_tables,
                now_row,
                timeline_row,
                track_deletions,
                diagnostics.clone(),
                runtime_handle.clone(),
                position_update_interval_ms,
                restore_playback,
                device,
                playback_clock,
            )
        }
    };
    // The import pipeline (scanning, transcoding, identify) is desktop-only.
    // Mobile is a sync/playback client, so its `AppServices` carries just the
    // library manager and the in-core player.
    #[cfg(desktop)]
    {
        let import = crate::import::ImportService::start(
            runtime_handle,
            database.clone(),
            catalogs,
            remote_images,
            diagnostics,
            crate::import::CandidatePreparations::new(database),
            clock,
            ids,
            closing.child_token(),
        )
        .await?;
        // Queue-wide identification runs whether or not anyone has the Import
        // section open: opening a view is not what starts it.
        let identification = import.start_identification_queue();
        Ok(AppServices::new(manager, views, playback, cast_picker, import, identification))
    }
    #[cfg(not(desktop))]
    Ok(AppServices::new(manager, views, playback, cast_picker))
}

/// Why the library's services did not start.
#[derive(Debug, thiserror::Error)]
pub enum ServicesStartError {
    /// Settling the playback queue failed.
    #[error(transparent)]
    Library(#[from] LibraryError),
    /// Emptying the cast picker's list failed.
    #[error(transparent)]
    Database(#[from] DbError),
    /// The import service did not start.
    #[cfg(desktop)]
    #[error(transparent)]
    Import(#[from] crate::import::ImportError),
}
