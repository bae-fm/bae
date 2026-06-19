pub mod album_detail;
pub mod ape;
pub mod app;
pub mod audio_codec;
pub mod clock;
#[doc(hidden)]
pub mod config;
pub mod cue_flac;
pub mod db;
pub mod discogs;
pub mod encryption;
pub mod id_provider;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod identify;
pub mod import;
pub mod keys;
pub mod library;
pub mod library_dir;
pub mod library_name;
pub mod musicbrainz;
pub mod network;
pub mod oauth;
pub mod playback;
pub mod queue;
pub mod retry;
// The signal-extraction machinery (OCR, disc-ID compute) is desktop-only and
// gated submodule-by-submodule inside `signals`. The module itself stays
// available on every target for the pure `LookupFailure` type, which the shared
// metadata-search path (`import::search`) maps provider errors into.
pub mod signals;
pub mod storage;
pub mod sync;
pub mod text_encoding;
pub mod ui;
pub mod util;
