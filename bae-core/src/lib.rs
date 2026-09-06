#![deny(unreachable_pub, dead_code)]

pub mod airplay;
pub mod album_detail;
pub mod app;
pub mod audio_codec;
pub mod cast;
#[doc(hidden)]
pub mod config;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod cue_flac;
pub mod db;
pub mod diagnostics;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod discogs;
pub mod dlna;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod identify;
pub mod import;
pub mod keys;
pub mod library;
pub mod library_name;
pub(crate) mod live_query;
// Only import measures loudness, and the import pipeline is desktop-only —
// the same predicate that gates `import`'s pipeline modules and `ebur128`
// itself. Playback derives its gain from the stored measurements with plain
// arithmetic, so mobile needs neither.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod loudness;
pub mod migrations;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod musicbrainz;
pub mod network;
pub mod oauth;
pub mod playback;
pub mod queue;
pub mod renderer;
pub mod retry;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod serde_helpers;
// The extraction machinery (OCR, disc-ID compute) is desktop-only and gated
// submodule-by-submodule inside `signals`. The module itself stays on every
// target for the pure `LookupFailure` type, which the shared metadata-search path
// (`import::search`) maps provider errors into.
pub mod signals;
pub mod storage;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_logs;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod text_encoding;
pub mod ui;
pub mod util;

pub type CloudKitOpsRef = std::sync::Arc<dyn coven::CloudKitOps>;
