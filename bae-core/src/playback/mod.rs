#[cfg(target_os = "android")]
pub mod aaudio_output;
pub mod audio_output;
#[cfg(not(target_os = "android"))]
pub mod cpal_output;
pub mod data_source;
mod decoded_pcm;
mod error;
pub mod format;
pub mod progress;
mod queue;
mod repeat_mode;
pub mod service;
pub mod source;
pub mod sparse_buffer;
pub mod track_stream;

#[cfg(feature = "test-utils")]
pub use audio_output::{wait_for_samples, CaptureAudioOutput};
pub use audio_output::{
    AudioError, AudioOutput, AudioState, AudioStream, CompletionEvent, PositionEvent,
};
pub use decoded_pcm::DecodedPcm;
pub use error::PlaybackError;
pub use progress::{PlaybackProgress, PreviewState};
pub use queue::{NextTrack, PlaybackQueue, PreviousAction, QueueEntry, QueueEntryId};
pub use repeat_mode::RepeatMode;
pub use service::{
    LoadingTrack, PlaybackHandle, PlaybackService, PlaybackSnapshot, PlaybackState,
    PlaybackTrackInfo, PositionDisplay,
};
pub use source::{PlaybackSource, TrackCrossing, TrackFmt};
pub use sparse_buffer::SharedSparseBuffer;
pub use track_stream::{create_track_stream_pair, TrackSink, TrackStream};

#[cfg(test)]
pub use track_stream::create_track_stream_pair_with_capacity;
