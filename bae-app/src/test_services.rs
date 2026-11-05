//! A test library's services, started through the app's own step, for the
//! tests of this crate and the crates above it.

use crate::{AppServices, ServicesStartError};
use bae_library::test_libraries::TestLibrary;
use std::sync::Arc;

/// `library`'s services, started through the app's own step, with playback on
/// a device that plays nothing and no stored queue restored.
pub async fn start_services(library: &TestLibrary) -> Result<AppServices, ServicesStartError> {
    start_services_on(library, Box::new(bae_playback::audio_output::FailingAudioDevice)).await
}

/// [`start_services`] with playback on `device`.
pub async fn start_services_on(
    library: &TestLibrary,
    device: Box<dyn bae_playback::AudioOutputDevice>,
) -> Result<AppServices, ServicesStartError> {
    crate::start_library_services(
        bae_library::LibraryManager::clone(library),
        library.store().clone(),
        library.database().clone(),
        library.catalogs().clone(),
        library.closing().clone(),
        library.clock().clone(),
        library.ids().clone(),
        library.diagnostics().clone(),
        tokio::runtime::Handle::current(),
        library.remote_images().clone(),
        50,
        false,
        Arc::new(bae_playback::WallPlaybackClock::new(library.clock().clone())),
        crate::PlaybackOutput::Device(device),
    )
    .await
}

/// [`bae_library::test_libraries::multi_thread_runtime`] with a fresh library's
/// [`AppServices`] already built on it. The `TempDir` owns the library's
/// files, so it must outlive both.
pub fn runtime_with_services() -> (tokio::runtime::Runtime, AppServices, tempfile::TempDir) {
    let runtime = bae_library::test_libraries::multi_thread_runtime();
    let (library, tmp) = bae_library::test_libraries::setup_fresh_library(&runtime);
    let services = runtime.block_on(async move {
        start_services(&library).await.expect("the test library's services start")
    });
    (runtime, services, tmp)
}
