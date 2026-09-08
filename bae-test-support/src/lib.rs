//! Helpers shared by bae-core's integration tests.
//!
//! Every `bae-core/tests/*.rs` binary is its own crate and reaches these through
//! `use bae_test_support as support;`. Keeping them in a library — rather than a
//! `mod support;` file textually recompiled into each binary — is what lets
//! `dead_code` and `unreachable_pub` stay armed here: the helpers are this
//! crate's public API, so neither lint has to be silenced for the binaries that
//! happen not to call a given one.
//!
//! The helpers are grouped by what they set up — a Discogs or MusicBrainz
//! fixture, files on disk, a library, an import, playback — and re-exported
//! here, so every caller keeps reaching them as `support::<name>`.

/// The calling crate's `tests/fixtures` directory, with any path segments
/// given joined onto it: `fixture_dir!("cue_flac")` is
/// `<manifest>/tests/fixtures/cue_flac`.
///
/// A macro rather than a function because `CARGO_MANIFEST_DIR` has to expand at
/// the call site — every `bae-core/tests/*.rs` binary is its own crate, and a
/// function here would resolve the variable to bae-test-support's manifest.
#[macro_export]
macro_rules! fixture_dir {
    ($($segment:expr),* $(,)?) => {
        ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            $(.join($segment))*
    };
}

mod audio;
mod discogs;
mod files;
mod import;
mod library;
mod musicbrainz;
mod playback;

pub use audio::{assert_captured_matches_reference, samples_as_f32};
pub use discogs::{
    discogs_artist, discogs_fixture_id, discogs_test_release, discogs_track,
    point_discogs_at_dead_port, seed_discogs_test_release,
};
pub use files::{
    copy_and_tag, cover_art_archive, cover_png, read_cover_image_blob, write_cover_png,
    write_tagged_flac, RemoteImageHost,
};
pub use import::{
    configure_test_discogs, discogs_release, folder_import, import_folder_and_wait,
    imported_release_setup, start_test_import, try_wait_for_import_complete,
    wait_for_import_complete, ImportedRelease,
};
pub use library::{
    multi_thread_runtime, open_test_library, runtime_with_services, setup_fresh_library,
    setup_test_library, setup_test_library_with_album_dir, temp_test_db, test_config, test_uuid,
    tracing_init,
};
pub use musicbrainz::{mb_medium, mb_release, mb_track, seed_mb_release};
pub use playback::{
    next_capture_stream, next_matching, start_capture_playback, wait_for_seek, wait_until_playing,
    CaptureStreamRx, TestAudioDevice,
};
