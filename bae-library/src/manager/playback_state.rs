//! Reading a track's stored audio for a caller outside playback, and settling
//! the playback queue as the library opens, for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// A reader of `file_id`'s stored audio start to finish, for work nobody
    /// listens to while it reads. Its fill opens the file's stream in one
    /// place of `lease`.
    pub fn create_audio_reader(
        &self,
        file_id: &str,
        lease: coven::FileLease,
    ) -> Box<dyn bae_track_audio::data_source::AudioDataReader> {
        Box::new(bae_track_audio::data_source::CovenFileReader::new(
            self.runtime_handle.clone(),
            self.records.clone(),
            file_id.to_string(),
            self.diagnostics.clone(),
            lease,
        ))
    }

    /// `file_id`'s bytes, `size` long, read into a buffer from the start,
    /// which a failed read is logged against. The read holds one place of the
    /// budget of open files until the buffer is dropped.
    pub async fn open_release_file_stream(
        &self,
        file_id: &str,
        size: u64,
    ) -> bae_audio::sparse_buffer::SharedSparseBuffer {
        self.start_release_file_stream(file_id, size, self.records.lease_one().await)
    }

    /// [`Self::open_release_file_stream`] in a place of `lease`.
    fn start_release_file_stream(
        &self,
        file_id: &str,
        size: u64,
        lease: coven::FileLease,
    ) -> bae_audio::sparse_buffer::SharedSparseBuffer {
        let buffer = bae_audio::sparse_buffer::create_sparse_buffer(size);
        let reader = self.create_audio_reader(file_id, lease);
        let file_id = file_id.to_string();
        reader.start_reading(
            buffer.clone(),
            Box::new(move |error| {
                tracing::warn!(file_id, %error, "reading release file failed");
            }),
        );
        buffer
    }

    /// Open `audio`'s stream for one decode start to finish (a transcode): the
    /// track from its first sample, as playback decodes it from its start — the
    /// silence a CUE `PREGAP` generates, then each segment, pregap included,
    /// seeked the way playback seeks it. Positions in the served stream are
    /// then positions in the track's stream, which a remote renderer reports
    /// and seeks by. One stream per file the segments read, each in a place
    /// the decode's lease took from the budget of open files in one wait;
    /// they close when the returned decode is dropped. Fails when the track
    /// reads more files than the budget has places.
    pub async fn open_track_decode(
        &self,
        audio: &bae_records::ResolvedTrackAudio,
    ) -> Result<crate::TrackDecode, LibraryError> {
        let files: std::collections::HashSet<&str> =
            audio.segments.iter().map(|segment| segment.file_id.as_str()).collect();
        let lease = self
            .store
            .file_leases()
            .lease(files.len())
            .await
            .map_err(|error| LibraryError::Internal(error.to_string()))?;
        let mut streams: std::collections::HashMap<
            String,
            bae_audio::sparse_buffer::SharedSparseBuffer,
        > = std::collections::HashMap::new();
        let decode = bae_playback::service::track_stream_decode(audio, |segment| {
            streams
                .entry(segment.file_id.clone())
                .or_insert_with(|| {
                    self.start_release_file_stream(
                        &segment.file_id,
                        segment.file_size,
                        lease.clone(),
                    )
                })
                .clone()
        });
        Ok(crate::TrackDecode::new(
            decode,
            audio.sample_rate.get(),
            audio.channels.get(),
            self.diagnostics.clone(),
        ))
    }

    /// Settle the playback queue for starting playback: kept when it will be
    /// resumed (`resume`, and a resume row stored), emptied with the resume
    /// row otherwise.
    pub async fn settle_playback_queue(&self, resume: bool) -> Result<(), LibraryError> {
        Ok(bae_playback::tables::settle_playback_queue(&self.store, resume).await?)
    }
}

#[cfg(test)]
#[path = "playback_state_tests.rs"]
mod tests;
