//! Reading a track's stored audio for a decode, for each of its users:
//! playback, a save or export, import's loudness pass and a transcode. The
//! readers fill a sparse buffer from a release file or a local path, the
//! decode window says what of the buffers a decode reads, and a failed read
//! is a [`TrackReadError`].

pub mod data_source;
mod decode;
mod preview_target;
mod read_error;
mod storage_wait;
// Whole-release passes (import loudness, release saves) are desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod track_sources;

pub(crate) use decode::{SegmentDecodeParams, StreamDecodeParams};
pub use preview_target::PreviewTarget;
pub use read_error::TrackReadError;
