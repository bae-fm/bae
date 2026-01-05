mod cpal_output;
mod error;
mod pcm_source;
pub mod progress;
pub mod service;
pub mod streaming_buffer;
pub mod streaming_source;
pub mod track_loader;
pub use error::PlaybackError;
pub use pcm_source::PcmSource;
pub use progress::PlaybackProgress;
pub use service::{PlaybackHandle, PlaybackService, PlaybackState};
// Re-export streaming types for use in audio_codec and future PlaybackService integration
#[allow(unused_imports)]
pub use streaming_buffer::{create_streaming_buffer, SharedStreamingBuffer, StreamingAudioBuffer};
#[allow(unused_imports)]
pub use streaming_source::{
    create_streaming_pair, create_streaming_pair_with_capacity, StreamingPcmSink,
    StreamingPcmSource, StreamingState,
};
#[cfg(feature = "test-utils")]
#[allow(unused_imports)]
pub use track_loader::load_track_audio;
