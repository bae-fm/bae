//! The services that run while a library is open, started once over its
//! opened store and manager. The bootstrap and every test that starts them
//! call this, so tests run the app's wiring.

use crate::AppServices;
use bae_db::Database;
use bae_diagnostics::Diagnostics;
#[cfg(desktop)]
use bae_library::CancellationToken;
use bae_library::{LibraryError, LibraryManager};
use bae_store::DbError;
use bae_store::Store;
#[cfg(desktop)]
use coven::ClockRef;
use coven::IdSourceRef;

/// Where playback plays.
pub enum PlaybackOutput {
    /// The system's audio output, shared with other apps as `AudioFocus`
    /// says.
    System(bae_playback::AudioFocus),
    /// An output a test supplies: every output the service opens, the main
    /// player's and any preview's, comes from it.
    #[cfg(any(test, feature = "test-utils"))]
    Device(Box<dyn bae_playback::AudioOutputDevice>),
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
    store: Store,
    records: bae_records::Records,
    database: Database,
    #[cfg(desktop)] catalogs: bae_import::CatalogSessions,
    #[cfg(desktop)] closing: CancellationToken,
    #[cfg(desktop)] clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    #[cfg(desktop)] remote_images: bae_catalogs::remote_image::RemoteImageCache,
    position_update_interval_ms: u32,
    restore_playback: bool,
    playback_clock: bae_playback::PlaybackClockRef,
    playback_output: PlaybackOutput,
) -> Result<AppServices, ServicesStartError> {
    manager.settle_playback_queue(restore_playback).await?;
    let queue_tables = bae_playback::tables::PlaybackQueueTables::new(store.clone());
    let now_row = bae_playback::tables::PlaybackNowRow::new(store.clone());
    let timeline_row = bae_playback::tables::PlaybackTimelineRow::new(store.clone());
    let views = bae_views::LibraryViews::new(store.clone());
    // No discovery outlives the process that ran it: the cast picker's list
    // starts empty, before anything that writes or reads it can.
    let cast_picker = bae_playback::renderer::CastPicker::new(store.clone());
    cast_picker.clear().await?;
    let playback = match playback_output {
        PlaybackOutput::System(audio_focus) => bae_playback::PlaybackService::start(
            store.clone(),
            records.clone(),
            database.clone(),
            ids.clone(),
            queue_tables,
            now_row,
            timeline_row,
            diagnostics.clone(),
            runtime_handle.clone(),
            position_update_interval_ms,
            restore_playback,
            playback_clock,
            audio_focus,
        ),
        #[cfg(any(test, feature = "test-utils"))]
        PlaybackOutput::Device(device) => bae_playback::PlaybackService::start_with_audio_device(
            store.clone(),
            records.clone(),
            database.clone(),
            ids.clone(),
            queue_tables,
            now_row,
            timeline_row,
            diagnostics.clone(),
            runtime_handle.clone(),
            position_update_interval_ms,
            restore_playback,
            device,
            playback_clock,
        ),
    };
    // The import pipeline (scanning, transcoding, identify) is desktop-only.
    // Mobile is a sync/playback client, so its `AppServices` carries just the
    // library manager and the in-core player.
    #[cfg(desktop)]
    {
        let import = bae_import::ImportService::start(
            runtime_handle,
            store.clone(),
            records,
            catalogs,
            remote_images,
            diagnostics,
            bae_import::CandidatePreparations::new(store),
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
    Import(#[from] bae_import::ImportError),
}
