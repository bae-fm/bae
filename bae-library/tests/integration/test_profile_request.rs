//! A profile asked for while the person's private circle is made: the
//! request coven's operation answers, and the write that ends it (§5.4).

use bae_db::people::{ProfileError, Sharing};
use bae_library::test_libraries::{connect_storage, requested_profile, storage_library};

/// Robin asks for her profile with sync stopped. The write journals her
/// private circle and records the request, offline too; she has no profile
/// yet, her plays stay on this device, and asking again waits on the first.
/// Once sync runs the circle lands, and one write creates her profile, moves
/// her plays into the circle and acknowledges the circle's operation (§5.4).
#[tokio::test]
async fn a_profile_asked_for_offline_is_written_once_its_circle_lands() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let database = library.database().clone();
    connect_storage(&library, &storage).await;
    library.stop_sync().await.unwrap();
    let track = bae_util::test_ids::test_uuid("waiting-track");
    let release = bae_util::test_ids::test_uuid("waiting-release");
    let medium = bae_util::test_ids::test_uuid("waiting-medium");
    let play = bae_util::test_ids::test_uuid("waiting-play");
    database.execute_local_sql_for_test(&format!("INSERT INTO albums (id,title,created_at) VALUES ('waiting-album','Album Title','2026-01-01'); INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{release}','waiting-album','2026-01-01', 0); INSERT INTO release_mediums (id,release_id,position,medium,side_count,created_at) VALUES ('{medium}','{release}',1,'cd',1,'2026-01-01'); INSERT INTO tracks (id,release_id,title,medium_id,side,track_number,position,duration_ms,created_at) VALUES ('{track}','{release}','Track Title','{medium}',1,1,0,180000,'2026-01-01'); INSERT INTO local_history (id, track_id, started_at, played_ms) VALUES ('{play}', '{track}', '2026-10-18T16:00:00Z', 30000);")).await.unwrap();

    library.create_profile("Robin").await.unwrap();
    assert!(database.profiles().await.unwrap().is_empty());
    let requested =
        database.query_texts_for_test("SELECT name FROM profile_request").await.unwrap();
    assert_eq!(requested, ["Robin"]);
    let running = database
        .query_texts_for_test(
            "SELECT kind FROM coven_operations AS o \
             JOIN profile_request AS r ON r.operation = o.operation WHERE o.outcome IS NULL",
        )
        .await
        .unwrap();
    assert_eq!(running, ["CreateCircle"]);
    assert_eq!(
        database.query_texts_for_test("SELECT id FROM local_history").await.unwrap(),
        [play.as_str()]
    );
    assert!(matches!(library.create_profile("Robin").await, Err(ProfileError::CirclePending(_))));

    let profile = requested_profile(&library).await;
    assert_eq!(profile.name, "Robin");
    assert_eq!(profile.history_sharing, Sharing::Private);
    let members = database
        .query_texts_for_test(&format!(
            "SELECT member FROM coven_circle_members WHERE circle = '{}'",
            profile.private_circle
        ))
        .await
        .unwrap();
    assert_eq!(members, [profile.id.to_string()]);
    assert_eq!(
        database
            .query_texts_for_test(&format!("SELECT audience FROM history WHERE id = '{play}'"))
            .await
            .unwrap(),
        [coven::Audience::Circle(profile.private_circle).to_string()]
    );
    assert!(database
        .query_texts_for_test("SELECT id FROM local_history")
        .await
        .unwrap()
        .is_empty());
    assert!(database
        .query_texts_for_test("SELECT name FROM profile_request")
        .await
        .unwrap()
        .is_empty());
    assert!(
        database
            .query_texts_for_test("SELECT kind FROM coven_operations")
            .await
            .unwrap()
            .is_empty(),
        "the profile's write acknowledged the circle's outcome"
    );
    library.close().await.unwrap();
}
