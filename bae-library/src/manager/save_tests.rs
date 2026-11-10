//! Saving: what a save of a release the library no longer holds reports.

use crate::manager::tests::*;

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
