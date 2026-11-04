use super::*;
use bae_catalogs::catalog::LookupCatalog;

/// A MusicBrainz release is read with the library's catalog session, so a
/// read whose preferences cannot be read fails with that error, rather than
/// reading with MusicBrainz alone as though nothing were switched off.
#[tokio::test]
async fn a_release_read_fails_when_the_preferences_cannot_be_read() {
    let (sessions, database, temp_dir) = crate::catalog_sessions::tests::test_sessions().await;
    crate::candidate_runtime::tests::add_to_library_schema(
        temp_dir.path(),
        "preferences",
        "PRAGMA ignore_check_constraints = 1; \
         UPDATE preferences SET replay_gain_mode = 'unknown';"
            .to_string(),
    )
    .await;
    let reads = ReleaseReads::new(database, sessions);

    let error = reads
        .release(
            &LookupRef::new(LookupCatalog::MusicBrainz, "mb-release-1"),
            ReadDepth::Complete,
            CallPriority::Interactive,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, ImportError::CatalogSession(crate::CatalogSessionError::Preferences(_))),
        "{error:?}"
    );
}
