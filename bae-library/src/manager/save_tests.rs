//! Saving a track: a preset that writes a release as one file is refused before
//! any track work.

use crate::manager::tests::*;

/// `save_track` resolves the preset first, so a preset that writes a release as
/// one image file is refused before any track work — the id need not even exist
/// as a track.
#[tokio::test]
async fn save_track_rejects_single_file_image_preset() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let mut presets = manager.save_presets().await.unwrap();
    presets.push(single_file_image_preset());
    manager.set_save_presets(presets).await.unwrap();

    let err = manager
        .save_track("any-track", std::path::Path::new("/tmp/out.flac"), "flac-image")
        .await
        .expect_err("a single-file image preset can't back a track save");
    assert!(err.to_string().contains("writes a release as one file"), "unexpected error: {err}");
}

/// A save of a release the library no longer holds fails as a save, and an
/// export as an export: the refusal is the operation's.
#[tokio::test]
async fn a_missing_release_fails_as_the_output_asked_for() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let preset = manager.preferences().await.unwrap().default_release_save_preset;

    let saved = manager
        .enqueue_release_save("missing-release", temp_dir.path().to_path_buf(), &preset)
        .await
        .expect_err("no release to save");
    assert_eq!(saved.category(), bae_diagnostics::UiErrorCategory::Save, "{saved}");
    let exported = manager
        .enqueue_export("missing-release", temp_dir.path().to_path_buf())
        .await
        .expect_err("no release to export");
    assert_eq!(exported.category(), bae_diagnostics::UiErrorCategory::Export, "{exported}");
}
