//! What a chosen folder is to the library, read off the same placement the
//! list uses.

use super::*;
use std::path::PathBuf;

fn chosen(display_path: &str) -> PathBuf {
    if display_path.is_empty() {
        return PathBuf::from(root());
    }
    PathBuf::from(key(display_path))
}

fn read(rows: &ImportQueueRows, display_path: &str) -> ChosenFolder {
    chosen_folder(rows, &root(), &chosen(display_path)).expect("the chosen folder reads")
}

fn skipped(rows: &mut ImportQueueRows, display_path: &str) {
    rows.skipped.insert((root(), display_path.to_string()));
}

#[test]
fn an_album_already_in_the_library_is_shown_there() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Artist/Album")];
    imported(&mut rows, "Artist/Album", "release-1", 1);

    assert_eq!(
        read(&rows, "Artist/Album"),
        ChosenFolder::InLibrary {
            album_id: "album-release-1".to_string()
        }
    );
}

#[test]
fn an_album_not_yet_imported_is_taken_to_the_queue() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Artist/Album"), candidate("Artist/Album 2")];

    assert_eq!(
        read(&rows, "Artist/Album"),
        ChosenFolder::InImportQueue {
            candidate_keys: vec![key("Artist/Album")]
        },
        "only the chosen folder's release, not its sibling"
    );
}

/// A folder of several albums, some imported: the person is taken to what is
/// left, in path order — never to the library, which holds only part of it.
#[test]
fn a_partly_imported_folder_is_taken_to_what_is_left() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Artist/Album 10"),
        candidate("Artist/Album 2"),
        candidate("Artist/Album 1"),
        candidate("Other Artist/Album"),
    ];
    imported(&mut rows, "Artist/Album 1", "release-1", 1);

    assert_eq!(
        read(&rows, "Artist"),
        ChosenFolder::InImportQueue {
            candidate_keys: vec![key("Artist/Album 2"), key("Artist/Album 10")]
        }
    );
}

#[test]
fn a_fully_imported_folder_shows_its_first_album() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Artist/Album 2"), candidate("Artist/Album 1")];
    imported(&mut rows, "Artist/Album 1", "release-1", 1);
    imported(&mut rows, "Artist/Album 2", "release-2", 2);

    assert_eq!(
        read(&rows, "Artist"),
        ChosenFolder::InLibrary {
            album_id: "album-release-1".to_string()
        }
    );
}

/// Skipped releases are not in the library, so a folder of them is not shown
/// there — but a release still waiting comes first.
#[test]
fn skipped_releases_are_shown_only_when_nothing_else_waits() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Artist/Album 1"), candidate("Artist/Album 2")];
    skipped(&mut rows, "Artist/Album 1");

    assert_eq!(
        read(&rows, "Artist"),
        ChosenFolder::InImportQueue {
            candidate_keys: vec![key("Artist/Album 2")]
        }
    );

    skipped(&mut rows, "Artist/Album 2");
    assert_eq!(
        read(&rows, "Artist"),
        ChosenFolder::InImportQueue {
            candidate_keys: vec![key("Artist/Album 1"), key("Artist/Album 2")]
        }
    );
}

/// A disc folder of a release read from its parent holds no release of its
/// own: it names the release it is part of.
#[test]
fn a_folder_inside_a_release_names_that_release() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Artist"), candidate("Artist/Album")];
    imported(&mut rows, "Artist/Album", "release-1", 1);

    assert_eq!(
        read(&rows, "Artist/Album/CD1"),
        ChosenFolder::InLibrary {
            album_id: "album-release-1".to_string()
        },
        "the nearest enclosing release, not the one further up"
    );
}

#[test]
fn a_folder_with_no_release_says_so() {
    let mut rows = queue();
    rows.candidates = vec![
        invalid("Artist/Album"),
        tentative("Artist/Album 2"),
        candidate("Other Artist/Album"),
    ];

    assert_eq!(read(&rows, "Artist"), ChosenFolder::NoReleases);
}

/// Releases under another watched root are not the chosen folder's, even when
/// a path compares as below it.
#[test]
fn releases_under_another_root_are_not_read() {
    let mut rows = queue();
    let mut elsewhere = candidate("Artist/Album");
    elsewhere.watched_folder_path = host_root("/other");
    rows.candidates = vec![elsewhere];

    assert_eq!(read(&rows, "Artist"), ChosenFolder::NoReleases);
}

#[test]
fn the_whole_root_reads_every_release_under_it() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Artist/Album"), candidate("Other Artist/Album")];
    imported(&mut rows, "Other Artist/Album", "release-1", 1);

    assert_eq!(
        read(&rows, ""),
        ChosenFolder::InImportQueue {
            candidate_keys: vec![key("Artist/Album")]
        }
    );
}
