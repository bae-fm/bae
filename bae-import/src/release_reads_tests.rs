use super::*;
use bae_catalogs::catalog::LookupCatalog;

/// A MusicBrainz release is read with the library's catalog session, so a
/// read whose preferences cannot be read fails with that error, rather than
/// reading with MusicBrainz alone as though nothing were switched off.
#[tokio::test]
async fn a_release_read_fails_when_the_preferences_cannot_be_read() {
    let (sessions, database, temp_dir) = crate::catalog_sessions::tests::test_sessions().await;
    let directory = temp_dir.path().to_path_buf();
    tokio::task::spawn_blocking(move || {
        bae_preferences::plant_unknown_replay_gain_mode_for_test(&directory)
    })
    .await
    .unwrap();
    let reads = ReleaseReads::new(database, sessions);

    let error = reads
        .release(
            &LookupRef::new(LookupCatalog::MusicBrainz, "mb-release-1"),
            ReadDepth::Complete,
            CallClass::REQUESTED,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ImportError::CatalogSession(crate::CatalogSessionError::Preferences(_))),
        "{error:?}"
    );
}
