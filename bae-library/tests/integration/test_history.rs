use bae_db::history::HistoryAudience;
use bae_db::people::Sharing;
use bae_library::test_libraries::{connect_storage, storage_library};
use bae_util::test_ids::test_uuid;
use std::time::Duration;

#[tokio::test]
async fn qualifying_plays_keep_their_identity_and_original_audience() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let db = library.database().clone();
    let store = library.store().clone();
    let release = test_uuid("history-release");
    let track = test_uuid("history-track");
    let medium = test_uuid("history-medium");
    store.execute_local_sql_for_test(&format!("INSERT INTO albums (id,title,created_at) VALUES ('history-album','Listening','2026-01-01'); INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{release}','history-album','2026-01-01', 0); INSERT INTO release_mediums (id,release_id,position,medium,side_count,created_at) VALUES ('{medium}','{release}',1,'cd',1,'2026-01-01'); INSERT INTO tracks (id,release_id,title,medium_id,side,track_number,position,duration_ms,created_at) VALUES ('{track}','{release}','Interlude','{medium}',1,1,0,40000,'2026-01-01');")).await.unwrap();
    let started = "2026-10-18T16:30:00Z".parse().unwrap();
    let mut local = db
        .begin_play(uuid::Uuid::new_v4(), &track, Some(Duration::from_secs(40)), started)
        .await
        .unwrap();
    local.set_played(Duration::from_secs(19));
    assert!(!db.record_play(&local).await.unwrap());
    assert!(db.history().await.unwrap().is_empty());
    local.set_played(Duration::from_secs(20));
    assert!(db.record_play(&local).await.unwrap());
    assert_eq!(db.history().await.unwrap()[0].audience, HistoryAudience::DevicePrivate);
    connect_storage(&library, &storage).await;
    let person = bae_library::test_libraries::made_profile(&library, "Robin").await;
    let private = HistoryAudience::Person {
        person: person.id,
        audience: coven::Audience::Circle(person.private_circle),
    };
    let rows = db.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, local.entry().id);
    assert_eq!(rows[0].audience, private);
    assert_eq!(rows[0].started_at, started);
    assert!(store.query_texts_for_test("SELECT id FROM local_history").await.unwrap().is_empty());
    library.set_history_sharing(Sharing::Household).await.unwrap();
    local.set_played(Duration::from_secs(24));
    db.record_play(&local).await.unwrap();
    assert_eq!(db.history().await.unwrap()[0].audience, private);
    let mut shared = db
        .begin_play(uuid::Uuid::new_v4(), &track, Some(Duration::from_secs(100)), started)
        .await
        .unwrap();
    library.set_history_sharing(Sharing::Private).await.unwrap();
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
        HistoryAudience::Person { person: person.id, audience: coven::Audience::Store }
    );
    assert_eq!(row.played, Duration::from_secs(62));
    assert_ne!(row.id, local.entry().id);
    let mut unknown = db.begin_play(uuid::Uuid::new_v4(), &track, None, started).await.unwrap();
    unknown.set_played(Duration::from_secs(8));
    assert!(!db.record_play(&unknown).await.unwrap());
    unknown.finish_at(Duration::from_secs(8));
    assert!(db.record_play(&unknown).await.unwrap());
    // A profile can arrive from another device while this device still has
    // listening from before registration. Retrying profile setup adopts it too.
    let awaiting_profile = uuid::Uuid::new_v4();
    store.execute_local_sql_for_test(&format!("INSERT INTO local_history (id, track_id, started_at, played_ms) VALUES ('{awaiting_profile}', '{track}', '2026-10-18T16:00:00Z', 30000)")).await.unwrap();
    library.create_profile("Robin").await.unwrap();
    let rows = db.history().await.unwrap();
    assert_eq!(rows.iter().find(|row| row.id == awaiting_profile).unwrap().audience, private);
    assert!(store.query_texts_for_test("SELECT id FROM local_history").await.unwrap().is_empty());
    library.close().await.unwrap();
}
