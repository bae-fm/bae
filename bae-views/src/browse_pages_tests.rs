//! The pages a media browser reads of an album's tracks and of a work's
//! children, each read once at its offset, and the live count of each list,
//! over libraries written through the records domain's own writes.

use crate::album_detail::WorkChild;
use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::{delete_release, test_records};
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const COMPOSER: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c01";
const WORK: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c11";
const FIRST_PART: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c12";
const SECOND_PART: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c13";
const ALPHA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c21";
const ALPHA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c22";
const BETA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c31";
const BETA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c32";
const GAMMA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c41";
const GAMMA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c42";

/// What a page of the work's children names: each part's work id, or each
/// release's id.
async fn work_children(views: &LibraryViews, offset: u64, limit: u64) -> Vec<String> {
    views
        .work_child_page(WORK, offset, limit)
        .await
        .unwrap()
        .into_iter()
        .map(|child| match child {
            WorkChild::Part(work) => work.work.id,
            WorkChild::Release(release) => release.release_id,
        })
        .collect()
}

/// A work's children are its parts, in their place among its parts whatever
/// their titles, then each release that performs it once however many of its
/// tracks do. bae sorts the releases by their album's title. The test reads
/// each page at its offset, and the count of the children is a live query
/// that counts a release that comes to perform the work.
#[tokio::test]
async fn a_works_children_are_its_parts_then_its_releases_paged_and_counted_in_sql() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(COMPOSER, "Composer Name")
        .work(WORK, "Whole Work")
        .composer(WORK, COMPOSER)
        .work(FIRST_PART, "Zeta Part")
        .part(WORK, FIRST_PART)
        .work(SECOND_PART, "Alpha Part")
        .part(WORK, SECOND_PART)
        .album(BETA_ALBUM, "Beta Album", Some(2026), &[COMPOSER])
        .release(BETA_ALBUM, BETA_RELEASE)
        .tracks(BETA_RELEASE, &[("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c33", "Beta Track")])
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c33", WORK)
        .album(ALPHA_ALBUM, "Alpha Album", Some(2026), &[COMPOSER])
        .release(ALPHA_ALBUM, ALPHA_RELEASE)
        .tracks(
            ALPHA_RELEASE,
            &[
                ("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c23", "Alpha Track One"),
                ("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c24", "Alpha Track Two"),
            ],
        )
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c23", WORK)
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c24", WORK)
        .write(&test_records(&db))
        .await;
    let views = LibraryViews::new(db.clone());

    assert_eq!(work_children(&views, 0, 3).await, [FIRST_PART, SECOND_PART, ALPHA_RELEASE]);
    assert_eq!(work_children(&views, 3, 3).await, [BETA_RELEASE]);
    assert!(work_children(&views, 4, 3).await.is_empty());

    let mut count = views.subscribe_work_child_count(WORK);
    assert_eq!(count.next().await.expect("the query stays open").unwrap(), 4);
    RecordsFixture::new()
        .album(GAMMA_ALBUM, "Gamma Album", Some(2026), &[COMPOSER])
        .release(GAMMA_ALBUM, GAMMA_RELEASE)
        .tracks(GAMMA_RELEASE, &[("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c43", "Gamma Track")])
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c43", WORK)
        .write(&test_records(&db))
        .await;
    let counted = tokio::time::timeout(HANG_GUARD, count.next()).await.unwrap().unwrap().unwrap();
    assert_eq!(counted, 5);
}

const PAGED_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c51";
const MAIN_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c52";
const OTHER_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c53";

/// What a page of the album's tracks names: each track's id.
async fn album_tracks(views: &LibraryViews, offset: u64, limit: u64) -> Vec<String> {
    let page = views.album_track_page(PAGED_ALBUM, offset, limit).await.unwrap();
    page.into_iter().map(|track| track.id).collect()
}

/// An album's tracks are its main release's, in playing order, never another
/// release's. The test reads each page at its offset, and the count of the
/// tracks is a live query that follows the album to the release that becomes
/// its main one when the test deletes the first release.
#[tokio::test]
async fn an_albums_tracks_are_its_main_releases_paged_and_counted_in_sql() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let (first, second, third) = (
        "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c54",
        "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c55",
        "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c56",
    );
    RecordsFixture::new()
        .artist(COMPOSER, "Composer Name")
        .album(PAGED_ALBUM, "Paged Album", Some(2026), &[COMPOSER])
        .release(PAGED_ALBUM, MAIN_RELEASE)
        .tracks(MAIN_RELEASE, &[(first, "First"), (second, "Second"), (third, "Third")])
        .release(PAGED_ALBUM, OTHER_RELEASE)
        .tracks(OTHER_RELEASE, &[("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c57", "Other")])
        .write(&test_records(&db))
        .await;
    let views = LibraryViews::new(db.clone());

    assert_eq!(album_tracks(&views, 0, 2).await, [first, second]);
    assert_eq!(album_tracks(&views, 2, 2).await, [third]);
    assert!(album_tracks(&views, 4, 2).await.is_empty());

    let mut count = views.subscribe_album_track_count(PAGED_ALBUM);
    assert_eq!(count.next().await.expect("the query stays open").unwrap(), 3);
    delete_release(&test_records(&db), MAIN_RELEASE).await;
    let counted = tokio::time::timeout(HANG_GUARD, count.next()).await.unwrap().unwrap().unwrap();
    assert_eq!(counted, 1);
}
