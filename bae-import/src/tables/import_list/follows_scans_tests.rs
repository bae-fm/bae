//! What the scan stores places: its releases, their folders' dates and the
//! folders the person skipped.

use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;

#[tokio::test]
async fn a_scanned_release_is_placed_on_found_as_nobody_has_looked_it_up() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(lib.scan("Album")).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A release found before the folder around it was known is no row until a
/// later scan settles what encloses it.
#[tokio::test]
async fn a_tentative_release_is_not_placed_until_it_settles() {
    let lib = Library::new().await;
    assert_eq!(lib.moved(lib.scan_as("Album", "tentative")).await, moves([]));
    assert_eq!(
        lib.moved(lib.scan("Album")).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

#[tokio::test]
async fn a_removed_release_leaves_the_list() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(lib.moved(lib.unscan("Album")).await, moves([(lib.release("Album"), None)]));
}

/// A rescan stamps every release it walks with its generation, which moves
/// nothing the list shows.
#[tokio::test]
async fn restamping_a_release_moves_nothing() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(lib.moved(lib.rescan()).await, moves([]));
}

/// A folder that failed validation is on Skipped whatever else is stored for
/// it, showing what its tags call it, and its folder's name without tags.
#[tokio::test]
async fn a_folder_that_failed_validation_shows_what_its_tags_call_it() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(lib.scan_as("Broken", "invalid")).await,
        moves([(lib.invalid("Broken"), Some(failed_validation("Broken")))])
    );
    assert_eq!(
        lib.moved(lib.scan_invalid("Broken", Some(("Tagged Album", None)))).await,
        moves([(
            lib.invalid("Broken"),
            Some(failed_validation("Broken").shown("Tagged Album", None))
        )])
    );
    assert_eq!(
        lib.moved(lib.scan_invalid("Broken", Some(("Tagged Album", Some("Tagged Artist"))))).await,
        moves([(
            lib.invalid("Broken"),
            Some(failed_validation("Broken").shown("Tagged Album", Some("Tagged Artist")))
        )])
    );
    assert_eq!(
        lib.moved(lib.scan_as("Broken", "invalid")).await,
        moves([(lib.invalid("Broken"), Some(failed_validation("Broken")))])
    );
}

/// A release is dated by its folder: the date the filesystem gives it, else
/// when the scans first saw it.
#[tokio::test]
async fn a_release_is_dated_by_its_folder() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(
        lib.moved(lib.scan_dated("Album", 100)).await,
        moves([(lib.release("Album"), Some(found("Album").dated(100)))])
    );
    assert_eq!(
        lib.moved(lib.scan_dated("Album", 50)).await,
        moves([(lib.release("Album"), Some(found("Album").dated(50)))])
    );
}

/// Skipping a folder moves its release to Skipped, out of Found's states;
/// restoring it brings it back.
#[tokio::test]
async fn a_skipped_folder_is_listed_on_skipped() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(
        lib.moved(lib.skip("Album")).await,
        moves([(lib.release("Album"), Some(found("Album").on("skipped")))])
    );
    assert_eq!(
        lib.moved(lib.set_skipped("Album", false)).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// No longer watching a folder takes every row under it out, its group
/// headers included.
#[tokio::test]
async fn unwatching_a_folder_takes_its_rows_out() {
    let lib = Library::new().await;
    lib.run(lib.scan("Artist/Album") + lib.scan("Single")).await;
    assert_eq!(
        lib.moved(lib.unwatch()).await,
        moves([
            (lib.release("Artist/Album"), None),
            (lib.group("Artist"), None),
            (lib.release("Single"), None),
        ])
    );
}
