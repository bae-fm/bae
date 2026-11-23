use bae_library::followed::query_matching;
use bae_library::test_libraries::{own_profile, storage_library};
use bae_people::collections::{RatingTarget, Stars};
use bae_people::people::Sharing;
use bae_util::test_channels::HANG_GUARD;
use bae_util::test_ids::test_uuid;

/// The playlists as the listing shows them now: a new subscription's first
/// value is what is stored.
async fn playlists_now(store: &bae_store::Store) -> Vec<bae_views::playlists::PlaylistSummary> {
    let views = bae_views::LibraryViews::new(store.clone());
    query_matching(&mut views.subscribe_playlists(), |_| true).await.listed
}

#[tokio::test]
async fn ratings_and_playlist_edits_preserve_identity_order_and_audience() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let store = library.store().clone();
    let people = bae_people::People::new(store.clone(), coven::FileBudget::for_test());
    let person = own_profile(library.people()).await;
    let release = test_uuid("collection-release");
    let first = test_uuid("collection-first");
    let second = test_uuid("collection-second");
    let album = test_uuid("collection-album");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&album, "Collection"),
        &release,
        &[(&first, "First"), (&second, "Second")],
    )
    .await;
    let target = RatingTarget::Track(first.clone());
    people.set_rating(target.clone(), Some(Stars::Three)).await.unwrap();
    people.set_rating(target.clone(), Some(Stars::Five)).await.unwrap();
    let ratings = people.ratings().await.unwrap();
    assert_eq!(ratings.len(), 1);
    assert_eq!(ratings[0].person, person.id);
    assert_eq!(ratings[0].stars, Stars::Five);
    people.set_rating(target, None).await.unwrap();
    assert!(people.ratings().await.unwrap().is_empty());
    people.set_rating(RatingTarget::Album(album.clone()), Some(Stars::Two)).await.unwrap();

    let id = coven::IdSource::new_id(&coven::UuidIds);
    let playlist = people
        .create_playlist(id, "Evening", Sharing::Private, Vec::new(), Vec::new().into())
        .await
        .unwrap();
    assert_eq!(playlist.audience, coven::Audience::Circle(person.private_circle));
    let a = coven::IdSource::new_id(&coven::UuidIds);
    let b = coven::IdSource::new_id(&coven::UuidIds);
    let c = coven::IdSource::new_id(&coven::UuidIds);
    people.append_playlist_entry(id, a, first.clone()).await.unwrap();
    people.append_playlist_entry(id, b, second).await.unwrap();
    people.append_playlist_entry(id, c, first.clone()).await.unwrap();
    let order = |entries: Vec<bae_people::collections::PlaylistEntry>| {
        entries.iter().map(|entry| entry.position_id).collect::<Vec<_>>()
    };
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![a, b, c]);
    // move_playlist_entry puts the entry just before or just after the entry
    // it names, or one place up or down, and keeps it in the playlist at
    // either end. It refuses a move that names an entry the playlist lacks,
    // and the order stays as it was.
    use bae_people::collections::{BesideEntry, EntryPlace};
    people.move_playlist_entry(id, b, EntryPlace::Beside(BesideEntry::Before(a))).await.unwrap();
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![b, a, c]);
    people.move_playlist_entry(id, b, EntryPlace::Beside(BesideEntry::After(c))).await.unwrap();
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![a, c, b]);
    people.move_playlist_entry(id, b, EntryPlace::Down).await.unwrap();
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![a, c, b]);
    let nil = uuid::Uuid::nil();
    assert!(people.move_playlist_entry(id, nil, EntryPlace::Up).await.is_err());
    assert!(people
        .move_playlist_entry(id, a, EntryPlace::Beside(BesideEntry::After(nil)))
        .await
        .is_err());
    assert!(people
        .move_playlist_entry(id, nil, EntryPlace::Beside(BesideEntry::Before(a)))
        .await
        .is_err());
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![a, c, b]);
    people.move_playlist_entry(id, c, EntryPlace::Up).await.unwrap();
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![c, a, b]);
    people.move_playlist_entry(id, c, EntryPlace::Up).await.unwrap();
    assert_eq!(order(people.playlist_entries(id).await.unwrap()), vec![c, a, b]);
    people.move_playlist_entry(id, a, EntryPlace::Down).await.unwrap();
    let entries = people.playlist_entries(id).await.unwrap();
    assert_eq!(entries.iter().map(|entry| entry.position_id).collect::<Vec<_>>(), vec![c, b, a]);
    assert_eq!(entries[0].track_id, first);
    people.edit_playlist(id, "Gathering", Sharing::Household).await.unwrap();
    let playlists = playlists_now(library.store()).await;
    assert_eq!(playlists[0].sharing, Sharing::Household);
    assert_eq!(playlists[0].name, "Gathering");
    // edit_playlist refuses a blank name and changes neither the name nor who
    // sees the playlist.
    assert!(people.edit_playlist(id, "  ", Sharing::Private).await.is_err());
    let playlists = playlists_now(library.store()).await;
    assert_eq!(playlists[0].sharing, Sharing::Household);
    assert_eq!(playlists[0].name, "Gathering");
    assert_eq!(people.playlist_entries(id).await.unwrap(), entries);
    people.remove_playlist_entry(id, b).await.unwrap();
    assert_eq!(people.playlist_entries(id).await.unwrap().len(), 2);
    people.delete_playlist(id).await.unwrap();
    assert!(playlists_now(library.store()).await.is_empty());
    assert!(people.playlist_entries(id).await.unwrap().is_empty());
    library.manager().close().await.unwrap();
}

#[tokio::test]
async fn a_playlist_is_made_holding_its_tracks_or_not_at_all() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let release = test_uuid("made-release");
    let first = test_uuid("made-first");
    let second = test_uuid("made-second");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("made-album"), "Made"),
        &release,
        &[(&first, "First"), (&second, "Second")],
    )
    .await;

    let id = library
        .manager()
        .create_playlist(
            "Evening",
            Sharing::Private,
            vec![second.clone(), first.clone(), second.clone()],
        )
        .await
        .unwrap();
    let tracks: Vec<String> =
        people.playlist_entries(id).await.unwrap().into_iter().map(|e| e.track_id).collect();
    assert_eq!(tracks, vec![second.clone(), first.clone(), second.clone()]);

    let unknown = test_uuid("made-unknown");
    let refused =
        library.manager().create_playlist("Morning", Sharing::Private, vec![first, unknown]).await;
    assert!(refused.is_err());
    let names: Vec<String> =
        playlists_now(library.store()).await.into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["Evening".to_string()]);
    library.manager().close().await.unwrap();
}

/// Appending several tracks puts them after the playlist's last entry in the
/// order given, and appends none of them when one is not in the library.
#[tokio::test]
async fn several_tracks_are_added_to_a_playlist_together_or_not_at_all() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let first = test_uuid("several-first");
    let second = test_uuid("several-second");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("several-album"), "Several"),
        &test_uuid("several-release"),
        &[(&first, "First"), (&second, "Second")],
    )
    .await;
    let id = library
        .manager()
        .create_playlist("Evening", Sharing::Private, vec![first.clone()])
        .await
        .unwrap();
    let tracks = || async {
        people
            .playlist_entries(id)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.track_id)
            .collect::<Vec<_>>()
    };

    library
        .manager()
        .append_playlist_entries(id, vec![second.clone(), first.clone()])
        .await
        .unwrap();
    assert_eq!(tracks().await, vec![first.clone(), second.clone(), first.clone()]);

    let refused = library
        .manager()
        .append_playlist_entries(id, vec![second, test_uuid("several-unknown")])
        .await;
    assert!(refused.is_err());
    assert_eq!(tracks().await.len(), 3);
    library.manager().close().await.unwrap();
}

/// Picked tracks make a playlist, and go onto one, in the order their
/// release lists them, whatever order the picks came in; a pick the library
/// lacks makes a new playlist hold the rest and adds none of them to an
/// existing one.
#[tokio::test]
async fn picked_tracks_go_into_a_playlist_in_their_release_s_order() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let first = test_uuid("picked-first");
    let second = test_uuid("picked-second");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("picked-album"), "Picked"),
        &test_uuid("picked-release"),
        &[(&first, "First"), (&second, "Second")],
    )
    .await;
    let picked = |ids: &[&String]| {
        bae_records::TrackList::Picked(ids.iter().map(|id| id.to_string()).collect())
    };
    let tracks = |id| {
        let people = people.clone();
        async move {
            people
                .playlist_entries(id)
                .await
                .unwrap()
                .into_iter()
                .map(|entry| entry.track_id)
                .collect::<Vec<_>>()
        }
    };

    let id = library
        .manager()
        .create_playlist("Evening", Sharing::Private, picked(&[&second, &first]))
        .await
        .unwrap();
    library.manager().append_playlist_entries(id, picked(&[&second, &first])).await.unwrap();
    assert_eq!(
        tracks(id).await,
        vec![first.clone(), second.clone(), first.clone(), second.clone()]
    );

    let unknown = test_uuid("picked-unknown");
    let refused =
        library.manager().append_playlist_entries(id, picked(&[&second, &unknown, &first])).await;
    assert!(refused.is_err());
    assert_eq!(tracks(id).await.len(), 4);
    let morning = library
        .manager()
        .create_playlist("Morning", Sharing::Private, picked(&[&second, &unknown, &first]))
        .await
        .unwrap();
    assert_eq!(tracks(morning).await, vec![first.clone(), second.clone()]);
    library.manager().close().await.unwrap();
}

/// Inserting picked tracks puts them just before or just after the entry the
/// place names, in their release's order; a place beside an entry the
/// playlist lacks, or a pick the library lacks, adds none of them.
#[tokio::test]
async fn picked_tracks_go_into_a_playlist_at_the_place_they_are_dropped() {
    use bae_people::collections::{BesideEntry, InsertPlace};
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let first = test_uuid("inserted-first");
    let second = test_uuid("inserted-second");
    let third = test_uuid("inserted-third");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("inserted-album"), "Inserted"),
        &test_uuid("inserted-release"),
        &[(&first, "First"), (&second, "Second"), (&third, "Third")],
    )
    .await;
    let picked = |ids: &[&String]| {
        bae_records::TrackList::Picked(ids.iter().map(|id| id.to_string()).collect())
    };
    let id = library
        .manager()
        .create_playlist("Evening", Sharing::Private, vec![third.clone(), third.clone()])
        .await
        .unwrap();
    let entries = || async { people.playlist_entries(id).await.unwrap() };
    let tracks =
        || async { entries().await.into_iter().map(|entry| entry.track_id).collect::<Vec<_>>() };
    let made = entries().await;
    let (top, bottom) = (made[0].position_id, made[1].position_id);

    library
        .manager()
        .insert_playlist_entries(
            id,
            picked(&[&second, &first]),
            InsertPlace::Beside(BesideEntry::After(top)),
        )
        .await
        .unwrap();
    assert_eq!(tracks().await, vec![third.clone(), first.clone(), second.clone(), third.clone()]);
    library
        .manager()
        .insert_playlist_entries(
            id,
            picked(&[&first]),
            InsertPlace::Beside(BesideEntry::Before(top)),
        )
        .await
        .unwrap();
    library
        .manager()
        .insert_playlist_entries(
            id,
            picked(&[&second]),
            InsertPlace::Beside(BesideEntry::After(bottom)),
        )
        .await
        .unwrap();
    assert_eq!(
        tracks().await,
        vec![
            first.clone(),
            third.clone(),
            first.clone(),
            second.clone(),
            third.clone(),
            second.clone()
        ]
    );

    let unknown = test_uuid("inserted-unknown");
    let missing_place = library
        .manager()
        .insert_playlist_entries(
            id,
            picked(&[&first]),
            InsertPlace::Beside(BesideEntry::Before(uuid::Uuid::nil())),
        )
        .await;
    assert!(matches!(missing_place, Err(bae_people::collections::CollectionError::Missing)));
    let missing_track = library
        .manager()
        .insert_playlist_entries(
            id,
            picked(&[&first, &unknown]),
            InsertPlace::Beside(BesideEntry::After(top)),
        )
        .await;
    assert!(missing_track.is_err());
    assert_eq!(tracks().await.len(), 6);
    library
        .manager()
        .insert_playlist_entries(
            id,
            picked(&[&third]),
            InsertPlace::Beside(BesideEntry::Before(bottom)),
        )
        .await
        .unwrap();
    let after = entries().await;
    assert_eq!(after[5].position_id, bottom);
    assert_eq!(after[4].track_id, third);
    library.manager().close().await.unwrap();
}

/// A playlist with no entries takes inserted picks at its start, the one
/// place it has, in their release's order; once it holds entries, the start
/// is before its first.
#[tokio::test]
async fn picked_tracks_go_into_an_empty_playlist_from_its_start() {
    use bae_people::collections::InsertPlace;
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let first = test_uuid("started-first");
    let second = test_uuid("started-second");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("started-album"), "Started"),
        &test_uuid("started-release"),
        &[(&first, "First"), (&second, "Second")],
    )
    .await;
    let picked = |ids: &[&String]| {
        bae_records::TrackList::Picked(ids.iter().map(|id| id.to_string()).collect())
    };
    let id = library
        .manager()
        .create_playlist("Evening", Sharing::Private, Vec::<String>::new())
        .await
        .unwrap();
    let views = bae_views::LibraryViews::new(library.store().clone());
    let mut detail = views.subscribe_playlist_detail(Some(id.to_string()));
    let shown = detail_matching(&mut detail, |detail| detail.is_some()).await.unwrap();
    assert!(shown.empty, "the page of a playlist with no entries takes drops at its start");
    let tracks = || async {
        people
            .playlist_entries(id)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.track_id)
            .collect::<Vec<_>>()
    };

    library
        .manager()
        .insert_playlist_entries(id, picked(&[&second, &first]), InsertPlace::Start)
        .await
        .unwrap();
    assert_eq!(tracks().await, vec![first.clone(), second.clone()]);
    detail_matching(&mut detail, |detail| detail.as_ref().is_some_and(|detail| !detail.empty))
        .await;
    library
        .manager()
        .insert_playlist_entries(id, picked(&[&second]), InsertPlace::Start)
        .await
        .unwrap();
    assert_eq!(tracks().await, vec![second.clone(), first.clone(), second.clone()]);
    library.manager().close().await.unwrap();
}

/// The playlists query computes each playlist's summary in SQL from its
/// entries: it counts them, adds up their time, and picks the covers of the
/// first four different releases when its entries are on that many, else its
/// first entry's cover; and Add to playlist lists only the person's own.
#[tokio::test]
async fn a_playlists_summary_comes_counted_with_its_row() {
    use bae_views::playlists::PlaylistCover;
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let tracks: Vec<String> = (0..5).map(|n| test_uuid(&format!("summary-track-{n}"))).collect();
    for (n, track) in tracks.iter().enumerate() {
        bae_records::test_fixtures::album_with_tracks(
            library.records(),
            (&test_uuid(&format!("summary-album-{n}")), &format!("Album {n}")),
            &test_uuid(&format!("summary-release-{n}")),
            &[(track, &format!("Track {n}"))],
        )
        .await;
    }
    let playlist = async |name: &str, entries: &[usize]| {
        let id = coven::IdSource::new_id(&coven::UuidIds);
        people
            .create_playlist(id, name, Sharing::Private, Vec::new(), Vec::new().into())
            .await
            .unwrap();
        for &entry in entries {
            let position = coven::IdSource::new_id(&coven::UuidIds);
            people.append_playlist_entry(id, position, tracks[entry].clone()).await.unwrap();
        }
    };
    playlist("Many", &[0, 0, 1, 2, 3, 4]).await;
    playlist("Few", &[0, 0, 1]).await;
    playlist("Empty", &[]).await;

    let views = bae_views::LibraryViews::new(library.store().clone());
    let listing = query_matching(&mut views.subscribe_playlists(), |_| true).await;
    let summary = |name: &str| listing.listed.iter().find(|p| p.name == name).unwrap().clone();
    let (many, few, empty) = (summary("Many"), summary("Few"), summary("Empty"));
    assert_eq!((many.track_count.count, many.total_duration_ms), (6, 6 * 180_000));
    assert_eq!(many.cover, PlaylistCover::Grid([None, None, None, None]));
    assert_eq!((few.track_count.count, few.total_duration_ms), (3, 3 * 180_000));
    assert_eq!(few.cover, PlaylistCover::Single(None));
    assert_eq!((empty.track_count.count, empty.total_duration_ms), (0, 0));
    assert_eq!(listing.add_targets, listing.listed);
    library.manager().close().await.unwrap();
}

/// The detail's next value that `matches`, failing when none comes.
async fn detail_matching(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<String>,
        Option<bae_views::playlists::PlaylistDetail>,
        bae_store::DbError,
    >,
    matches: impl Fn(&Option<bae_views::playlists::PlaylistDetail>) -> bool,
) -> Option<bae_views::playlists::PlaylistDetail> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let event = values.next_inline().await.expect("the playlist's read stays open");
            let detail = event.result.expect("the playlist reads");
            if matches(&detail) {
                return detail;
            }
        }
    })
    .await
    .expect("the playlist's page never showed the value waited for")
}

/// Both playlist reads follow a playlist as it is made, renamed, shared,
/// grown, reordered and deleted, and list the person's own playlists beside
/// the household's.
#[tokio::test]
async fn the_playlist_reads_follow_each_change_to_a_playlist() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let robin = own_profile(library.people()).await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let views = bae_views::LibraryViews::new(library.store().clone());
    let first = test_uuid("reads-first");
    let second = test_uuid("reads-second");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("reads-album"), "Reads"),
        &test_uuid("reads-release"),
        &[(&first, "First"), (&second, "Second")],
    )
    .await;

    let mut listing = views.subscribe_playlists();
    let none = query_matching(&mut listing, |_| true).await;
    assert!(none.listed.is_empty() && none.add_targets.is_empty());
    assert_eq!(none.add_target_count, 0);
    assert!(none.can_create);

    let id = library
        .manager()
        .create_playlist("Evening", Sharing::Private, vec![second.clone(), first.clone()])
        .await
        .unwrap();
    let mut detail = views.subscribe_playlist_detail(Some(id.to_string()));
    let made = query_matching(&mut listing, |listing| !listing.listed.is_empty()).await;
    let summary = &made.listed[0];
    assert_eq!(summary.id, id);
    assert_eq!(summary.name, "Evening");
    assert_eq!(summary.sharing, Sharing::Private);
    assert!(summary.yours);
    assert_eq!(summary.owner.person, robin.id);
    assert_eq!(summary.owner.name, bae_store::test_devices::TEST_CREATOR);
    assert_eq!(summary.track_count.count, 2);
    assert_eq!(made.add_targets, made.listed);
    assert_eq!(made.add_target_count, 1, "the listing counts the person's own playlists");
    let shown = detail_matching(&mut detail, |detail| detail.is_some()).await.unwrap();
    assert_eq!(shown.summary, *summary);
    assert_eq!(
        shown.entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(),
        vec!["Second", "First"]
    );

    people.edit_playlist(id, "Gathering", Sharing::Household).await.unwrap();
    let renamed = query_matching(&mut listing, |listing| {
        listing.listed[0].name == "Gathering" && listing.listed[0].sharing == Sharing::Household
    })
    .await;
    assert_eq!(renamed.listed.len(), 1);
    detail_matching(&mut detail, |detail| {
        detail.as_ref().is_some_and(|detail| detail.summary.name == "Gathering")
    })
    .await;

    let appended = library.manager().append_playlist_entry(id, first.clone()).await.unwrap();
    let grown = detail_matching(&mut detail, |detail| {
        detail.as_ref().is_some_and(|detail| detail.entries.len() == 3)
    })
    .await
    .unwrap();
    assert_eq!(grown.summary.track_count.count, 3);
    let mut order: Vec<_> = grown.entries.iter().map(|entry| entry.position_id).collect();
    order.rotate_right(1);
    people
        .move_playlist_entry(
            id,
            appended,
            bae_people::collections::EntryPlace::Beside(
                bae_people::collections::BesideEntry::Before(order[1]),
            ),
        )
        .await
        .unwrap();
    let reordered = detail_matching(&mut detail, |detail| {
        detail.as_ref().is_some_and(|detail| detail.entries[0].position_id == appended)
    })
    .await
    .unwrap();
    assert_eq!(reordered.entries.iter().map(|entry| entry.position_id).collect::<Vec<_>>(), order);
    let moves: Vec<_> = reordered
        .entries
        .iter()
        .map(|entry| (entry.number, entry.can_move_up, entry.can_move_down))
        .collect();
    assert_eq!(
        moves,
        vec![(1, false, true), (2, true, true), (3, true, false)],
        "each entry says its number from 1, and whether one comes before it and after it"
    );

    people.delete_playlist(id).await.unwrap();
    detail_matching(&mut detail, |detail| detail.is_none()).await;
    let gone = query_matching(&mut listing, |listing| listing.listed.is_empty()).await;
    assert!(gone.add_targets.is_empty());
    assert_eq!(gone.total, 0);
    library.manager().close().await.unwrap();
}

/// Playlists list by name in the device's language, and the live listing
/// lists them again when the language changes: Swedish puts å and ä after z,
/// the root order puts them with a, and the byte order after every ASCII
/// letter.
#[tokio::test]
async fn playlists_list_by_name_in_the_devices_language() {
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let views = bae_views::LibraryViews::new(library.store().clone());
    for name in ["Ängel", "Zebra", "Apa"] {
        library.manager().create_playlist(name, Sharing::Private, Vec::new()).await.unwrap();
    }
    let names = |listing: &bae_views::playlists::PlaylistListing| -> Vec<String> {
        listing.listed.iter().map(|playlist| playlist.name.clone()).collect()
    };

    let mut listing = views.subscribe_playlists();
    let root = query_matching(&mut listing, |listing| listing.listed.len() == 3).await;
    assert_eq!(names(&root), ["Ängel", "Apa", "Zebra"]);
    assert_eq!(root.total, 3);

    library.records().set_sort_language("sv").await.unwrap();
    let swedish = query_matching(&mut listing, |listing| names(listing)[0] == "Apa").await;
    assert_eq!(names(&swedish), ["Apa", "Zebra", "Ängel"]);
    library.manager().close().await.unwrap();
}

/// Rating an album or one of its tracks delivers a new album detail: the
/// person's stars on the album with them first among its raters, and on the
/// track.
#[tokio::test]
async fn rating_delivers_a_new_album_detail() {
    use bae_views::album_detail::{AlbumRating, Rater, TrackRating};
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let release = test_uuid("rating-release");
    let track = test_uuid("rating-track");
    let album = test_uuid("rating-album");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&album, "Rated"),
        &release,
        &[(&track, "Only")],
    )
    .await;

    let mut values = bae_views::LibraryViews::new(library.store().clone()).subscribe_album_detail(
        Some(bae_views::album_detail::AlbumDetailRequest::album(album.clone())),
    );
    let first = next_detail(&mut values).await;
    assert_eq!(first.rating, AlbumRating::Stars { mine: None, raters: vec![] });

    people.set_rating(RatingTarget::Album(album.clone()), Some(Stars::Four)).await.unwrap();
    let rated = next_detail_matching(&mut values, |detail| {
        detail.rating != AlbumRating::Stars { mine: None, raters: vec![] }
    })
    .await;
    assert_eq!(
        rated.rating,
        AlbumRating::Stars { mine: Some(Stars::Four), raters: vec![Rater::You] }
    );

    people.set_rating(RatingTarget::Track(track.clone()), Some(Stars::Two)).await.unwrap();
    let tracked = next_detail_matching(&mut values, |detail| {
        detail.releases[0].tracks.iter().any(|track| track.rating != TrackRating::Stars(None))
    })
    .await;
    assert_eq!(tracked.releases[0].tracks[0].rating, TrackRating::Stars(Some(Stars::Two)));
    drop(values);
    library.manager().close().await.unwrap();
}

/// Each rating system draws the album's and its tracks' ratings its own way
/// from the same stored stars: a heart is five stars and no heart is no
/// rating, and *None* shows none.
#[tokio::test]
async fn the_rating_systems_shape_the_album_detail_s_ratings() {
    use bae_people::people::{RatedItems, RatingSystem};
    use bae_views::album_detail::{AlbumRating, Hearter, Rater, TrackRating};
    let directory = bae_util::test_files::TestFolder::new();
    let (library, _storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone(), coven::FileBudget::for_test());
    let release = test_uuid("heart-release");
    let track = test_uuid("heart-track");
    let album = test_uuid("heart-album");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&album, "Hearted"),
        &release,
        &[(&track, "Only")],
    )
    .await;
    let mut values = bae_views::LibraryViews::new(library.store().clone()).subscribe_album_detail(
        Some(bae_views::album_detail::AlbumDetailRequest::album(album.clone())),
    );
    next_detail(&mut values).await;

    people.set_rating_system(RatedItems::Albums, RatingSystem::Heart).await.unwrap();
    people.set_rating_system(RatedItems::Tracks, RatingSystem::Heart).await.unwrap();
    let unhearted = next_detail_matching(&mut values, |detail| {
        detail.rating == AlbumRating::Heart { hearted: false, hearters: None }
            && detail.releases[0].tracks[0].rating == TrackRating::Heart(false)
    })
    .await;
    assert_eq!(unhearted.releases.len(), 1);

    people.set_hearted(RatingTarget::Album(album.clone()), true).await.unwrap();
    people.set_hearted(RatingTarget::Track(track.clone()), true).await.unwrap();
    next_detail_matching(&mut values, |detail| {
        detail.rating == AlbumRating::Heart { hearted: true, hearters: Some(vec![Hearter::You]) }
            && detail.releases[0].tracks[0].rating == TrackRating::Heart(true)
    })
    .await;

    // The heart is five stars under *Stars*.
    people.set_rating_system(RatedItems::Albums, RatingSystem::Stars).await.unwrap();
    next_detail_matching(&mut values, |detail| {
        detail.rating == AlbumRating::Stars { mine: Some(Stars::Five), raters: vec![Rater::You] }
    })
    .await;

    // Taking the heart off clears the rating.
    people.set_hearted(RatingTarget::Album(album.clone()), false).await.unwrap();
    next_detail_matching(&mut values, |detail| {
        detail.rating == AlbumRating::Stars { mine: None, raters: vec![] }
    })
    .await;

    people.set_rating_system(RatedItems::Albums, RatingSystem::Hidden).await.unwrap();
    next_detail_matching(&mut values, |detail| detail.rating == AlbumRating::Hidden).await;
    drop(values);
    library.manager().close().await.unwrap();
}

async fn next_detail(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<bae_views::album_detail::AlbumDetailRequest>,
        bae_views::AlbumDetailProjection,
        bae_store::DbError,
    >,
) -> bae_views::album_detail::AlbumDetail {
    let event = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, values.next_inline())
        .await
        .expect("the album detail arrives")
        .expect("the album detail's read stays open");
    bae_views::album::resolve_album_detail_projection(event.result.expect("the album detail reads"))
        .expect("the album is present")
}

async fn next_detail_matching(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<bae_views::album_detail::AlbumDetailRequest>,
        bae_views::AlbumDetailProjection,
        bae_store::DbError,
    >,
    wanted: impl Fn(&bae_views::album_detail::AlbumDetail) -> bool,
) -> bae_views::album_detail::AlbumDetail {
    loop {
        let detail = next_detail(values).await;
        if wanted(&detail) {
            return detail;
        }
    }
}
