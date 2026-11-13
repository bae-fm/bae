use bae_library::followed::value_matching;
use bae_library::test_libraries::{connect_storage, made_profile, storage_library};
use bae_people::collections::{CollectionError, RatingTarget, Stars};
use bae_people::people::Sharing;
use bae_util::test_channels::HANG_GUARD;
use bae_util::test_ids::test_uuid;

/// The playlists as the listing shows them now: a new subscription's first
/// value is what is stored.
async fn playlists_now(store: &bae_store::Store) -> Vec<bae_views::playlists::PlaylistSummary> {
    let views = bae_views::LibraryViews::new(store.clone());
    value_matching(&mut views.subscribe_playlists(), |_| true).await.listed
}

#[tokio::test]
async fn ratings_and_playlist_edits_preserve_identity_order_and_audience() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let store = library.store().clone();
    let people = bae_people::People::new(store.clone());
    connect_storage(library.manager(), library.store(), &storage).await;
    let person = made_profile(library.manager(), library.people(), "Robin").await;
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
    let playlist =
        people.create_playlist(id, "Evening", Sharing::Private, Vec::new()).await.unwrap();
    assert_eq!(playlist.audience, coven::Audience::Circle(person.private_circle));
    let a = coven::IdSource::new_id(&coven::UuidIds);
    let b = coven::IdSource::new_id(&coven::UuidIds);
    let c = coven::IdSource::new_id(&coven::UuidIds);
    people.append_playlist_entry(id, a, first.clone()).await.unwrap();
    people.append_playlist_entry(id, b, second).await.unwrap();
    people.append_playlist_entry(id, c, first.clone()).await.unwrap();
    assert!(matches!(
        people.reorder_playlist(id, vec![b, a]).await,
        Err(CollectionError::OrderChanged)
    ));
    assert!(matches!(
        people.reorder_playlist(id, vec![a, a, c]).await,
        Err(CollectionError::OrderChanged)
    ));
    assert_eq!(
        people
            .playlist_entries(id)
            .await
            .unwrap()
            .iter()
            .map(|entry| entry.position_id)
            .collect::<Vec<_>>(),
        vec![a, b, c]
    );
    people.reorder_playlist(id, vec![c, b, a]).await.unwrap();
    let entries = people.playlist_entries(id).await.unwrap();
    assert_eq!(entries.iter().map(|entry| entry.position_id).collect::<Vec<_>>(), vec![c, b, a]);
    assert_eq!(entries[0].track_id, first);
    people.set_playlist_sharing(id, Sharing::Household).await.unwrap();
    people.rename_playlist(id, "Gathering").await.unwrap();
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
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone());
    connect_storage(library.manager(), library.store(), &storage).await;
    made_profile(library.manager(), library.people(), "Robin").await;
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
            let event = values.next().await.expect("the playlist's read stays open");
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
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    connect_storage(library.manager(), library.store(), &storage).await;
    let robin = made_profile(library.manager(), library.people(), "Robin").await;
    let people = bae_people::People::new(library.store().clone());
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
    let none = value_matching(&mut listing, |_| true).await;
    assert!(none.listed.is_empty() && none.add_targets.is_empty());
    assert!(none.can_create);

    let id = library
        .manager()
        .create_playlist("Evening", Sharing::Private, vec![second.clone(), first.clone()])
        .await
        .unwrap();
    let mut detail = views.subscribe_playlist_detail(Some(id.to_string()));
    let made = value_matching(&mut listing, |listing| !listing.listed.is_empty()).await;
    let summary = &made.listed[0];
    assert_eq!(summary.id, id);
    assert_eq!(summary.name, "Evening");
    assert_eq!(summary.sharing, Sharing::Private);
    assert!(summary.yours);
    assert_eq!(summary.owner.person, robin.id);
    assert_eq!(summary.owner.name, "Robin");
    assert_eq!(summary.track_count.count, 2);
    assert_eq!(made.add_targets, made.listed);
    let shown = detail_matching(&mut detail, |detail| detail.is_some()).await.unwrap();
    assert_eq!(shown.summary, *summary);
    assert_eq!(
        shown.entries.iter().map(|entry| entry.title.as_str()).collect::<Vec<_>>(),
        vec!["Second", "First"]
    );

    people.set_playlist_sharing(id, Sharing::Household).await.unwrap();
    people.rename_playlist(id, "Gathering").await.unwrap();
    let renamed = value_matching(&mut listing, |listing| {
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
    let order: Vec<_> = grown.entries.iter().rev().map(|entry| entry.position_id).collect();
    people.reorder_playlist(id, order.clone()).await.unwrap();
    let reordered = detail_matching(&mut detail, |detail| {
        detail.as_ref().is_some_and(|detail| detail.entries[0].position_id == appended)
    })
    .await
    .unwrap();
    assert_eq!(reordered.entries.iter().map(|entry| entry.position_id).collect::<Vec<_>>(), order);

    people.delete_playlist(id).await.unwrap();
    detail_matching(&mut detail, |detail| detail.is_none()).await;
    let gone = value_matching(&mut listing, |listing| listing.listed.is_empty()).await;
    assert!(gone.add_targets.is_empty());
    library.manager().close().await.unwrap();
}

/// Playlists list by name in the device's language, and the live listing
/// lists them again when the language changes: Swedish puts å and ä after z,
/// the root order puts them with a, and the byte order after every ASCII
/// letter.
#[tokio::test]
async fn playlists_list_by_name_in_the_devices_language() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    connect_storage(library.manager(), library.store(), &storage).await;
    made_profile(library.manager(), library.people(), "Robin").await;
    let views = bae_views::LibraryViews::new(library.store().clone());
    for name in ["Ängel", "Zebra", "Apa"] {
        library.manager().create_playlist(name, Sharing::Private, Vec::new()).await.unwrap();
    }
    let names = |listing: &bae_views::playlists::PlaylistListing| -> Vec<String> {
        listing.listed.iter().map(|playlist| playlist.name.clone()).collect()
    };

    let mut listing = views.subscribe_playlists();
    let root = value_matching(&mut listing, |listing| listing.listed.len() == 3).await;
    assert_eq!(names(&root), ["Ängel", "Apa", "Zebra"]);

    views.set_sort_language("sv").await.unwrap();
    let swedish = value_matching(&mut listing, |listing| names(listing)[0] == "Apa").await;
    assert_eq!(names(&swedish), ["Apa", "Zebra", "Ängel"]);
    library.manager().close().await.unwrap();
}

/// Rating an album or one of its tracks delivers a new album detail: the
/// person's stars on the album with them first among its raters, and on the
/// track.
#[tokio::test]
async fn rating_delivers_a_new_album_detail() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone());
    connect_storage(library.manager(), library.store(), &storage).await;
    made_profile(library.manager(), library.people(), "Robin").await;
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

    let mut values = bae_views::LibraryViews::new(library.store().clone())
        .subscribe_album_detail(Some(album.clone()));
    let first = next_detail(&mut values).await;
    assert!(first.rating.can_rate);
    assert_eq!(first.rating.mine, None);

    people.set_rating(RatingTarget::Album(album.clone()), Some(Stars::Four)).await.unwrap();
    let rated = next_detail_matching(&mut values, |detail| detail.rating.mine.is_some()).await;
    assert_eq!(rated.rating.mine, Some(Stars::Four));
    assert_eq!(rated.rating.raters, vec![bae_views::album_detail::Rater::You]);

    people.set_rating(RatingTarget::Track(track.clone()), Some(Stars::Two)).await.unwrap();
    let tracked = next_detail_matching(&mut values, |detail| {
        detail.releases[0].tracks.iter().any(|track| track.my_stars.is_some())
    })
    .await;
    assert_eq!(tracked.releases[0].tracks[0].my_stars, Some(Stars::Two));
    drop(values);
    library.manager().close().await.unwrap();
}

async fn next_detail(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<String>,
        bae_views::AlbumDetailProjection,
        bae_store::DbError,
    >,
) -> bae_views::album_detail::AlbumDetail {
    let event = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, values.next())
        .await
        .expect("the album detail arrives")
        .expect("the album detail's read stays open");
    bae_views::album::resolve_album_detail_projection(event.result.expect("the album detail reads"))
        .expect("the album is present")
}

async fn next_detail_matching(
    values: &mut coven::ReconfigurableLiveQuery<
        Option<String>,
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
