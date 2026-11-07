use bae_library::test_libraries::{connect_storage, storage_library};
use bae_people::history::HistoryAudience;
use bae_people::people::Sharing;
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
    let store = library.store().clone();
    let people = bae_people::People::new(store.clone());
    let release = test_uuid("history-release");
    let track = test_uuid("history-track");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("history-album"), "Listening"),
        &release,
        &[(&track, "Interlude")],
    )
    .await;
    let started = "2026-10-18T16:30:00Z".parse().unwrap();
    let mut local = people
        .begin_play(uuid::Uuid::new_v4(), &track, Some(Duration::from_secs(40)), started)
        .await
        .unwrap();
    local.set_played(Duration::from_secs(19));
    assert!(!people.record_play(&local).await.unwrap());
    assert!(people.history().await.unwrap().is_empty());
    local.set_played(Duration::from_secs(20));
    assert!(people.record_play(&local).await.unwrap());
    assert_eq!(people.history().await.unwrap()[0].audience, HistoryAudience::DevicePrivate);
    connect_storage(library.manager(), library.store(), &storage).await;
    let person =
        bae_library::test_libraries::made_profile(library.manager(), library.people(), "Robin")
            .await;
    let private = HistoryAudience::Person {
        person: person.id,
        audience: coven::Audience::Circle(person.private_circle),
    };
    let rows = people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, local.entry().id);
    assert_eq!(rows[0].audience, private);
    assert_eq!(rows[0].started_at, started);
    assert!(store.query_texts_for_test("SELECT id FROM local_history").await.unwrap().is_empty());
    library.people().set_history_sharing(Sharing::Household).await.unwrap();
    local.set_played(Duration::from_secs(24));
    people.record_play(&local).await.unwrap();
    assert_eq!(people.history().await.unwrap()[0].audience, private);
    let mut shared = people
        .begin_play(uuid::Uuid::new_v4(), &track, Some(Duration::from_secs(100)), started)
        .await
        .unwrap();
    library.people().set_history_sharing(Sharing::Private).await.unwrap();
    shared.set_played(Duration::from_secs(29));
    assert!(!people.record_play(&shared).await.unwrap());
    shared.set_played(Duration::from_secs(30));
    people.record_play(&shared).await.unwrap();
    shared.set_played(Duration::from_secs(62));
    people.record_play(&shared).await.unwrap();
    people.record_play(&shared).await.unwrap();
    let rows = people.history().await.unwrap();
    assert_eq!(rows.len(), 2);
    let row = rows.iter().find(|row| row.id == shared.entry().id).unwrap();
    assert_eq!(
        row.audience,
        HistoryAudience::Person { person: person.id, audience: coven::Audience::Store }
    );
    assert_eq!(row.played, Duration::from_secs(62));
    assert_ne!(row.id, local.entry().id);
    let mut unknown = people.begin_play(uuid::Uuid::new_v4(), &track, None, started).await.unwrap();
    unknown.set_played(Duration::from_secs(8));
    assert!(!people.record_play(&unknown).await.unwrap());
    unknown.finish_at(Duration::from_secs(8));
    assert!(people.record_play(&unknown).await.unwrap());
    library.manager().close().await.unwrap();
}

/// A profile can arrive from another device while this device still has
/// listening from before registration. Retrying profile setup adopts it too.
#[test]
fn retrying_profile_setup_adopts_listening_from_before_the_profile_arrived() {
    bae_library::test_libraries::run_two_device_test(async |devices| {
        let release = test_uuid("arriving-release");
        let track = test_uuid("arriving-track");
        bae_records::test_fixtures::album_with_tracks(
            devices.a().records(),
            (&test_uuid("arriving-album"), "Listening"),
            &release,
            &[(&track, "Interlude")],
        )
        .await;
        devices.converge("SELECT id FROM tracks").await;
        devices.b().stop_sync().await;
        let people = devices.b().people();
        let started = "2026-10-18T16:00:00Z".parse().unwrap();
        let mut awaiting_profile = people
            .begin_play(uuid::Uuid::new_v4(), &track, Some(Duration::from_secs(40)), started)
            .await
            .unwrap();
        awaiting_profile.set_played(Duration::from_secs(30));
        assert!(people.record_play(&awaiting_profile).await.unwrap());

        let person = bae_library::test_libraries::made_profile(
            devices.a().manager(),
            devices.a().people(),
            "Robin",
        )
        .await;
        devices.b().store().start_sync().await.unwrap();
        devices
            .settle("the profile on both devices", async |_, store| {
                !bae_people::People::new(store.clone()).profiles().await.unwrap().is_empty()
            })
            .await;
        let b = devices.b().store();
        assert!(!b.query_texts_for_test("SELECT id FROM local_history").await.unwrap().is_empty());

        devices.b().people().create_profile("Robin").await.unwrap();
        let rows = people.history().await.unwrap();
        let private = HistoryAudience::Person {
            person: person.id,
            audience: coven::Audience::Circle(person.private_circle),
        };
        let adopted = rows.iter().find(|row| row.id == awaiting_profile.entry().id).unwrap();
        assert_eq!(adopted.audience, private);
        assert!(b.query_texts_for_test("SELECT id FROM local_history").await.unwrap().is_empty());
    });
}
