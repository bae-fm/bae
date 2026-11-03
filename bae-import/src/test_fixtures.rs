//! Audio the tests of this crate and the crates above it import: short FLAC
//! tracks and a 96 kHz one (tests/fixtures/flac), a CUE/APE album beside two
//! FLAC tracks of its own (tests/fixtures/cue_ape), and ALAC and AAC files
//! with a sheet for one of them (test-fixtures/alac).

use std::path::PathBuf;

/// The FLAC tracks' folder: `01 Test Track 1.flac` to `03 Test Track 3.flac`
/// and `96khz_test.flac`.
pub fn flac_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/flac"))
}

/// The ALAC and AAC files' folder: `silence-alac.m4a`, `silence-aac.m4a`,
/// and `cue-alac.m4a` with the `cue-alac.cue` that splits it.
pub fn alac_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test-fixtures/alac"))
}

/// The CUE/APE album's folder: `Test Album.ape`, `Test Album.cue`, and two
/// tracks as FLAC files of their own.
pub fn cue_ape_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape"))
}
