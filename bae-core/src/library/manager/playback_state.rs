//! Device-local playback-state persistence for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub(crate) fn create_audio_reader(
        &self,
        file_id: &str,
        arbiter: Arc<crate::track_audio::data_source::FetchArbiter>,
        prefetch_byte: Option<u64>,
        prefetch_file_end: bool,
    ) -> Box<dyn crate::track_audio::data_source::AudioDataReader> {
        Box::new(crate::track_audio::data_source::CovenFileReader::new(
            self.database.clone(),
            file_id.to_string(),
            arbiter,
            prefetch_byte,
            prefetch_file_end,
            self.diagnostics.clone(),
        ))
    }

    /// `decode`, a track's stored audio as `sample_rate` and `channels`, as
    /// one decode for a caller outside the library (a transcode), reporting a
    /// read of it that panics to the library's diagnostics.
    pub(crate) fn track_decode(
        &self,
        decode: crate::track_audio::StreamDecodeParams,
        sample_rate: u32,
        channels: u32,
    ) -> crate::library::TrackDecode {
        crate::library::TrackDecode::new(decode, sample_rate, channels, self.diagnostics.clone())
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
    pub async fn settle_playback_queue(&self, resume: bool) -> Result<(), LibraryError> {
        Ok(self.database.settle_playback_queue(resume).await?)
    }

    /// Delete the device-local playback-state row (playback stopped).
    pub async fn clear_playback_state(&self) -> Result<(), LibraryError> {
        Ok(self.database.clear_playback_state().await?)
    }
}
