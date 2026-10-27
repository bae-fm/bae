//! Device-local playback-state persistence for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Start playback over the system's audio output. `audio_focus` is how
    /// the platform shares the speaker between apps. `restore_playback`
    /// ("Restore on launch") keeps the stored queue and resumes its track;
    /// off, the queue and the resume point are forgotten before the service
    /// starts, so no screen reads a queue the library is about to drop.
    pub async fn start_playback_service(
        &self,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_focus: crate::playback::AudioFocus,
    ) -> Result<crate::playback::PlaybackHandle, LibraryError> {
        self.settle_playback_queue(restore_playback).await?;
        Ok(crate::playback::PlaybackService::start(
            self.clone(),
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            self.playback_clock(),
            audio_focus,
        ))
    }

    /// The playback service's time source over this library's injected wall
    /// clock.
    fn playback_clock(&self) -> crate::playback::PlaybackClockRef {
        Arc::new(crate::playback::WallPlaybackClock::new(self.clock.clone()))
    }

    /// Start playback over a caller-supplied audio device instead of the
    /// system's: every output the service opens — the main player's and any
    /// preview's — comes from it, so a test drives playback without touching
    /// audio hardware.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn start_playback_service_with_audio_device(
        &self,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_device: Box<dyn crate::playback::audio_output::AudioOutputDevice>,
    ) -> Result<crate::playback::PlaybackHandle, LibraryError> {
        self.start_playback_service_with_clock(
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            audio_device,
            self.playback_clock(),
        )
        .await
    }

    /// [`Self::start_playback_service_with_audio_device`] with the service's
    /// time source supplied too, so a test decides when a side-pause countdown
    /// runs out.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn start_playback_service_with_clock(
        &self,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_device: Box<dyn crate::playback::audio_output::AudioOutputDevice>,
        clock: crate::playback::PlaybackClockRef,
    ) -> Result<crate::playback::PlaybackHandle, LibraryError> {
        self.settle_playback_queue(restore_playback).await?;
        Ok(crate::playback::PlaybackService::start_with_audio_device(
            self.clone(),
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            audio_device,
            clock,
        ))
    }

    pub(crate) fn create_audio_reader(
        &self,
        file_id: &str,
        arbiter: Arc<crate::playback::data_source::FetchArbiter>,
        prefetch_byte: Option<u64>,
        prefetch_file_end: bool,
    ) -> Box<dyn crate::playback::data_source::AudioDataReader> {
        Box::new(crate::playback::data_source::CovenFileReader::new(
            self.database.clone(),
            file_id.to_string(),
            arbiter,
            prefetch_byte,
            prefetch_file_end,
        ))
    }

    /// Write the device-local playback-state row. Propagates the DB error so the
    /// caller can distinguish a write failure from a stored absence; the resume
    /// row is a device-local cache, so the call site logs and continues rather
    /// than treating a failed write as fatal to playback.
    pub async fn save_playback_state(
        &self,
        state: &crate::db::DbPlaybackState,
    ) -> Result<(), LibraryError> {
        Ok(self.database.save_playback_state(state).await?)
    }

    /// Read the device-local playback-state row (kept, not deleted — it's
    /// overwritten on the next playback change and cleared on stop), `None`
    /// when none is stored. An `Err` is a read failure, kept distinct from an
    /// absent row so the caller doesn't silently start fresh on a DB error.
    pub async fn load_playback_state(
        &self,
    ) -> Result<Option<crate::db::DbPlaybackState>, LibraryError> {
        Ok(self.database.load_playback_state().await?)
    }

    /// Settle the playback queue for starting playback: kept when it will be
    /// resumed (`resume`, and a resume row stored), emptied with the resume
    /// row otherwise.
    async fn settle_playback_queue(&self, resume: bool) -> Result<(), LibraryError> {
        Ok(self.database.settle_playback_queue(resume).await?)
    }

    /// Delete the device-local playback-state row (playback stopped).
    pub async fn clear_playback_state(&self) -> Result<(), LibraryError> {
        Ok(self.database.clear_playback_state().await?)
    }
}
