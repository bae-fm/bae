//! `release_storage_action`: a release is in one storage transition at a
//! time, its reads show it, and opening the library ends every one.

use super::super::*;
use super::*;
use crate::library::release_queue::ReleaseQueueEntry;
use bae_types::release_storage::ReleaseStorageAction;

const LIBRARY: &str = "
    INSERT INTO albums (id, title, is_compilation, created_at) VALUES
        ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('0252dedb-ee39-4547-8803-438dbeb57a64', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0),
        ('64e79a1f-404a-4c34-809a-a3cb44bf1942', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 1);
";

async fn library() -> (Database, tempfile::TempDir) {
    let (db, tmp) = temp_db().await;
    db.call(|sql| sql.execute_batch(LIBRARY).map_err(DbError::from)).await.unwrap();
    (db, tmp)
}

/// A release already in a transition refuses a second; ending it lets one
/// begin, and each release's read shows its own.
#[tokio::test]
async fn a_release_is_in_one_transition_at_a_time() {
    let (db, _tmp) = library().await;
    assert!(db
        .begin_release_storage_action(RELEASE_A, StoredTransition::DetachAudio)
        .await
        .unwrap());
    assert!(
        !db.begin_release_storage_action(RELEASE_A, StoredTransition::Unpin).await.unwrap(),
        "a release in a transition refuses another"
    );
    assert_eq!(
        db.release_transfer_action(RELEASE_A).await.unwrap(),
        Some(ReleaseStorageAction::DetachAudio)
    );
    assert_eq!(db.release_transfer_action(RELEASE_B).await.unwrap(), None);

    db.end_release_storage_action(RELEASE_A).await.unwrap();
    assert_eq!(db.release_transfer_action(RELEASE_A).await.unwrap(), None);
    assert!(db.begin_release_storage_action(RELEASE_A, StoredTransition::Upload).await.unwrap());
    assert_eq!(
        db.release_transfer_action(RELEASE_A).await.unwrap(),
        Some(ReleaseStorageAction::Upload)
    );
}

/// A pin in flight is the download queue's active entry: a queued one is
/// no transition, an active one reads as pinning.
#[tokio::test]
async fn the_active_download_reads_as_a_pin() {
    let (db, _tmp) = library().await;
    let entries =
        vec![ReleaseQueueEntry { release_id: RELEASE_B.to_string(), created_at: 7, payload: () }];
    db.call(move |sql| crate::library::queued_releases::tables::enqueue_releases_on(sql, &entries))
        .await
        .unwrap();
    assert_eq!(db.release_transfer_action(RELEASE_B).await.unwrap(), None);

    assert!(db
        .call(|sql| crate::library::queued_releases::tables::activate_queued_release_on::<()>(
            sql, RELEASE_B
        ))
        .await
        .unwrap());
    assert_eq!(
        db.release_transfer_action(RELEASE_B).await.unwrap(),
        Some(ReleaseStorageAction::Pin)
    );
}

/// Opening the library ends every transition: none outlives the process
/// that ran it.
#[tokio::test]
async fn opening_ends_every_transition() {
    let (db, _tmp) = library().await;
    db.begin_release_storage_action(RELEASE_A, StoredTransition::Unpin).await.unwrap();

    db.call(crate::library::queued_releases::tables::clear_transfers_on).await.unwrap();

    assert_eq!(db.release_transfer_action(RELEASE_A).await.unwrap(), None);
}
