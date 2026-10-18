#![cfg(feature = "test-utils")]

use bae_core::history::HistoryAudience;
use bae_core::people::Sharing;
use bae_test_support::{connect_storage, storage_library, test_uuid};
use std::time::Duration;

#[tokio::test]
async fn qualifying_plays_keep_their_identity_and_original_audience() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, db, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    let release = test_uuid("history-release");
    let track = test_uuid("history-track");
    db.execute_local_sql_for_test(&format!("INSERT INTO albums (id,title,created_at) VALUES ('history-album','Listening','2026-01-01'); INSERT INTO releases (id,album_id,created_at) VALUES ('{release}','history-album','2026-01-01'); INSERT INTO tracks (id,release_id,title,position,duration_ms,created_at) VALUES ('{track}','{release}','Interlude',0,40000,'2026-01-01');")).await.unwrap();
    let started = "2026-10-18T16:30:00Z".parse().unwrap();
    let mut local = db
        .begin_play(
            uuid::Uuid::new_v4(),
            &track,
            &release,
            Some(Duration::from_secs(40)),
            started,
        )
        .await
        .unwrap();
    local.set_played(Duration::from_secs(19));
    assert!(!db.record_play(&local).await.unwrap());
    assert!(db.history().await.unwrap().is_empty());
    local.set_played(Duration::from_secs(20));
    assert!(db.record_play(&local).await.unwrap());
    assert_eq!(
        db.history().await.unwrap()[0].audience,
        HistoryAudience::DevicePrivate
    );
    connect_storage(&manager, &storage).await;
    let person = manager.create_profile("Robin").await.unwrap();
    let private = HistoryAudience::Person {
        person: person.id,
        audience: coven::Audience::Circle(person.private_circle),
    };
    let rows = db.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, local.entry().id);
    assert_eq!(rows[0].audience, private);
    assert_eq!(rows[0].started_at, started);
    assert!(db
        .query_texts_for_test("SELECT id FROM local_history")
        .await
        .unwrap()
        .is_empty());
    manager
        .set_history_sharing(Sharing::Household)
        .await
        .unwrap();
    local.set_played(Duration::from_secs(24));
    db.record_play(&local).await.unwrap();
    assert_eq!(db.history().await.unwrap()[0].audience, private);
    let mut shared = db
        .begin_play(
            uuid::Uuid::new_v4(),
            &track,
            &release,
            Some(Duration::from_secs(100)),
            started,
        )
        .await
        .unwrap();
    manager.set_history_sharing(Sharing::Private).await.unwrap();
    shared.set_played(Duration::from_secs(29));
    assert!(!db.record_play(&shared).await.unwrap());
    shared.set_played(Duration::from_secs(30));
    db.record_play(&shared).await.unwrap();
    shared.set_played(Duration::from_secs(62));
    db.record_play(&shared).await.unwrap();
    db.record_play(&shared).await.unwrap();
    let rows = db.history().await.unwrap();
    assert_eq!(rows.len(), 2);
    let row = rows.iter().find(|row| row.id == shared.entry().id).unwrap();
    assert_eq!(
        row.audience,
        HistoryAudience::Person {
            person: person.id,
            audience: coven::Audience::Store
        }
    );
    assert_eq!(row.played, Duration::from_secs(62));
    assert_ne!(row.id, local.entry().id);
    let mut unknown = db
        .begin_play(uuid::Uuid::new_v4(), &track, &release, None, started)
        .await
        .unwrap();
    unknown.set_played(Duration::from_secs(8));
    assert!(!db.record_play(&unknown).await.unwrap());
    unknown.finish_at(Duration::from_secs(8));
    assert!(db.record_play(&unknown).await.unwrap());
    // A profile can arrive from another device while this device still has
    // listening from before registration. Retrying profile setup adopts it too.
    let awaiting_profile = uuid::Uuid::new_v4();
    db.execute_local_sql_for_test(&format!("INSERT INTO local_history (id, track_id, release_id, started_at, played_ms) VALUES ('{awaiting_profile}', '{track}', '{release}', '2026-10-18T16:00:00Z', 30000)")).await.unwrap();
    manager.create_profile("Robin").await.unwrap();
    let rows = db.history().await.unwrap();
    assert_eq!(
        rows.iter()
            .find(|row| row.id == awaiting_profile)
            .unwrap()
            .audience,
        private
    );
    assert!(db
        .query_texts_for_test("SELECT id FROM local_history")
        .await
        .unwrap()
        .is_empty());
    manager.close().await.unwrap();
}
