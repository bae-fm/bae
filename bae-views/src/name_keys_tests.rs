//! Names order in the device's language, and a change of language reorders
//! the lists a person is looking at.

use crate::*;
use bae_records::test_fixtures::album_listing::*;
use bae_records::test_fixtures::test_records;
use bae_store::{ListWindow, WindowRead};

async fn retitle(db: &Store, titles: [&str; 3]) {
    for (album, title) in [ALBUM_ONE, ALBUM_TWO, ALBUM_THREE].into_iter().zip(titles) {
        test_records(db).rename_album_for_test(album, title).await.unwrap();
    }
}

fn first_window() -> ListWindow {
    ListWindow::top(50)
}

async fn album_titles_next(
    live: &mut coven::ReconfigurableLiveQuery<
        ListWindow,
        WindowRead<DbAlbumAppearance>,
        bae_store::DbError,
    >,
) -> Vec<String> {
    let shown = live.next_inline().await.expect("the query stays open").result.unwrap();
    shown.rows.iter().map(|(row, _key)| row.album.title.clone()).collect()
}

#[tokio::test]
async fn a_language_change_reorders_the_window_a_list_shows() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    retitle(&db, ["Öl", "Zebra", "Åke"]).await;
    let views = LibraryViews::new(db.clone());
    let by_title =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let mut live = views.subscribe_album_window(&by_title, false, first_window());

    assert_eq!(
        album_titles_next(&mut live).await,
        ["Åke", "Öl", "Zebra"],
        "the root order puts å and ö with a and o"
    );
    test_records(&db).set_sort_language("sv").await.unwrap();
    assert_eq!(
        album_titles_next(&mut live).await,
        ["Zebra", "Åke", "Öl"],
        "Swedish puts them after z, and the window that was showing follows"
    );
    test_records(&db).set_sort_language("sv").await.unwrap();
    assert!(!live.is_marked_for_rerun(), "the language it already has writes nothing");
}

#[tokio::test]
async fn cyrillic_titles_sort_by_letter_whatever_their_case() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    retitle(&db, ["Яблоко", "арбуз", "Борщ"]).await;
    let views = LibraryViews::new(db.clone());
    test_records(&db).set_sort_language("ru").await.unwrap();
    let titles = views
        .get_album_page(
            &[AlbumSortCriterion {
                field: AlbumSortField::Title,
                direction: SortDirection::Ascending,
            }],
            0,
            50,
        )
        .await
        .unwrap()
        .into_iter()
        .map(|album| album.title)
        .collect::<Vec<_>>();
    assert_eq!(titles, ["арбуз", "Борщ", "Яблоко"]);
}

#[tokio::test]
async fn the_storage_manager_and_links_follow_the_albums_language() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    retitle(&db, ["Öl", "Zebra", "Åke"]).await;
    test_records(&db).set_sort_language("sv").await.unwrap();
    let by_title = db.read_rows(crate::name_keys::ListingAlbumsByTitleParams {}).await.unwrap();
    let linked = db
        .read_rows(crate::name_keys::ArtistLinkAlbumsByTitleParams {
            artist: "4309b580-a6bf-455e-92ea-ec58ee7129e7".into(),
        })
        .await
        .unwrap();
    let ordered: (Vec<String>, Vec<String>) = (
        by_title.into_iter().map(|row| row.album_id).collect(),
        linked.into_iter().map(|row| row.album_id).collect(),
    );
    assert_eq!(ordered.0, [ALBUM_TWO, ALBUM_THREE, ALBUM_ONE]);
    assert_eq!(ordered.1, [ALBUM_TWO, ALBUM_THREE]);
}

#[tokio::test]
async fn keys_written_with_other_collation_data_are_written_again_when_a_library_opens() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    records.settle_name_keys().await.unwrap();
    // Keys of an earlier build's data: another key, under the data version
    // that build wrote.
    records.age_name_keys_for_test().await.unwrap();
    LibraryViews::new(db.clone()).plant_keys_of_earlier_data().await.unwrap();

    records.settle_name_keys().await.unwrap();

    let keys = db.read_row(crate::name_keys::KeysOtherThanUndeterminedParams {}).await.unwrap();
    assert_eq!(keys.stale_titles, 0, "every title key is the current data's");
    assert_eq!(keys.stale_names, 0, "every artist name key is the current data's");
    assert_eq!(keys.stale_links, 0, "every album link's artist key is the current data's");
    assert_eq!(keys.data_version, bae_util::name_key::DATA_VERSION);
}
