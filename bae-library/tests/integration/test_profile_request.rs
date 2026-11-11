//! A profile asked for while the person's private circle is made: the
//! request coven's operation answers, and the write that ends it (§5.4).

use bae_library::test_libraries::{connect_storage, requested_profile, storage_library};
use bae_people::people::{HistorySharing, ProfileError};

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
    let db = library.store().clone();
    let people = bae_people::People::new(db.clone());
    connect_storage(library.manager(), library.store(), &storage).await;
    library.manager().stop_sync().await.unwrap();
    let track = bae_util::test_ids::test_uuid("waiting-track");
    let release = bae_util::test_ids::test_uuid("waiting-release");
    let play = bae_util::test_ids::test_uuid("waiting-play");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&bae_util::test_ids::test_uuid("waiting-album"), "Album Title"),
        &release,
        &[(&track, "Track Title")],
    )
    .await;
    // A play before any profile stays on this device.
    let started = "2026-10-18T16:00:00Z".parse().unwrap();
    let mut played = people
        .begin_play(play.parse().unwrap(), &track, None, started)
        .await
        .unwrap()
        .expect("recorded");
    played.set_played(std::time::Duration::from_secs(30));
    assert!(people.record_play(&played).await.unwrap());

    library.people().create_profile("Robin").await.unwrap();
    assert!(people.profiles().await.unwrap().is_empty());
    let requested = db.query_texts_for_test("SELECT name FROM profile_request").await.unwrap();
    assert_eq!(requested, ["Robin"]);
    let running = db
        .query_texts_for_test(
            "SELECT kind FROM coven_operations AS o \
             JOIN profile_request AS r ON r.operation = o.operation WHERE o.outcome IS NULL",
        )
        .await
        .unwrap();
    assert_eq!(running, ["CreateCircle"]);
    assert_eq!(
        db.query_texts_for_test("SELECT id FROM local_history").await.unwrap(),
        [play.as_str()]
    );
    assert!(matches!(
        library.people().create_profile("Robin").await,
        Err(ProfileError::CirclePending(_))
    ));

    let profile = requested_profile(library.manager(), library.people()).await;
    assert_eq!(profile.name, "Robin");
    assert_eq!(profile.history_sharing, HistorySharing::Private);
    let members = db
        .query_texts_for_test(&format!(
            "SELECT member FROM coven_circle_members WHERE circle = '{}'",
            profile.private_circle
        ))
        .await
        .unwrap();
    assert_eq!(members, [profile.id.to_string()]);
    assert_eq!(
        db.query_texts_for_test(&format!("SELECT audience FROM history WHERE id = '{play}'"))
            .await
            .unwrap(),
        [coven::Audience::Circle(profile.private_circle).to_string()]
    );
    assert!(db.query_texts_for_test("SELECT id FROM local_history").await.unwrap().is_empty());
    assert!(db.query_texts_for_test("SELECT name FROM profile_request").await.unwrap().is_empty());
    assert!(
        db.query_texts_for_test("SELECT kind FROM coven_operations WHERE kind = 'CreateCircle'")
            .await
            .unwrap()
            .is_empty(),
        "the profile's write acknowledged the circle's outcome"
    );
    library.manager().close().await.unwrap();
}
