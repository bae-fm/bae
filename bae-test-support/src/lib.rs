//! Helpers shared by bae-core's integration tests.
//!
//! bae-core's two integration test binaries (`tests/integration/` and
//! `tests/test_playback_cpu.rs`) reach these through
//! `use bae_test_support as support;`, and the unit tests of the crates above
//! bae-core use them too, so they are a library of their own.
//!
//! The helpers are grouped by what they set up — a Discogs or MusicBrainz
//! fixture, files on disk, a library, an import, playback — and re-exported
//! here, so every caller keeps reaching them as `support::<name>`.

/// The calling crate's `tests/fixtures` directory, with any path segments
/// given joined onto it: `fixture_dir!("cue_flac")` is
/// `<manifest>/tests/fixtures/cue_flac`.
///
/// A macro rather than a function because `CARGO_MANIFEST_DIR` has to expand at
/// the call site: a function here would resolve the variable to
/// bae-test-support's manifest.
#[macro_export]
macro_rules! fixture_dir {
    ($($segment:expr),* $(,)?) => {
        ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            $(.join($segment))*
    };
}

/// How long a test waits on an event before failing instead of hanging.
/// Every wait it bounds ends on the code's own event, so only code that never
/// sends it reaches this. It is not a measure of speed, so it sits far above
/// what any step takes on a loaded machine.
pub const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(300);

mod audio;
mod discogs;
mod files;
mod import;
mod library;
mod musicbrainz;
mod playback;
mod storage;
pub use storage::{connect_storage, release_storage, remove_uploaded_file, storage_library};

pub use audio::{assert_captured_matches_reference, filled_sparse_buffer, samples_as_f32};
pub use discogs::{
    discogs_artist, discogs_fixture_id, discogs_test_release, discogs_track,
    seed_discogs_test_release,
};
pub use files::{
    copy_and_tag, cover_png, read_cover_image_bytes, read_fixture, write_cover_png,
    write_tagged_flac, ImageHold, RemoteImageHost, TEST_IMAGE_HOST,
};
pub use import::{
    configure_test_discogs, discogs_release, folder_import, import_folder_and_wait,
    imported_release_setup, start_test_import, try_wait_for_import_complete,
    wait_for_import_complete, wait_for_import_end, DraftSource, ImportedRelease,
};
pub use library::{
    multi_thread_runtime, open_test_library, open_test_library_with, runtime_with_services,
    setup_fresh_library, setup_test_library, setup_test_library_with_album_dir, temp_test_db,
    test_config, test_library_from_handle, test_uuid, tracing_init,
};
pub use musicbrainz::{mb_medium, mb_release, mb_track, seed_mb_release};
pub use playback::{
    next_capture_stream, next_matching, start_capture_playback, start_capture_playback_with_clock,
    wait_for_seek, wait_until_playing, CaptureStreamRx, TestAudioDevice,
};
