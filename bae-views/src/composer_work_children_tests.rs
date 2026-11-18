//! A work's children as a media browser lists them: its parts, then the
//! releases that perform it, read a page at a time from a child's key on.

use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;

const COMPOSER_A: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c01";
const PAGED_WORK: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c11";
const FIRST_PART: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c12";
const SECOND_PART: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c13";
const ALPHA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c21";
const ALPHA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c22";
const BETA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c31";
const BETA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c32";

/// Each child's id in one delivery of a page of a work's children: a part's
/// work id, or a release's id.
fn children_of(rows: crate::ListRows) -> Vec<String> {
    let crate::ListRows::WorkChildren(rows) = rows else {
        panic!("a page of a work's children holds children");
    };
    rows.into_iter()
        .map(|child| match child {
            crate::album_detail::WorkChild::Part(work) => work.work.id,
            crate::album_detail::WorkChild::Release(release) => release.release_id,
        })
        .collect()
}

/// A work's children are its parts in their place among its parts whatever
/// their titles, then each release that performs it, once however many of
/// its tracks do, in the order its page lists them: bae sorts the releases
/// by their album's title. A page holds its limit of children from the child
/// at its first place, and follows a renamed album that moves its release
/// into the page. The count is a live query of its own.
#[tokio::test]
async fn a_works_children_are_its_parts_then_its_releases_in_pages() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    RecordsFixture::new()
        .artist(COMPOSER_A, "Composer Name")
        .work(PAGED_WORK, "Whole Work")
        .composer(PAGED_WORK, COMPOSER_A)
        .work(FIRST_PART, "Zeta Part")
        .part(PAGED_WORK, FIRST_PART)
        .work(SECOND_PART, "Alpha Part")
        .part(PAGED_WORK, SECOND_PART)
        .album(BETA_ALBUM, "Beta Album", Some(2026), &[COMPOSER_A])
        .release(BETA_ALBUM, BETA_RELEASE)
        .tracks(BETA_RELEASE, &[("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c33", "Beta Track")])
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c33", PAGED_WORK)
        .album(ALPHA_ALBUM, "Alpha Album", Some(2026), &[COMPOSER_A])
        .release(ALPHA_ALBUM, ALPHA_RELEASE)
        .tracks(
            ALPHA_RELEASE,
            &[
                ("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c23", "Alpha Track One"),
                ("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c24", "Alpha Track Two"),
            ],
        )
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c23", PAGED_WORK)
        .performs("6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c24", PAGED_WORK)
        .write(&test_records(&db))
        .await;
    let views = LibraryViews::new(db.clone());

    let page = |offset| {
        crate::test_pages::page_values(
            views.subscribe_work_child_page(PAGED_WORK, bae_store::ListPage { offset, limit: 3 }),
        )
    };
    let live = page(0);
    let rows = crate::test_pages::next_page(&live).await;
    assert_eq!(children_of(rows), [FIRST_PART, SECOND_PART, ALPHA_RELEASE]);
    let rows = crate::test_pages::next_page(&page(3)).await;
    assert_eq!(children_of(rows), [BETA_RELEASE]);
    let mut count = views.subscribe_work_child_count(PAGED_WORK);
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 4);

    test_records(&db).rename_album_for_test(BETA_ALBUM, "Aardvark Album").await.unwrap();
    let rows = crate::test_pages::next_page(&live).await;
    assert_eq!(children_of(rows), [FIRST_PART, SECOND_PART, BETA_RELEASE]);
}
