//! `release_storage_action`: a release is in one storage transition at a
//! time, and its reads show it.

use super::*;
use bae_db::test_fixtures::*;

const LIBRARY: &str = "
    INSERT INTO albums (id, title, is_compilation, created_at) VALUES
        ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('0252dedb-ee39-4547-8803-438dbeb57a64', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0),
        ('64e79a1f-404a-4c34-809a-a3cb44bf1942', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 1);
";

async fn library() -> (Database, tempfile::TempDir) {
    let (db, tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    db.call(|sql| sql.execute_batch(LIBRARY).map_err(DbError::from)).await.unwrap();
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
    assert!(begin_release_storage_action(&db, RELEASE_A, StoredTransition::Upload).await.unwrap());
    assert_eq!(
        release_transfer_action(&db, RELEASE_A).await.unwrap(),
        Some(ReleaseStorageAction::Upload)
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
