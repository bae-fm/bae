//! `release_storage_action`: a release is in one storage transition at a
//! time, and its reads show it.

use super::*;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::*;

/// One album holding `RELEASE_A` and `RELEASE_B`.
async fn library() -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    album_with_releases(&test_records(&db), ALBUM_A, "Album Title A", &[RELEASE_A, RELEASE_B])
        .await;
    (db, tmp)
}

/// A release already in a transition refuses a second; ending it lets one
/// begin, and each release's read shows its own.
#[tokio::test]
async fn a_release_is_in_one_transition_at_a_time() {
    let (db, _tmp) = library().await;
    assert!(begin_release_storage_action(&db, RELEASE_A, StoredTransition::DetachAudio)
        .await
        .unwrap());
    assert!(
        !begin_release_storage_action(&db, RELEASE_A, StoredTransition::Unpin).await.unwrap(),
        "a release in a transition refuses another"
    );
    assert_eq!(
        release_transfer_action(&db, RELEASE_A).await.unwrap(),
        Some(ReleaseStorageAction::DetachAudio)
    );
    assert_eq!(release_transfer_action(&db, RELEASE_B).await.unwrap(), None);

    end_release_storage_action(&db, RELEASE_A).await.unwrap();
    assert_eq!(release_transfer_action(&db, RELEASE_A).await.unwrap(), None);
    assert!(begin_release_storage_action(&db, RELEASE_A, StoredTransition::Unpin).await.unwrap());
    assert_eq!(
        release_transfer_action(&db, RELEASE_A).await.unwrap(),
        Some(ReleaseStorageAction::Unpin)
    );
}

/// A pin in flight is the download queue's active entry: a queued one is
/// no transition, an active one reads as pinning.
#[tokio::test]
async fn the_active_download_reads_as_a_pin() {
    let (db, _tmp) = library().await;
    exec(
        &db,
        "INSERT INTO download_queue (release_id, created_at, state) VALUES (?1, 7, 'queued')",
        &[RELEASE_B],
    )
    .await;
    assert_eq!(release_transfer_action(&db, RELEASE_B).await.unwrap(), None);

    exec(&db, "UPDATE download_queue SET state = 'active' WHERE release_id = ?1", &[RELEASE_B])
        .await;
    assert_eq!(
        release_transfer_action(&db, RELEASE_B).await.unwrap(),
        Some(ReleaseStorageAction::Pin)
    );
}
