use bae_catalogs::catalog::LookupCatalog;
use bae_preferences::{DiscogsValidation, SourceAvailability};

/// A library at `dir` holding a Discogs key Discogs accepted.
async fn library_with_a_key(
    dir: &std::path::Path,
) -> (bae_library::test_libraries::TestLibrary, bae_app::AppServices) {
    let library = bae_library::test_libraries::open_test_library(dir).await;
    library.catalogs().store_discogs_key("the-key", DiscogsValidation::Valid).await.unwrap();
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    (library, services)
}

/// Switching Search Discogs off takes its key with it: the switch is how a
/// key goes.
#[tokio::test]
async fn switching_discogs_off_clears_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let (library, services) = library_with_a_key(dir.path()).await;

    services.set_metadata_source_enabled(LookupCatalog::Discogs, false).await.unwrap();

    let preferences = library.manager().preferences().await.unwrap();
    assert_eq!(preferences.discogs, None, "the key's state is gone");
    assert_eq!(library.catalogs().stored_discogs_key().await.unwrap(), None, "so are its bytes");
    assert_eq!(preferences.metadata_sources()[1].state, SourceAvailability::Off);
}

/// A switch the library refuses to move leaves the key where it is.
#[tokio::test]
async fn a_refused_switch_keeps_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let (library, services) = library_with_a_key(dir.path()).await;
    services.set_metadata_source_enabled(LookupCatalog::MusicBrainz, false).await.unwrap();

    services
        .set_metadata_source_enabled(LookupCatalog::Discogs, false)
        .await
        .expect_err("Discogs is the only source left to search");

    assert_eq!(library.catalogs().stored_discogs_key().await.unwrap().as_deref(), Some("the-key"));
}

/// Switching Search Discogs on needs no key: it waits for one.
#[tokio::test]
async fn switching_discogs_on_without_a_key_waits_for_one() {
    let dir = tempfile::tempdir().unwrap();
    let library = bae_library::test_libraries::open_test_library(dir.path()).await;
    let services = bae_app::test_services::start_services(&library).await.unwrap();
    services.set_metadata_source_enabled(LookupCatalog::Discogs, false).await.unwrap();

    services.set_metadata_source_enabled(LookupCatalog::Discogs, true).await.unwrap();

    let sources = library.manager().metadata_sources().await.unwrap();
    assert_eq!(sources[1].state, SourceAvailability::NotConfigured);
}
