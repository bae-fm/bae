//! Names order in the device's language, and a change of language reorders
//! the lists a person is looking at.

use crate::*;
use bae_records::test_fixtures::album_listing::*;
use bae_records::test_fixtures::test_records;
use bae_store::{LibraryPageWindow, LibraryPageWindows};

async fn retitle(db: &Store, titles: [&str; 3]) {
    for (album, title) in [ALBUM_ONE, ALBUM_TWO, ALBUM_THREE].into_iter().zip(titles) {
        test_records(db).rename_album_for_test(album, title).await.unwrap();
    }
}

fn first_window() -> LibraryPageWindows {
    [LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect()
}

async fn album_titles_next(
    live: &mut coven::ReconfigurableLiveQuery<
        LibraryPageWindows,
        AlbumBrowseProjection,
        bae_store::DbError,
    >,
) -> Vec<String> {
    let shown = live.next().await.expect("the query stays open").result.unwrap();
    shown.windows[0].rows.iter().map(|row| row.album.title.clone()).collect()
}

#[tokio::test]
async fn a_language_change_reorders_the_window_a_list_shows() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    retitle(&db, ["Öl", "Zebra", "Åke"]).await;
    let views = LibraryViews::new(db.clone());
    let by_title =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let mut live = views.subscribe_album_browse(&by_title, false, first_window());

    assert_eq!(
        album_titles_next(&mut live).await,
        ["Åke", "Öl", "Zebra"],
        "the root order puts å and ö with a and o"
    );
    views.set_sort_language("sv").await.unwrap();
    assert_eq!(views.sort_language().await.unwrap().as_str(), "sv");
    assert_eq!(
        album_titles_next(&mut live).await,
        ["Zebra", "Åke", "Öl"],
        "Swedish puts them after z, and the window that was showing follows"
    );
    views.set_sort_language("sv").await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), live.next()).await.is_err(),
        "the language it already has writes nothing"
    );
}

#[tokio::test]
async fn cyrillic_titles_sort_by_letter_whatever_their_case() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    retitle(&db, ["Яблоко", "арбуз", "Борщ"]).await;
    let views = LibraryViews::new(db.clone());
    views.set_sort_language("ru").await.unwrap();
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
    LibraryViews::new(db.clone()).set_sort_language("sv").await.unwrap();
    let ordered: (Vec<String>, Vec<String>) = db
        .read(|sql| {
            let ids = |query: &str| -> Result<Vec<String>, DbError> {
                Ok(sql.query(query, [], |row| row.get(0))?)
            };
            Ok((
                ids("SELECT album_id FROM release_listing ORDER BY album_title_sort_key")?,
                ids("SELECT album_id FROM album_artist_link WHERE artist_id = '4309b580-a6bf-455e-92ea-ec58ee7129e7' ORDER BY title_sort_key")?,
            ))
        })
        .await
        .unwrap();
    assert_eq!(ordered.0, [ALBUM_TWO, ALBUM_THREE, ALBUM_ONE]);
    assert_eq!(ordered.1, [ALBUM_TWO, ALBUM_THREE]);
}

#[tokio::test]
async fn a_language_that_is_not_one_is_refused_and_changes_nothing() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let views = LibraryViews::new(db.clone());
    assert!(matches!(
        views.set_sort_language("not a language").await,
        Err(SortLanguageError::Invalid(_))
    ));
    assert_eq!(views.sort_language().await.unwrap().as_str(), "und");
}

#[tokio::test]
async fn keys_written_with_other_collation_data_are_written_again_when_a_library_opens() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let views = LibraryViews::new(db.clone());
    views.settle_name_keys().await.unwrap();
    // Keys of an earlier build's data: another key, under the data version
    // that build wrote.
    db.call_bulk(|sql| {
        sql.execute("UPDATE name_sort SET data_version = data_version + 1", [])?;
        sql.execute("UPDATE album_listing SET title_sort_key = x'01'", [])?;
        Ok(())
    })
    .await
    .unwrap();

    views.settle_name_keys().await.unwrap();

    let (stale, version): (i64, i64) = db
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT (SELECT COUNT(*) FROM album_listing l JOIN albums a ON a.id = l.album_id
                         WHERE l.title_sort_key IS NOT bae_name_key(a.title, 'und')),
                        (SELECT data_version FROM name_sort)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(stale, 0, "every title key is the current data's");
    assert_eq!(version, i64::from(bae_util::name_key::DATA_VERSION));
}
