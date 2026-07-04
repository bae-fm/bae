#[cfg(target_os = "android")]
pub mod aaudio_output;
pub mod audio_output;
mod context;
#[cfg(not(target_os = "android"))]
pub mod cpal_output;
pub mod data_source;
mod decoded_pcm;
mod error;
pub mod format;
mod persisted;
mod preview_player;
pub mod progress;
mod queue;
mod repeat_mode;
pub mod service;
pub mod source;
pub mod sparse_buffer;
pub mod track_stream;

#[cfg(feature = "test-utils")]
pub use audio_output::{wait_for_samples, CaptureAudioOutput, RealtimeProbeOutput};
pub use audio_output::{
    AudioError, AudioOutput, AudioState, AudioStream, CompletionEvent, PositionEvent,
};
pub use context::{ContextSource, ContextStart, Traversal};
pub use decoded_pcm::DecodedPcm;
pub use error::PlaybackError;
pub use persisted::{repeat_to_str, source_to_str, PersistedPlayback};
pub use progress::{PlaybackProgress, PreviewState};
pub use queue::{
    ContextProjection, ContextSnapshot, NextEntry, PlaybackQueue, PreviousAction, QueueEntry,
    QueueEntryId, QueueSnapshot,
};
pub use repeat_mode::RepeatMode;
pub use service::{
    LoadingTrack, PlaybackHandle, PlaybackPauseReason, PlaybackService, PlaybackSidePausePrompt,
    PlaybackState, PlaybackTrackInfo, PlaybackTrackSide, SIDE_PAUSE_CASSETTE_MESSAGE_KEY,
    SIDE_PAUSE_TITLE_KEY, SIDE_PAUSE_VINYL_MESSAGE_KEY,
};
pub use source::{TrackCrossing, TrackFmt};
pub use sparse_buffer::SharedSparseBuffer;
pub use track_stream::{create_track_stream_pair, TrackSink, TrackStream};

#[cfg(test)]
pub use track_stream::create_track_stream_pair_with_capacity;
