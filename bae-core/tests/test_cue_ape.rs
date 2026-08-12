#![cfg(feature = "test-utils")]
//! Tests for CUE/APE format handling.
//!
//! CUE/APE albums have multiple tracks in a single APE file. The import must:
//! - Parse the CUE sheet to find track boundaries
//! - Record per-track timing (track_start_ms / track_end_ms)
//! - Store correct per-track durations (NOT the full file duration)
//! - Enable playback of individual tracks via full-file decode with seek/stop
include!("test_cue_ape/single_disc.rs");
include!("test_cue_ape/multi_disc.rs");
