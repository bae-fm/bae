mod cpal_output;
pub mod data_source;
mod error;
mod pcm_source;
pub mod progress;
pub mod service;
pub mod sparse_buffer;
pub mod streaming_source;
pub mod track_loader;

pub use error::PlaybackError;
pub use pcm_source::PcmSource;
pub use progress::PlaybackProgress;
pub use service::{PlaybackHandle, PlaybackService, PlaybackState};
pub use sparse_buffer::SharedSparseBuffer;
pub use streaming_source::{create_streaming_pair, StreamingPcmSink, StreamingPcmSource};

#[cfg(test)]
pub use streaming_source::create_streaming_pair_with_capacity;
