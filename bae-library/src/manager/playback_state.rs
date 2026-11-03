//! Reading a track's stored audio for a caller outside playback, and settling
//! the playback queue as the library opens, for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// A reader of `file_id`'s stored audio start to finish, for work nobody
    /// listens to while it reads.
    pub fn create_audio_reader(
        &self,
        file_id: &str,
    ) -> Box<dyn bae_track_audio::data_source::AudioDataReader> {
        Box::new(bae_track_audio::data_source::CovenFileReader::new(
            self.database.clone(),
            file_id.to_string(),
            self.diagnostics.clone(),
        ))
    }

    /// `file_id`'s bytes, `size` long, read into a buffer from the start,
    /// which a failed read is logged against.
    pub fn open_release_file_stream(
        &self,
        file_id: &str,
        size: u64,
    ) -> bae_audio::sparse_buffer::SharedSparseBuffer {
        let buffer = bae_audio::sparse_buffer::create_sparse_buffer(size);
        let reader = self.create_audio_reader(file_id);
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
    /// and seeks by. One stream per file the segments read; they close when
    /// the returned decode is dropped.
    pub fn open_track_decode(&self, audio: &bae_db::ResolvedTrackAudio) -> crate::TrackDecode {
        use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};

        let mut streams: std::collections::HashMap<
            &str,
            bae_audio::sparse_buffer::SharedSparseBuffer,
        > = std::collections::HashMap::new();
        let segments = audio
            .segments
            .iter()
            .map(|segment| {
                let buffer = streams
                    .entry(segment.file_id.as_str())
                    .or_insert_with(|| {
                        self.open_release_file_stream(&segment.file_id, segment.file_size)
                    })
                    .clone();
                SegmentDecodeParams::new(buffer, segment.span, 0)
            })
            .collect();
        crate::TrackDecode::new(
            StreamDecodeParams::new(
                segments,
                bae_audio::byte_jump_keeps_place(&audio.content_type),
                audio.generated_pregap_frames(),
                0,
            ),
            audio.sample_rate.get(),
            audio.channels.get(),
            self.diagnostics.clone(),
        )
    }

    /// Settle the playback queue for starting playback: kept when it will be
    /// resumed (`resume`, and a resume row stored), emptied with the resume
    /// row otherwise.
    pub async fn settle_playback_queue(&self, resume: bool) -> Result<(), LibraryError> {
        Ok(bae_playback::tables::settle_playback_queue(&self.database, resume).await?)
    }
}

#[cfg(test)]
#[path = "playback_state_tests.rs"]
mod tests;
