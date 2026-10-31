//! What a release's import state places: its draft, its link, its failure,
//! what this device imported from it, and what is running for it.

use super::super::{ALBUM_A, ARTIST_A, ARTIST_B, RELEASE_A};
use super::*;

/// The draft counts only while its state describes the folder's current
/// files: it shows once it is not blank, and an older revision's shows nothing.
#[tokio::test]
async fn the_draft_shows_while_its_state_is_current() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    assert_eq!(lib.moved(&lib.state("Album")).await, moves([]));
    assert_eq!(
        lib.moved(&lib.draft("Album", "Album Title")).await,
        moves([(
            lib.release("Album"),
            Some(found("Album").shown("Album Title", Some("")))
        )])
    );
    let draft = |set: &str| format!("{set} WHERE content_hash = '{}';", hash("Album"));
    assert_eq!(
        lib.moved(&draft("UPDATE import_candidate_edit SET draft_valid = 1"))
            .await,
        moves([])
    );
    assert_eq!(
        lib.moved(&draft(
            "UPDATE import_candidate_state SET edit_revision = 1, metadata_revision = 4"
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE scan_candidate SET file_edit_revision = 1 WHERE path = {};",
            lib.sql("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").shown("Album Title", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(&draft("DELETE FROM import_candidate_state"))
            .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A blank draft shows once a source or a link answered it.
#[tokio::test]
async fn a_blank_draft_shows_once_something_answered_it() {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album")))
        .await;
    let provenance = format!(
        "INSERT INTO import_candidate_draft_provenance (content_hash, kind)
         VALUES ('{}', 'file_tags');",
        hash("Album")
    );
    assert_eq!(
        lib.moved(&provenance).await,
        moves([(
            lib.release("Album"),
            Some(found("Album").shown("", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM import_candidate_draft_provenance WHERE content_hash = '{}';",
            hash("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// The draft's album credits print as one line: each credit's printed name,
/// after the join the credit before it states, or a comma.
#[tokio::test]
async fn the_album_credits_print_as_one_line() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}
         INSERT INTO artists (id, name, name_key, created_at)
         VALUES ('{ARTIST_A}', 'Picked Artist', 'picked artist', '2026-01-01T00:00:00Z');",
        lib.scan("Album"),
        lib.state("Album"),
        lib.draft("Album", "Album Title")
    ))
    .await;
    let shown = |artists: &str| {
        moves([(
            lib.release("Album"),
            Some(found("Album").shown("Album Title", Some(artists))),
        )])
    };
    let hash = hash("Album");
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_album_artist_assignment
                 (content_hash, position, assignment_kind, name, credited_as, join_phrase)
             VALUES ('{hash}', 0, 'credit', 'First Artist', NULL, ' & '),
                    ('{hash}', 2, 'credit', 'Second Artist', 'Printed Second', NULL);"
        ))
        .await,
        shown("First Artist & Printed Second")
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_album_artist_assignment
                 (content_hash, position, assignment_kind, artist_id, credit_key)
             VALUES ('{hash}', 3, 'picked', '{ARTIST_A}', 'other name');"
        ))
        .await,
        shown("First Artist & Printed Second, Picked Artist")
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE import_candidate_album_artist_assignment SET join_phrase = ' feat. '
             WHERE content_hash = '{hash}' AND position = 2;"
        ))
        .await,
        shown("First Artist & Printed Second feat. Picked Artist")
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE artists SET name = 'Renamed Artist' WHERE id = '{ARTIST_A}';"
        ))
        .await,
        shown("First Artist & Printed Second feat. Renamed Artist")
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO artist_credits (artist_id, name_key, name, created_at)
             VALUES ('{ARTIST_A}', 'other name', 'Other Name', '2026-01-01T00:00:00Z');"
        ))
        .await,
        shown("First Artist & Printed Second feat. Other Name")
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE artist_credits SET name = 'Other Printing' WHERE artist_id = '{ARTIST_A}';"
        ))
        .await,
        shown("First Artist & Printed Second feat. Other Printing")
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM artist_credits WHERE artist_id = '{ARTIST_A}';"
        ))
        .await,
        shown("First Artist & Printed Second feat. Renamed Artist")
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM import_candidate_album_artist_assignment
             WHERE content_hash = '{hash}' AND position = 0;"
        ))
        .await,
        shown("Printed Second feat. Renamed Artist")
    );
    // An artist no draft credits renames nothing.
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO artists (id, name, name_key, created_at)
             VALUES ('{ARTIST_B}', 'Other Artist', 'other artist', '2026-01-01T00:00:00Z');
             UPDATE artists SET name = 'Still Other' WHERE id = '{ARTIST_B}';"
        ))
        .await,
        moves([])
    );
}

/// A link to the catalogs makes the release identified and shows its draft.
#[tokio::test]
async fn a_linked_release_is_identified() {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album")))
        .await;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_release_link (content_hash, kind)
             VALUES ('{}', 'album');",
            hash("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").state("identified").shown("", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM import_candidate_release_link WHERE content_hash = '{}';",
            hash("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A failed import keeps its release on Found, saying so, until the failure
/// is cleared; it counts whatever revision the state is at.
#[tokio::test]
async fn a_failed_import_is_founds_import_error() {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album")))
        .await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE import_candidate_state SET edit_revision = 3 WHERE content_hash = '{0}';
             INSERT INTO import_candidate_failure (content_hash, error, failed_at)
             VALUES ('{0}', 'it broke', '2026-01-01T00:00:00Z');",
            hash("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").state("import_error"))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM import_candidate_failure WHERE content_hash = '{}';",
            hash("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// What this device imported from a folder's files puts it on Done, dated by
/// when its release was written, whatever else is stored for it.
#[tokio::test]
async fn an_imported_release_is_done_dated_by_its_release() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}
         INSERT INTO skipped_import_candidates (candidate_path) VALUES ({});
         INSERT INTO albums (id, title, is_compilation, created_at)
         VALUES ('{ALBUM_A}', 'Album Title', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order)
         VALUES ('{RELEASE_A}', '{ALBUM_A}', '2026-01-01T00:00:00.123456789+00:00', 0);",
        lib.scan("Album"),
        lib.discovered("Album", 100),
        lib.state("Album"),
        lib.sql("Album")
    ))
    .await;
    let imported_at = 1_767_225_600_000;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO local_release_origin (release_id, content_hash)
             VALUES ('{RELEASE_A}', '{}');",
            hash("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("done").dated(imported_at + 123))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE releases SET created_at = '2026-01-01T02:00:01+02:00' WHERE id = '{RELEASE_A}';"
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("done").dated(imported_at + 1_000))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE releases SET created_at = '2026-01-01T00:00:02Z' WHERE id = '{RELEASE_A}';"
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("done").dated(imported_at + 2_000))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM local_release_origin WHERE release_id = '{RELEASE_A}';"
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("skipped").dated(100))
        )])
    );
}

/// What is running for a Found release decides its state over the tables: an
/// import, then a run, then a run's answer bae could not store.
#[tokio::test]
async fn what_is_running_decides_a_found_releases_state() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Album"),
        lib.state("Album"),
        lib.verdict("Album", "not_found", None)
    ))
    .await;
    let key = lib.sql("Album");
    let standing = |set: &str| format!("{set} WHERE candidate_key = {key};");
    let now = |state: &str| moves([(lib.release("Album"), Some(found("Album").state(state)))]);
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_standing (candidate_key, import, import_id)
             VALUES ({key}, 'queued', 'import-1');"
        ))
        .await,
        now("importing")
    );
    assert_eq!(
        lib.moved(&standing(&format!(
            "UPDATE import_candidate_standing SET import = 'running', identification = 'queued', \
             content_hash = '{}', file_edit_revision = 0, admission = 'requested'",
            hash("Album")
        )))
        .await,
        moves([])
    );
    assert_eq!(
        lib.moved(&standing(
            "UPDATE import_candidate_standing SET import = NULL, import_id = NULL"
        ))
        .await,
        now("identifying")
    );
    assert_eq!(
        lib.moved(&standing(
            "UPDATE import_candidate_standing SET identification = 'reading_pick', \
             admission = NULL, identify_run = 1, import_asked = 0"
        ))
        .await,
        now("identified")
    );
    assert_eq!(
        lib.moved(&standing(
            "UPDATE import_candidate_standing SET identification = 'not_written', \
             identify_run = NULL, import_asked = NULL, save_failure_error = 'disk full'"
        ))
        .await,
        now("error")
    );
    assert_eq!(
        lib.moved(&standing("DELETE FROM import_candidate_standing"))
            .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").needs_you("not_found"))
        )])
    );
    // Off Found nothing running is a state.
    lib.run(&format!(
        "INSERT INTO skipped_import_candidates (candidate_path) VALUES ({key});"
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_standing (candidate_key, import, import_id)
             VALUES ({key}, 'queued', 'import-1');"
        ))
        .await,
        moves([])
    );
}

/// With its draft gone a release shows its folder's name again.
#[tokio::test]
async fn a_release_whose_draft_is_gone_shows_its_folder() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Album"),
        lib.state("Album"),
        lib.draft("Album", "Album Title")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM import_candidate_edit WHERE content_hash = '{}';",
            hash("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A row keyed by a release's files, moved to another release's files, moves
/// both releases.
#[tokio::test]
async fn a_row_moved_to_other_files_moves_both_releases() {
    let imported = |placed: Placed| placed.on("done").dated(1_767_225_600_000);
    for (table, row, placed) in [
        (
            "import_candidate_failure",
            "INSERT INTO import_candidate_failure (content_hash, error, failed_at)
             VALUES ('{hash}', 'it broke', '2026-01-01T00:00:00Z');",
            (|placed: Placed| placed.state("import_error")) as fn(Placed) -> Placed,
        ),
        (
            "import_candidate_release_link",
            "INSERT INTO import_candidate_release_link (content_hash, kind)
             VALUES ('{hash}', 'album');",
            |placed: Placed| {
                let title = placed.display_path.clone().unwrap();
                placed.state("identified").shown(&title, Some(""))
            },
        ),
        (
            "import_candidate_draft_provenance",
            "INSERT INTO import_candidate_draft_provenance (content_hash, kind)
             VALUES ('{hash}', 'file_tags');",
            |placed: Placed| {
                let title = placed.display_path.clone().unwrap();
                placed.shown(&title, Some(""))
            },
        ),
        (
            "import_candidate_verdict",
            "INSERT INTO import_candidate_verdict
                 (content_hash, kind, kept_own_draft, identified_at)
             VALUES ('{hash}', 'not_found', 0, '2026-01-01T00:00:00Z');",
            |placed: Placed| placed.needs_you("not_found"),
        ),
        (
            "local_release_origin",
            "INSERT INTO local_release_origin (release_id, content_hash)
             VALUES ('{RELEASE_A}', '{hash}');",
            imported,
        ),
    ] {
        let lib = Library::new().await;
        let mut setup = String::new();
        for name in ["First", "Second"] {
            setup.push_str(&lib.scan(name));
            setup.push_str(&lib.state(name));
            setup.push_str(&format!(
                "UPDATE import_candidate_edit SET album_title = '{name}'
                 WHERE content_hash = '{}';",
                hash(name)
            ));
        }
        setup.push_str(&format!(
            "INSERT INTO albums (id, title, is_compilation, created_at)
             VALUES ('{ALBUM_A}', 'Album Title', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order)
             VALUES ('{RELEASE_A}', '{ALBUM_A}', '2026-01-01T00:00:00Z', 0);"
        ));
        setup.push_str(
            &row.replace("{hash}", &hash("First"))
                .replace("{RELEASE_A}", RELEASE_A),
        );
        lib.run(&setup).await;
        assert_eq!(
            lib.moved(&format!(
                "UPDATE {table} SET content_hash = '{}' WHERE content_hash = '{}';",
                hash("Second"),
                hash("First")
            ))
            .await,
            moves([
                (lib.release("First"), Some(found("First"))),
                (lib.release("Second"), Some(placed(found("Second")))),
            ]),
            "{table}"
        );
    }
}

/// A draft stored already written shows as soon as it is stored.
#[tokio::test]
async fn a_draft_stored_written_shows_at_once() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}INSERT INTO import_candidate_state (content_hash, candidate_key) VALUES ('{}', {});",
        lib.scan("Album"),
        hash("Album"),
        lib.sql("Album")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_edit
                 (content_hash, album_title, album_year, year, author, draft_blank, draft_valid)
             VALUES ('{}', 'Tagged Title', '', '', 'person', 0, 1);",
            hash("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").shown("Tagged Title", Some("")))
        )])
    );
}
