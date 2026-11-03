//! Helpers shared by bae-core's integration tests.
//!
//! bae-core's two integration test binaries (`tests/integration/` and
//! `tests/test_playback_cpu.rs`) reach these through
//! `use bae_test_support as support;`, and the unit tests of the crates above
//! bae-core use them too, so they are a library of their own.
//!
//! The helpers are grouped by what they set up — files on disk, a library, an import, playback — and re-exported
//! here, so every caller keeps reaching them as `support::<name>`.

/// The calling crate's `tests/fixtures` directory, with any path segments
/// given joined onto it: `fixture_dir!("flac")` is
/// `<manifest>/tests/fixtures/flac`.
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

mod files;
mod import;
mod library;
mod playback;
mod profiles;
mod storage;
pub use profiles::{made_profile, made_profile_on, requested_profile, requested_profile_on};
pub use storage::{connect_storage, release_storage, remove_uploaded_file, storage_library};

pub use files::{
    copy_and_tag, read_cover_image_bytes, read_fixture, write_cover_png, write_tagged_flac,
};
pub use import::{
    configure_test_discogs, discogs_release, folder_import, import_folder_and_wait,
    imported_release_setup, imported_release_setup_with, start_import_service, start_test_import,
    try_wait_for_import_complete, wait_for_import_complete, wait_for_import_end, DraftSource,
    ImportedRelease,
};
pub use library::{
    multi_thread_runtime, open_test_library, open_test_library_with, runtime_with_services,
    setup_fresh_library, setup_test_library, setup_test_library_with,
    setup_test_library_with_album_dir, start_test_services, test_library_from_handle, tracing_init,
};
pub use playback::{start_capture_playback, start_capture_playback_with_clock, start_playback_on};
