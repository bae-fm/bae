//! A work's children as a media browser lists them: its parts, then the
//! releases that perform it, read as a window from a child's key on.

use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

const COMPOSER_A: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c01";
const PAGED_WORK: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c11";
const FIRST_PART: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c12";
const SECOND_PART: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c13";
const ALPHA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c21";
const ALPHA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c22";
const BETA_ALBUM: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c31";
const BETA_RELEASE: &str = "6d1f0b3a-2c4e-4f7a-9b1d-5e8c7a6b4c32";

/// Each child's id in one delivery of a window of a work's children: a
/// part's work id, or a release's id.
fn children_of(read: WindowRead<DbWorkChild>) -> Vec<String> {
    let crate::ListRows::WorkChildren(rows) = crate::composer::resolve_work_child_window(read).rows
    else {
        panic!("a window of a work's children holds children");
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
/// by their album's title. A window holds its limit of children from a
/// child's key on, and follows a renamed album that moves its release into
/// the window. The count is a live query of its own.
#[tokio::test]
async fn a_works_children_are_its_parts_then_its_releases_in_windows_from_a_key() {
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

    let mut live = views.subscribe_work_child_window(PAGED_WORK, ListWindow::top(3));
    let read = live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(children_of(read), [FIRST_PART, SECOND_PART, ALPHA_RELEASE]);
    let fourth = views.work_child_key_at(PAGED_WORK, 3).await.unwrap().expect("a fourth child");
    let mut last_live =
        views.subscribe_work_child_window(PAGED_WORK, ListWindow { from: Some(fourth), limit: 3 });
    let read = last_live.next_inline().await.expect("the query stays open").result.unwrap();
    assert_eq!(children_of(read), [BETA_RELEASE]);
    let mut count = views.subscribe_work_child_count(PAGED_WORK);
    assert_eq!(count.next_inline().await.expect("the query stays open").unwrap(), 4);
    let third = views.work_child_key_at(PAGED_WORK, 2).await.unwrap().expect("a third child");
    let mut position = views.subscribe_work_child_position(PAGED_WORK, third);
    assert_eq!(position.next_inline().await.expect("the query stays open").result.unwrap(), 2);

    test_records(&db).rename_album_for_test(BETA_ALBUM, "Aardvark Album").await.unwrap();
    let read = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap().result;
    assert_eq!(children_of(read.unwrap()), [FIRST_PART, SECOND_PART, BETA_RELEASE]);
}
