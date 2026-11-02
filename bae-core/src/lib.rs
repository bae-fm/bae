pub mod album_detail;
pub mod app;
pub mod catalog_model;
pub mod collections;
#[doc(hidden)]
pub mod config;
pub mod credits;
#[cfg(desktop)]
pub mod cue_flac;
pub mod db;
#[cfg(all(feature = "dev-reset", unix))]
pub mod dev_reset;
pub mod diagnostics;
pub mod history;
#[cfg(desktop)]
pub mod identify;
pub mod import;
pub mod keys;
pub mod library;
pub mod library_name;
pub(crate) mod live_query;
pub mod logging;
// Only import measures loudness, and the import pipeline is desktop-only —
// the same predicate that gates `import`'s pipeline modules and `ebur128`
// itself. Playback derives its gain from the stored measurements with plain
// arithmetic, so mobile needs neither.
#[cfg(any(test, feature = "test-utils"))]
pub mod followed;
#[cfg(desktop)]
pub mod loudness;
pub mod migrations;
#[cfg(any(test, feature = "test-utils"))]
pub mod open_files_peak;
pub mod people;
pub mod playback;
pub mod queue;
pub mod renderer;
// The extraction machinery (OCR, disc-ID compute) is desktop-only and gated
// submodule-by-submodule inside `signals`. The module itself stays on every
// target for the pure origin values (`origin`).
pub mod signals;
pub mod storage;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_files;
#[cfg(test)]
pub(crate) mod test_gate;
#[cfg(all(test, desktop))]
pub(crate) mod test_images;
#[cfg(test)]
pub(crate) mod test_logs;
#[cfg(test)]
pub(crate) mod test_reads;
