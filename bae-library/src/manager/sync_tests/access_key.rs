//! Using a new S3 key: what each way it ends comes to, and the recorded key
//! the config query shows.

use super::*;
use bae_diagnostics::UiErrorCategory;

/// The S3 key the config query says the library records for this person.
fn recorded_key(config: &bae_preferences::Config) -> Option<&str> {
    match &config.storage_access {
        Some(coven::MemberAccess::S3AccessKey { access_key_id }) => Some(access_key_id),
        _ => None,
    }
}

/// Wait for the config query to show `key` as the recorded key.
async fn config_showing(
    config: &bae_store::live_query::LiveValues<bae_preferences::Config>,
    key: &str,
) {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while recorded_key(&config.next().await.unwrap()) != Some(key) {}
    })
    .await
    .unwrap_or_else(|_| panic!("the config never showed the key {key}"));
}

/// A cancel nobody sends.
fn uncancelled() -> (tokio::sync::watch::Sender<bool>, tokio::sync::watch::Receiver<bool>) {
    tokio::sync::watch::channel(false)
}

/// A new key storage takes is saved and names the key the library recorded
/// before, and the config query the Sync page reads shows the new key.
#[tokio::test]
async fn a_new_key_is_saved_and_the_config_shows_it() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;
    let config = manager.subscribe_config();
    config_showing(&config, "test-key").await;

    let (_cancel, cancelled) = uncancelled();
    let outcome = manager
        .use_access_key("new-key".into(), coven::SecretText::new("new-secret".into()), &cancelled)
        .await;

    assert_eq!(outcome, crate::AccessKeyOutcome::Saved { replaced: Some("test-key".into()) });
    config_showing(&config, "new-key").await;
}

/// The key the library already records is saved again with no old key to
/// delete, as on a device after another of the person's devices replaced it.
#[tokio::test]
async fn the_recorded_key_is_saved_with_no_old_key() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;

    let (_cancel, cancelled) = uncancelled();
    let outcome = manager
        .use_access_key("test-key".into(), coven::SecretText::new("test-secret".into()), &cancelled)
        .await;

    assert_eq!(outcome, crate::AccessKeyOutcome::Saved { replaced: None });
}

/// A cancel before the check ends it as cancelled, and the library keeps
/// its key.
#[tokio::test]
async fn a_cancelled_check_changes_nothing() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;
    let (cancel, cancelled) = uncancelled();
    cancel.send_replace(true);

    let outcome = manager
        .use_access_key("new-key".into(), coven::SecretText::new("new-secret".into()), &cancelled)
        .await;

    assert_eq!(outcome, crate::AccessKeyOutcome::Cancelled);
    let config = manager.config().await.unwrap();
    assert_eq!(recorded_key(&config), Some("test-key"));
}

/// A check that can't reach storage fails in the network's line, and the
/// library keeps its key.
#[tokio::test]
async fn an_unreachable_storage_fails_the_key_in_its_line() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;
    test.storage.set_online(false);

    let (_cancel, cancelled) = uncancelled();
    let outcome = manager
        .use_access_key("new-key".into(), coven::SecretText::new("new-secret".into()), &cancelled)
        .await;

    let crate::AccessKeyOutcome::Failed(bae_diagnostics::UiError::Diagnostic { category, .. }) =
        outcome
    else {
        panic!("an unreachable storage fails the key: {outcome:?}");
    };
    assert_eq!(category, UiErrorCategory::Storage(coven::StorageFailure::Network));
    test.storage.set_online(true);
    let config = manager.config().await.unwrap();
    assert_eq!(recorded_key(&config), Some("test-key"));
}

/// Storage refusing the key, as a key it doesn't know or one that can't
/// write, is a refused key wherever in the check storage refused it; any
/// other failure of the check is a failure in its own line.
#[test]
fn storage_refusing_the_key_is_a_refused_key() {
    use coven::{StorageCheckError as Check, StorageFailure as F, UseAccessKeyError as E};
    let storage = |failure| E::Check(Check::Storage(coven::StorageError::from(failure)));
    let check = |failure| {
        E::Check(Check::ProviderCheck {
            check: coven::StorageCheck::Create,
            source: coven::StorageError::from(failure),
        })
    };
    for refused in [
        storage(F::Authentication),
        storage(F::PermissionDenied),
        check(F::Authentication),
        check(F::PermissionDenied),
    ] {
        assert_eq!(crate::AccessKeyOutcome::of(Err(refused)), crate::AccessKeyOutcome::Refused);
    }
    for failed in [storage(F::Network), check(F::ClockSkew)] {
        assert!(matches!(
            crate::AccessKeyOutcome::of(Err(failed)),
            crate::AccessKeyOutcome::Failed(_)
        ));
    }
    assert_eq!(crate::AccessKeyOutcome::of(Err(E::Cancelled)), crate::AccessKeyOutcome::Cancelled);
}
