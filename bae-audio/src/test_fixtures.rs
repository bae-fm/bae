//! Audio files the tests of this crate and the crates above it decode, probe
//! and import: a single-file CUE/FLAC album (tests/fixtures/cue_flac) and one
//! short placeholder file per format bae reads (test-fixtures/audio-format).

use std::path::PathBuf;

/// The CUE/FLAC album's folder: `Test Album.flac` and `Test Album.cue`, and
/// two of its tracks as files of their own.
pub fn cue_flac_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_flac"))
}

/// The folder of placeholder files, one per format.
pub fn audio_format_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test-fixtures/audio-format"))
}

/// The placeholder MP3's bytes.
pub static PLACEHOLDER_MP3: &[u8] =
    include_bytes!("../test-fixtures/audio-format/placeholder-mp3.mp3");
/// The placeholder AAC (MP4) file's bytes.
pub static PLACEHOLDER_AAC: &[u8] =
    include_bytes!("../test-fixtures/audio-format/placeholder-aac.m4a");
