//! Reading a track's stored audio for a decode, for each of its users:
//! playback, a save or export, import's loudness pass and a transcode. The
//! readers fill a sparse buffer from a release file or a local path, the
//! decode window says what of the buffers a decode reads, and a failed read
//! is a [`TrackReadError`].

pub mod data_source;
mod decode;
mod preview_target;
mod read_error;
// Only import's verifying decode takes scrub points, and the import pipeline
// is desktop-only.
#[cfg(desktop)]
pub mod scrub_points;
mod storage_wait;
// Whole-release passes (import loudness, release saves) are desktop-only.
#[cfg(desktop)]
pub mod track_sources;

pub use decode::{SegmentDecodeParams, SegmentStart, StreamDecodeParams};
pub use preview_target::PreviewTarget;
pub use read_error::TrackReadError;
pub use storage_wait::StorageWaitWatch;
