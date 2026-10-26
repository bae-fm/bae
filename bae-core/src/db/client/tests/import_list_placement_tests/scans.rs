//! What the scan stores places: its releases, their folders' dates, the
//! watched folders' order and the folders the person skipped.

use super::*;

#[tokio::test]
async fn a_scanned_release_is_placed_on_found_as_nobody_has_looked_it_up() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(&lib.scan("Album")).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A release found before the folder around it was known is no row until a
/// later scan settles what encloses it.
#[tokio::test]
async fn a_tentative_release_is_not_placed_until_it_settles() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(&lib.scan_as("Album", "tentative")).await,
        moves([])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE scan_candidate SET kind = 'valid' WHERE path = '{}';",
            lib.path("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

#[tokio::test]
async fn a_removed_release_leaves_the_list() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM scan_candidate WHERE path = '{}';",
            lib.path("Album")
        ))
        .await,
        moves([(lib.release("Album"), None)])
    );
}

/// A rescan stamps every release it walks with its generation, which moves
/// nothing the list shows.
#[tokio::test]
async fn restamping_a_release_moves_nothing() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE scan_candidate SET generation = 2, name = name WHERE path = '{}';",
            lib.path("Album")
        ))
        .await,
        moves([])
    );
}

/// With no draft a release shows its folder's name, which the filter matches.
#[tokio::test]
async fn a_release_with_no_draft_shows_its_folders_new_name() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE scan_candidate SET name = 'Renamed' WHERE path = '{}';",
            lib.path("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").shown("Renamed", None))
        )])
    );
}

/// A folder that failed validation is on Skipped whatever else is stored for
/// it, showing what its tags call it, and its folder's name without tags.
#[tokio::test]
async fn a_folder_that_failed_validation_shows_what_its_tags_call_it() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(&lib.scan_as("Broken", "invalid")).await,
        moves([(lib.invalid("Broken"), Some(failed_validation("Broken")))])
    );
    let tags = |set: &str| {
        format!(
            "{set} WHERE watched_folder_path = '{}' AND candidate_path = '{}';",
            lib.root,
            lib.path("Broken")
        )
    };
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO scan_skipped_tags (watched_folder_path, candidate_path, album_title, artist)
             VALUES ('{}', '{}', 'Tagged Album', NULL);",
            lib.root,
            lib.path("Broken")
        ))
        .await,
        moves([(
            lib.invalid("Broken"),
            Some(failed_validation("Broken").shown("Tagged Album", None))
        )])
    );
    assert_eq!(
        lib.moved(&tags(
            "UPDATE scan_skipped_tags SET artist = 'Tagged Artist'"
        ))
        .await,
        moves([(
            lib.invalid("Broken"),
            Some(failed_validation("Broken").shown("Tagged Album", Some("Tagged Artist")))
        )])
    );
    assert_eq!(
        lib.moved(&tags("DELETE FROM scan_skipped_tags")).await,
        moves([(lib.invalid("Broken"), Some(failed_validation("Broken")))])
    );
}

/// A release is dated by its folder: the date the filesystem gives it, else
/// when the scans first saw it. Whether a scan has read it moves nothing.
#[tokio::test]
async fn a_release_is_dated_by_its_folder() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    assert_eq!(
        lib.moved(&lib.discovered("Album", 100)).await,
        moves([(lib.release("Album"), Some(found("Album").dated(100)))])
    );
    let discovery = |set: &str| format!("{set} WHERE folder = '{}';", lib.path("Album"));
    assert_eq!(
        lib.moved(&discovery(
            "UPDATE folder_discovery SET source_date = 50, source_date_kind = 'created'"
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album").dated(50)))])
    );
    assert_eq!(
        lib.moved(&discovery("UPDATE folder_discovery SET settled = 0"))
            .await,
        moves([])
    );
    assert_eq!(
        lib.moved(&discovery("DELETE FROM folder_discovery")).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// The watched folders' order is the list's order after the date, so moving
/// one moves every row under it, its group headers included.
#[tokio::test]
async fn reordering_the_watched_folders_moves_every_row_under_them() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Artist/Album"),
        lib.scan_as("Broken", "invalid")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE watched_import_folders SET position = 3 WHERE path = '{}';",
            lib.root
        ))
        .await,
        moves([
            (
                lib.release("Artist/Album"),
                Some(found("Artist/Album").grouped("Artist", None).at_root(3))
            ),
            (lib.group("Artist"), Some(header("Artist", None).at_root(3))),
            (
                lib.invalid("Broken"),
                Some(failed_validation("Broken").at_root(3))
            ),
        ])
    );
}

/// Skipping a folder moves its release to Skipped, out of Found's states;
/// restoring it brings it back.
#[tokio::test]
async fn a_skipped_folder_is_listed_on_skipped() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    let skip = format!(
        "INSERT INTO skipped_import_candidates (candidate_path) VALUES ('{}');",
        lib.path("Album")
    );
    assert_eq!(
        lib.moved(&skip).await,
        moves([(lib.release("Album"), Some(found("Album").on("skipped")))])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM skipped_import_candidates WHERE candidate_path = '{}';",
            lib.path("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// No longer watching a folder takes every row under it out, its group
/// headers included.
#[tokio::test]
async fn unwatching_a_folder_takes_its_rows_out() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Artist/Album"),
        lib.scan("Single")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM watched_import_folders WHERE path = '{}';",
            lib.root
        ))
        .await,
        moves([
            (lib.release("Artist/Album"), None),
            (lib.group("Artist"), None),
            (lib.release("Single"), None),
        ])
    );
}

/// A skip stored under another folder skips that folder's release instead.
#[tokio::test]
async fn a_skip_moved_to_another_folder_moves_both_releases() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}INSERT INTO skipped_import_candidates (candidate_path) VALUES ('{}');",
        lib.scan("First"),
        lib.scan("Second"),
        lib.path("First")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE skipped_import_candidates SET candidate_path = '{}';",
            lib.path("Second")
        ))
        .await,
        moves([
            (lib.release("First"), Some(found("First"))),
            (lib.release("Second"), Some(found("Second").on("skipped"))),
        ])
    );
}
