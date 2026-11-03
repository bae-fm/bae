//! Reading a track's stored audio for a caller outside playback, and settling
//! the playback queue as the library opens, for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub(crate) fn create_audio_reader(
        &self,
        file_id: &str,
        arbiter: Arc<bae_track_audio::data_source::FetchArbiter>,
        prefetch_byte: Option<u64>,
        prefetch_file_end: bool,
    ) -> Box<dyn bae_track_audio::data_source::AudioDataReader> {
        Box::new(bae_track_audio::data_source::CovenFileReader::new(
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
        decode: bae_track_audio::StreamDecodeParams,
        sample_rate: u32,
        channels: u32,
    ) -> crate::library::TrackDecode {
        crate::library::TrackDecode::new(decode, sample_rate, channels, self.diagnostics.clone())
    }

    /// Settle the playback queue for starting playback: kept when it will be
    /// resumed (`resume`, and a resume row stored), emptied with the resume
    /// row otherwise.
    pub async fn settle_playback_queue(&self, resume: bool) -> Result<(), LibraryError> {
        Ok(crate::playback::tables::settle_playback_queue(&self.database, resume).await?)
    }
}
