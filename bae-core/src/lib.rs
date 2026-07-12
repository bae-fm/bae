#![deny(unreachable_pub, dead_code)]

pub mod album_detail;
pub mod app;
pub mod audio_codec;
#[doc(hidden)]
pub mod config;
pub mod cue_flac;
pub mod db;
pub mod diagnostics;
pub mod discogs;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod identify;
pub mod import;
pub mod keys;
pub mod library;
pub mod library_name;
// Loudness measurement uses `ebur128`, a desktop-only dependency (import decodes
// audio only on these targets; playback derives the gain from the stored
// measurements with plain arithmetic and needs no ebur128).
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub mod loudness;
pub mod migrations;
pub mod musicbrainz;
pub mod network;
pub mod playback;
pub mod queue;
pub mod retry;
mod serde_helpers;
// The signal-extraction machinery (OCR, disc-ID compute) is desktop-only and
// gated submodule-by-submodule inside `signals`. The module itself stays
// available on every target for the pure `LookupFailure` type, which the shared
// metadata-search path (`import::search`) maps provider errors into.
pub mod signals;
pub mod storage;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_logs;
pub mod text_encoding;
pub mod ui;
pub mod util;

pub type CloudKitOpsRef = std::sync::Arc<dyn coven::CloudKitOps>;
