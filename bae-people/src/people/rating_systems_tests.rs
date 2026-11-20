use crate::people::{RatedItems, RatingSystem, RatingSystems};
use crate::People;
use bae_util::test_channels::HANG_GUARD;

/// The next value `query` delivers; the test fails if none ever comes.
async fn next_systems(
    query: &mut coven::LiveQuery<RatingSystems, bae_store::DbError>,
) -> RatingSystems {
    tokio::time::timeout(HANG_GUARD, query.next_inline())
        .await
        .expect("the query delivers")
        .expect("the query stays open")
        .expect("the query reads")
}

/// A library starts with stars for albums and for tracks, which its first
/// write sets as the library is created.
#[tokio::test]
async fn a_new_library_rates_albums_and_tracks_with_stars() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let people = People::new(db, coven::FileBudget::for_test());

    let mut systems = people.subscribe_rating_systems();

    assert_eq!(
        next_systems(&mut systems).await,
        RatingSystems { albums: RatingSystem::Stars, tracks: RatingSystem::Stars }
    );
}

/// The library's creator is its admin: each change sets the system of the
/// items it names, keeps the other, and reaches the live query.
#[tokio::test]
async fn an_admin_sets_each_rating_system_on_its_own() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let people = People::new(db, coven::FileBudget::for_test());
    let mut systems = people.subscribe_rating_systems();
    next_systems(&mut systems).await;

    people.set_rating_system(RatedItems::Albums, RatingSystem::Heart).await.unwrap();
    assert_eq!(
        next_systems(&mut systems).await,
        RatingSystems { albums: RatingSystem::Heart, tracks: RatingSystem::Stars }
    );

    people.set_rating_system(RatedItems::Tracks, RatingSystem::Hidden).await.unwrap();
    assert_eq!(
        next_systems(&mut systems).await,
        RatingSystems { albums: RatingSystem::Heart, tracks: RatingSystem::Hidden }
    );
}
