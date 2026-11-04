use super::*;
use tempfile::TempDir;

/// Sessions over a fresh library whose keyring is in memory, asking
/// providers that reach nothing.
pub(crate) async fn test_sessions() -> (CatalogSessions, Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let database = Database::new_test(
        crate::TEST_SCHEMA,
        temp_dir.path().join("test.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let sessions = CatalogSessions::new(
        Providers::offline(),
        database.clone(),
        Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    (sessions, database, temp_dir)
}

async fn stored_validation(database: &Database) -> Option<DiscogsValidation> {
    bae_preferences::preferences(database).await.unwrap().discogs
}

async fn asks_discogs(sessions: &CatalogSessions) -> bool {
    sessions.session().await.unwrap().asked().contains(&LookupCatalog::Discogs)
}

/// Record what Discogs said of the stored key, as a check of it does.
async fn record_validation(database: &Database, validation: DiscogsValidation) {
    bae_preferences::edit_preferences(database, move |prefs| {
        prefs.discogs = Some(validation);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn discogs_is_not_asked_with_a_rejected_key() {
    let (sessions, database, _temp_dir) = test_sessions().await;
    sessions
        .store_discogs_key("f7228aaf-52b3-40ea-8526-a7e8aa0bf5da", DiscogsValidation::Valid)
        .await
        .unwrap();

    assert!(asks_discogs(&sessions).await, "a Valid key is served");

    record_validation(&database, DiscogsValidation::Unvalidated).await;
    assert!(asks_discogs(&sessions).await, "an Unvalidated key is served optimistically");

    record_validation(&database, DiscogsValidation::Rejected).await;
    assert!(!asks_discogs(&sessions).await, "a Rejected key is withheld");
}

/// A catalog the person switched off is not one a session asks, whatever key
/// the library holds; switching it back on asks it again.
#[tokio::test]
async fn a_switched_off_catalog_is_not_asked() {
    let (sessions, database, _temp_dir) = test_sessions().await;
    sessions.store_discogs_key("the-key", DiscogsValidation::Valid).await.unwrap();
    let switch = |catalog, enabled| {
        bae_preferences::edit_preferences(&database, move |prefs| {
            prefs.identification.catalogs.set(catalog, enabled);
            Ok(())
        })
    };

    switch(LookupCatalog::Discogs, false).await.unwrap();
    assert_eq!(sessions.session().await.unwrap().asked(), [LookupCatalog::MusicBrainz]);

    switch(LookupCatalog::Discogs, true).await.unwrap();
    switch(LookupCatalog::MusicBrainz, false).await.unwrap();
    assert_eq!(sessions.session().await.unwrap().asked(), [LookupCatalog::Discogs]);
}

/// A key present in the keyring but absent from the preferences (the residue a
/// torn write or external keyring tampering can leave) is not served: a usable
/// key requires both stores to agree it exists.
#[tokio::test]
async fn discogs_is_not_asked_with_a_key_the_preferences_do_not_claim() {
    let (sessions, database, _temp_dir) = test_sessions().await;

    database.set_host_secret(DISCOGS_API_KEY, "orphan-key").unwrap();

    assert_eq!(stored_validation(&database).await, None);
    assert!(!asks_discogs(&sessions).await, "a keyring key with no preference is not served");
}

/// Storing and clearing the key move both durable stores together.
#[tokio::test]
async fn storing_and_clearing_the_key_move_both_stores() {
    let (sessions, database, _temp_dir) = test_sessions().await;

    sessions.store_discogs_key("the-key", DiscogsValidation::Valid).await.unwrap();
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));
    assert_eq!(database.host_secret(DISCOGS_API_KEY).unwrap().as_deref(), Some("the-key"),);
    assert!(asks_discogs(&sessions).await);

    sessions.clear_discogs_key().await.unwrap();
    assert_eq!(stored_validation(&database).await, None);
    assert_eq!(database.host_secret(DISCOGS_API_KEY).unwrap(), None);
    assert!(!asks_discogs(&sessions).await);
}

#[tokio::test]
async fn discogs_validation_signals_confirm_and_reject() {
    let (_sessions, database, _temp_dir) = test_sessions().await;
    // A success confirms a stored Unvalidated key.
    record_validation(&database, DiscogsValidation::Unvalidated).await;
    record_discogs_validation_signal(&database, DiscogsKeySignal::Accepted).await.unwrap();
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));

    // A 401 rejects, from any prior state.
    record_discogs_validation_signal(&database, DiscogsKeySignal::Rejected).await.unwrap();
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Rejected));

    // A success does NOT flip an already-Rejected key back to Valid.
    record_discogs_validation_signal(&database, DiscogsKeySignal::Accepted).await.unwrap();
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Rejected));

    // A success while already Valid is a no-op (only Unvalidated -> Valid).
    record_validation(&database, DiscogsValidation::Valid).await;
    record_discogs_validation_signal(&database, DiscogsKeySignal::Accepted).await.unwrap();
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));
}

/// Sessions over a fresh library asking a Discogs that `host` stands in for,
/// reporting bugs to `bugs`.
async fn sessions_over(
    host: &bae_util::test_host::FakeHost,
    bugs: Arc<bae_diagnostics::RecordingBugReporter>,
) -> (CatalogSessions, Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let database = Database::new_test(
        crate::TEST_SCHEMA,
        temp_dir.path().join("test.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let http = bae_util::http::Http::for_test().serve("api.discogs.com", host.base_url());
    let providers = Providers::for_test(
        http.clone(),
        Arc::new(bae_catalogs::musicbrainz::MusicBrainz::for_test(http.clone())),
        Arc::new(bae_catalogs::discogs::client::Discogs::new(http.clone())),
        Arc::new(bae_catalogs::wikidata::Wikidata::for_test(http)),
    );
    let sessions = CatalogSessions::new(
        providers,
        database.clone(),
        Diagnostics::noop().reporting_bugs_to(bugs),
        tokio::runtime::Handle::current(),
    );
    (sessions, database, temp_dir)
}

const REJECTED: &str = r#"{"message": "Invalid consumer token."}"#;

/// A check of the stored key that Discogs answers after the key was replaced
/// is no answer about the new key: the new key keeps what its own store
/// recorded.
#[tokio::test(flavor = "multi_thread")]
async fn a_revalidation_answered_after_the_key_changed_leaves_the_new_key_alone() {
    let host = bae_util::test_host::FakeHost::start().await;
    let (sessions, database, _temp_dir) = sessions_over(&host, Arc::default()).await;
    sessions.store_discogs_key("old-key", DiscogsValidation::Unvalidated).await.unwrap();
    host.route("/database/search", 401, REJECTED);
    host.hold("/database/search");

    let revalidating = {
        let sessions = sessions.clone();
        tokio::spawn(async move { sessions.revalidate_discogs_key().await })
    };
    host.wait_for_request("/database/search", 1).await;
    sessions.store_discogs_key("new-key", DiscogsValidation::Valid).await.unwrap();
    host.release();

    let check = revalidating.await.unwrap().unwrap();
    assert!(matches!(check, Some(DiscogsCheck::Rejected)), "{check:?}");
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));
}

/// What a call made with a replaced key proves is recorded nowhere: a 401 a
/// client of the old key hears after the key changed leaves the new key as
/// its store recorded it.
#[tokio::test(flavor = "multi_thread")]
async fn a_signal_about_a_replaced_key_leaves_the_new_key_alone() {
    let host = bae_util::test_host::FakeHost::start().await;
    let (sessions, database, _temp_dir) = sessions_over(&host, Arc::default()).await;
    sessions.store_discogs_key("old-key", DiscogsValidation::Valid).await.unwrap();
    let old = sessions.session().await.unwrap();
    sessions.store_discogs_key("new-key", DiscogsValidation::Valid).await.unwrap();
    host.route("/masters/", 401, REJECTED);

    old.master_covers("909", CallPriority::Interactive).await.unwrap_err();
    sessions.signals_recorded().await;

    assert_eq!(host.count_containing("/masters/"), 1, "the old client asked");
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));
}

/// A call's signal is recorded with no caller waiting on it, so a write of
/// it that fails is reported as a bug.
#[tokio::test(flavor = "multi_thread")]
async fn a_signal_that_cannot_be_recorded_is_reported() {
    let host = bae_util::test_host::FakeHost::start().await;
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let (sessions, _database, temp_dir) = sessions_over(&host, bugs.clone()).await;
    sessions.store_discogs_key("the-key", DiscogsValidation::Valid).await.unwrap();
    let session = sessions.session().await.unwrap();
    host.route("/masters/", 401, REJECTED);
    crate::candidate_runtime::tests::add_to_library_schema(
        temp_dir.path(),
        "preferences",
        "CREATE TRIGGER fail_preferences_write BEFORE UPDATE ON preferences \
         BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;"
            .to_string(),
    )
    .await;

    session.master_covers("909", CallPriority::Interactive).await.unwrap_err();
    sessions.signals_recorded().await;

    let reports = bugs.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].0, "record a Discogs validation signal");
}
