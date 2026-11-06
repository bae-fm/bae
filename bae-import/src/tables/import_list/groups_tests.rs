//! Found's groups: which releases a folder's header gathers, and the date the
//! header and its members sort by.

use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;

/// A release nested below a folder is listed under that folder's header, and
/// the group sorts by its newest member. A member sorts in its header's
/// place, so one that moves the group's date moves only the header: a scan
/// writing a large group's members newest last writes two rows each.
#[tokio::test]
async fn nested_releases_group_under_their_first_folder_by_the_newest() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(lib.scan_dated("Artist/Early", 100)).await,
        moves([
            (lib.release("Artist/Early"), Some(found("Artist/Early").dated(100).grouped("Artist"))),
            (lib.group("Artist"), Some(header("Artist", Some(100)))),
        ])
    );
    assert_eq!(
        lib.moved(lib.scan_dated("Artist/Late", 300)).await,
        moves([
            (lib.release("Artist/Late"), Some(found("Artist/Late").dated(300).grouped("Artist"))),
            (lib.group("Artist"), Some(header("Artist", Some(300)))),
        ])
    );
    assert_eq!(
        lib.moved(lib.unscan("Artist/Late")).await,
        moves([
            (lib.release("Artist/Late"), None),
            (lib.group("Artist"), Some(header("Artist", Some(100)))),
        ])
    );
}

/// The release at a folder joins that folder's group once another release
/// is nested below it — even one that failed validation, which Found never
/// lists — and leaves it when none is.
#[tokio::test]
async fn a_release_at_a_folder_groups_while_another_is_nested_below_it() {
    let lib = Library::new().await;
    lib.run(lib.scan("Box")).await;
    assert_eq!(
        lib.moved(lib.scan_as("Box/Disc 1", "invalid")).await,
        moves([
            (lib.release("Box"), Some(found("Box").grouped("Box"))),
            (lib.group("Box"), Some(header("Box", Some(seen())))),
            (lib.invalid("Box/Disc 1"), Some(failed_validation("Box/Disc 1"))),
        ])
    );
    assert_eq!(
        lib.moved(lib.unscan("Box/Disc 1")).await,
        moves([
            (lib.release("Box"), Some(found("Box"))),
            (lib.group("Box"), None),
            (lib.invalid("Box/Disc 1"), None),
        ])
    );
}

/// A folder whose name only starts with another's is not nested below it.
#[tokio::test]
async fn a_folder_sharing_a_name_prefix_is_not_nested() {
    let lib = Library::new().await;
    lib.run(lib.scan("Box")).await;
    assert_eq!(
        lib.moved(lib.scan("Box Set/Disc 1")).await,
        moves([
            (lib.release("Box Set/Disc 1"), Some(found("Box Set/Disc 1").grouped("Box Set"))),
            (lib.group("Box Set"), Some(header("Box Set", Some(seen())))),
        ])
    );
}

/// Only Found groups: a member that leaves Found leaves its group, and the
/// group's date is its remaining members'.
#[tokio::test]
async fn a_member_leaving_found_leaves_its_group() {
    let lib = Library::new().await;
    lib.run(lib.scan_dated("Artist/Early", 100) + lib.scan_dated("Artist/Late", 300)).await;
    assert_eq!(
        lib.moved(lib.skip("Artist/Late")).await,
        moves([
            (lib.release("Artist/Late"), Some(found("Artist/Late").dated(300).on("skipped"))),
            (lib.group("Artist"), Some(header("Artist", Some(100)))),
        ])
    );
    assert_eq!(
        lib.moved(lib.skip("Artist/Early")).await,
        moves([
            (lib.release("Artist/Early"), Some(found("Artist/Early").dated(100).on("skipped"))),
            (lib.group("Artist"), None),
        ])
    );
}

/// A folder moved on disk is another release, in its new folder's group; the
/// group it left keeps its other member.
#[tokio::test]
async fn a_moved_release_changes_group() {
    let lib = Library::new().await;
    lib.run(lib.scan("Old/Album") + lib.scan("Old/Other")).await;
    assert_eq!(
        lib.moved(lib.unscan("Old/Album") + lib.scan("New/Album")).await,
        moves([
            (lib.release("Old/Album"), None),
            (lib.release("New/Album"), Some(found("New/Album").grouped("New"))),
            (lib.group("New"), Some(header("New", Some(seen())))),
        ])
    );
}

/// Placing a release under a group costs the same however many releases the
/// group already holds, each newer than the last as a scan writing a
/// copied-in collection finds them: only the header holds the group's date,
/// and the newest member is read off the group's index. Counted in SQLite's
/// own steps for the write of the release's scan row, its triggers' included.
///
/// The group, and when each folder was found, come from the scans' own writes;
/// the measured scan row alone is written apart from its pass, by the folder
/// scans' statement for it.
#[tokio::test]
async fn a_member_costs_the_same_however_large_its_group() {
    let lib = Library::new().await;
    let display_path = |n: usize| format!("Collection/Album {n}");
    let dated = |members: std::ops::Range<usize>| {
        members.map(move |n| (display_path(n), Some(i64::try_from(n).unwrap())))
    };
    // A first pass finds every folder, each newer than the last; a second
    // reads the first 19 alone, so the others stay found but leave the scan.
    lib.run(
        lib.scan_batch(dated(0..2_000))
            + lib.finish_pass()
            + lib.begin_pass()
            + lib.scan_batch(dated(0..19))
            + lib.finish_pass()
            + lib.begin_pass(),
    )
    .await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let place = async |n: usize| -> usize {
        tables
            .scan_row_write_steps_for_test(lib.generation().await, &lib.candidate(&display_path(n)))
            .await
    };
    let early = place(19).await;
    lib.run(lib.scan_batch(dated(19..1_999)) + lib.finish_pass() + lib.begin_pass()).await;
    let late = place(1_999).await;
    assert!(late < early * 3 / 2, "placing the 20th member took {early} steps, the 2,000th {late}");
}

/// A folder whose name isn't valid UTF-8 nests and groups by its bytes: the
/// release below it joins its group, and the release at it joins too, both
/// shown under the name's escape.
#[cfg(unix)]
#[tokio::test]
async fn a_folder_whose_name_is_not_utf8_nests_and_groups_by_its_bytes() {
    use std::os::unix::ffi::OsStrExt;
    let lib = Library::new().await;
    let cafe = std::path::Path::new(std::ffi::OsStr::from_bytes(b"Caf\xE9"));
    let disc = cafe.join("Disc 1");
    assert_eq!(
        lib.moved(lib.scan_folder(cafe, "valid")).await,
        moves([(lib.release_at(cafe), Some(found("Caf\\xE9")))])
    );
    assert_eq!(
        lib.moved(lib.scan_folder(&disc, "valid")).await,
        moves([
            (lib.release_at(cafe), Some(found("Caf\\xE9").grouped("Caf\\xE9"))),
            (lib.release_at(&disc), Some(found("Caf\\xE9/Disc 1").grouped("Caf\\xE9"))),
            (lib.group("Caf\\xE9"), Some(header("Caf\\xE9", Some(seen())))),
        ])
    );
}
