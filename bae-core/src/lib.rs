#![deny(unreachable_pub, dead_code)]

pub mod album_detail;
pub mod app;
pub mod audio_codec;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod barcode;
pub mod collections;
#[doc(hidden)]
pub mod config;
pub mod credits;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod cue_flac;
pub mod db;
#[cfg(all(feature = "dev-reset", unix))]
pub mod dev_reset;
pub mod diagnostics;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod discogs;
pub mod history;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod identify;
pub mod import;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod isrc;
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
pub mod mediums;
pub mod migrations;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod musicbrainz;
#[cfg(any(test, feature = "test-utils"))]
pub mod open_files_peak;
pub mod people;
pub mod playback;
pub mod pressing;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod providers;
pub mod queue;
pub mod renderer;
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
pub(crate) mod test_files;
#[cfg(test)]
pub(crate) mod test_gate;
#[cfg(test)]
pub(crate) mod test_logs;
#[cfg(test)]
pub(crate) mod test_reads;
pub(crate) mod text_match;
pub mod ui;
pub mod util;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod wikidata;
