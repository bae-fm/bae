//! Saving a track: a preset for releases only is refused before any track work.

use crate::manager::tests::*;

/// `save_track` resolves the preset first, so a release-only preset is refused
/// before any track work — the id need not even exist as a track.
#[tokio::test]
async fn save_track_rejects_release_only_preset() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut presets = manager.save_presets().await.unwrap();
    presets.push(release_only_image_preset());
    manager.set_save_presets(presets).await.unwrap();

    let err = manager
        .save_track("any-track", std::path::Path::new("/tmp/out.flac"), "flac-image")
        .await
        .expect_err("a release-only preset can't back a track save");
    assert!(err.to_string().contains("not available for track save"), "unexpected error: {err}");
}
