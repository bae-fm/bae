use super::*;
use tempfile::TempDir;

/// Sessions over a fresh library whose keyring is in memory, asking
/// providers that reach nothing.
async fn test_sessions() -> (CatalogSessions, Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let database = Database::new_test(
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
    crate::preferences::preferences(database).await.unwrap().discogs
}

async fn asks_discogs(sessions: &CatalogSessions) -> bool {
    sessions.session().await.unwrap().asks_discogs()
}

/// Record what Discogs said of the stored key, as a check of it does.
async fn record_validation(database: &Database, validation: DiscogsValidation) {
    crate::preferences::edit_preferences(database, move |prefs| {
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

/// A key present in the keyring but absent from the preferences (the residue a
/// torn write or external keyring tampering can leave) is not served: a usable
/// key requires both stores to agree it exists.
#[tokio::test]
async fn discogs_is_not_asked_with_a_key_the_preferences_do_not_claim() {
    let (sessions, database, _temp_dir) = test_sessions().await;

    database.set_host_secret(crate::keys::DISCOGS_API_KEY, "orphan-key").unwrap();

    assert_eq!(stored_validation(&database).await, None);
    assert!(!asks_discogs(&sessions).await, "a keyring key with no preference is not served");
}

/// Storing and clearing the key move both durable stores together.
#[tokio::test]
async fn storing_and_clearing_the_key_move_both_stores() {
    let (sessions, database, _temp_dir) = test_sessions().await;

    sessions.store_discogs_key("the-key", DiscogsValidation::Valid).await.unwrap();
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));
    assert_eq!(
        database.host_secret(crate::keys::DISCOGS_API_KEY).unwrap().as_deref(),
        Some("the-key"),
    );
    assert!(asks_discogs(&sessions).await);

    sessions.clear_discogs_key().await.unwrap();
    assert_eq!(stored_validation(&database).await, None);
    assert_eq!(database.host_secret(crate::keys::DISCOGS_API_KEY).unwrap(), None);
    assert!(!asks_discogs(&sessions).await);
}

#[tokio::test]
async fn discogs_validation_signals_confirm_and_reject() {
    let (_sessions, database, _temp_dir) = test_sessions().await;
    // A success confirms a stored Unvalidated key.
    record_validation(&database, DiscogsValidation::Unvalidated).await;
    record_discogs_validation_signal(&database, DiscogsKeySignal::Accepted).await;
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));

    // A 401 rejects, from any prior state.
    record_discogs_validation_signal(&database, DiscogsKeySignal::Rejected).await;
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Rejected));

    // A success does NOT flip an already-Rejected key back to Valid.
    record_discogs_validation_signal(&database, DiscogsKeySignal::Accepted).await;
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Rejected));

    // A success while already Valid is a no-op (only Unvalidated -> Valid).
    record_validation(&database, DiscogsValidation::Valid).await;
    record_discogs_validation_signal(&database, DiscogsKeySignal::Accepted).await;
    assert_eq!(stored_validation(&database).await, Some(DiscogsValidation::Valid));
}
