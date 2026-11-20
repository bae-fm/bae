use bae_library::test_libraries::{own_profile, storage_library};
use bae_people::people::HistorySharing;
use bae_util::test_ids::test_uuid;
use std::time::Duration;

#[tokio::test]
async fn qualifying_plays_keep_their_identity_and_original_audience() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let person = own_profile(library.people()).await;
    let release = test_uuid("history-release");
    let track = test_uuid("history-track");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("history-album"), "Listening"),
        &release,
        &[(&track, "Interlude")],
    )
    .await;
    let started = bae_util::timestamp::parse_rfc3339("2026-10-18T16:30:00Z").unwrap();
    let mut private_play = people
        .begin_play(
            coven::IdSource::new_id(&coven::UuidIds),
            &track,
            Some(Duration::from_secs(40)),
            started,
            started.date(),
        )
        .await
        .unwrap()
        .expect("recorded");
    private_play.set_played(Duration::from_secs(19));
    assert!(!people.record_play(&private_play).await.unwrap());
    assert!(people.history().await.unwrap().is_empty());
    private_play.set_played(Duration::from_secs(20));
    assert!(people.record_play(&private_play).await.unwrap());
    let private = coven::Audience::Circle(person.private_circle);
    let rows = people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, private_play.entry().id);
    assert_eq!(rows[0].person, person.id);
    assert_eq!(rows[0].audience, private);
    assert_eq!(rows[0].started_at, started);
    library.people().set_history_sharing(HistorySharing::Household).await.unwrap();
    private_play.set_played(Duration::from_secs(24));
    people.record_play(&private_play).await.unwrap();
    assert_eq!(people.history().await.unwrap()[0].audience, private);
    let mut shared = people
        .begin_play(
            coven::IdSource::new_id(&coven::UuidIds),
            &track,
            Some(Duration::from_secs(100)),
            started,
            started.date(),
        )
        .await
        .unwrap()
        .expect("recorded");
    library.people().set_history_sharing(HistorySharing::Private).await.unwrap();
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
    assert_eq!((row.person, &row.audience), (person.id, &coven::Audience::Store));
    assert_eq!(row.played, Duration::from_secs(62));
    assert_ne!(row.id, private_play.entry().id);
    let mut unknown = people
        .begin_play(coven::IdSource::new_id(&coven::UuidIds), &track, None, started, started.date())
        .await
        .unwrap()
        .expect("recorded");
    unknown.set_played(Duration::from_secs(8));
    assert!(!people.record_play(&unknown).await.unwrap());
    unknown.finish_at(Duration::from_secs(8));
    assert!(people.record_play(&unknown).await.unwrap());
    library.manager().close().await.unwrap();
}

/// Off keeps no record of new plays: not on this device, not in any circle.
/// A play begun before Off keeps the audience it began with.
#[tokio::test]
async fn off_records_no_new_plays_and_earlier_plays_keep_their_audience() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let release = test_uuid("off-release");
    let track = test_uuid("off-track");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("off-album"), "Listening"),
        &release,
        &[(&track, "Interlude")],
    )
    .await;
    let person = own_profile(library.people()).await;
    let started = bae_util::timestamp::parse_rfc3339("2026-10-18T16:30:00Z").unwrap();
    let mut before = people
        .begin_play(coven::IdSource::new_id(&coven::UuidIds), &track, None, started, started.date())
        .await
        .unwrap()
        .expect("a private setting keeps plays");

    library.people().set_history_sharing(HistorySharing::Off).await.unwrap();
    let off = people
        .begin_play(coven::IdSource::new_id(&coven::UuidIds), &track, None, started, started.date())
        .await
        .unwrap();
    assert!(off.is_none());
    before.set_played(Duration::from_secs(40));
    assert!(people.record_play(&before).await.unwrap());
    let rows = people.history().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].person, &rows[0].audience),
        (person.id, &coven::Audience::Circle(person.private_circle))
    );

    library.people().set_history_sharing(HistorySharing::Household).await.unwrap();
    assert!(people
        .begin_play(coven::IdSource::new_id(&coven::UuidIds), &track, None, started, started.date())
        .await
        .unwrap()
        .is_some());
    library.manager().close().await.unwrap();
}
