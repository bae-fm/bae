pub mod album_detail;
pub mod app;
#[doc(hidden)]
#[cfg(desktop)]
pub mod cue_flac;
#[cfg(all(feature = "dev-reset", unix))]
pub mod dev_reset;
#[cfg(desktop)]
pub mod identify;
pub mod import;
pub mod keys;
pub mod library;
pub mod library_name;
pub mod live_query;
// Only import measures loudness, and the import pipeline is desktop-only —
// the same predicate that gates `import`'s pipeline modules and `ebur128`
// itself. Playback derives its gain from the stored measurements with plain
// arithmetic, so mobile needs neither.
#[cfg(any(test, feature = "test-utils"))]
pub mod followed;
#[cfg(desktop)]
pub mod loudness;
#[cfg(any(test, feature = "test-utils"))]
pub mod open_files_peak;
pub mod people;
pub mod playback;
pub mod renderer;
pub mod storage;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_files;
#[cfg(test)]
pub(crate) mod test_gate;
#[cfg(all(test, desktop))]
pub(crate) mod test_images;
#[cfg(test)]
pub(crate) mod test_reads;
pub mod track_audio;
