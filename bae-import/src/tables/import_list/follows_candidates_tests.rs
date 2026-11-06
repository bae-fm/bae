//! What a release's stored import state places: its draft, its link, its
//! failure, what is running for it, and the verdict its lookup stored and why
//! a found one picks none of its releases unattended.

use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;
use bae_records::test_fixtures::{ARTIST_A, ARTIST_B};

/// The draft counts only while its state describes the folder's current
/// files: it shows once it is not blank, and an older revision's shows nothing.
#[tokio::test]
async fn the_draft_shows_while_its_state_is_current() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Album")).await;
    assert_eq!(lib.moved(&lib.state("Album")).await, moves([]));
    assert_eq!(
        lib.moved(&lib.draft("Album", "Album Title")).await,
        moves([(lib.release("Album"), Some(found("Album").shown("Album Title", Some(""))))])
    );
    let draft = |set: &str| format!("{set} WHERE content_hash = '{}';", hash("Album"));
    assert_eq!(
        lib.moved(&draft("UPDATE import_candidate_edit SET draft_valid = 1")).await,
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
        moves([(lib.release("Album"), Some(found("Album").shown("Album Title", Some(""))))])
    );
    assert_eq!(
        lib.moved(&draft("DELETE FROM import_candidate_state")).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A blank draft shows once a source or a link answered it.
#[tokio::test]
async fn a_blank_draft_shows_once_something_answered_it() {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album"))).await;
    let provenance = format!(
        "INSERT INTO import_candidate_draft_provenance (content_hash, kind)
         VALUES ('{}', 'file_tags');",
        hash("Album")
    );
    assert_eq!(
        lib.moved(&provenance).await,
        moves([(lib.release("Album"), Some(found("Album").shown("", Some(""))))])
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
        moves([(lib.release("Album"), Some(found("Album").shown("Album Title", Some(artists))))])
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
        lib.moved(&format!("UPDATE artists SET name = 'Renamed Artist' WHERE id = '{ARTIST_A}';"))
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
        lib.moved(&format!("DELETE FROM artist_credits WHERE artist_id = '{ARTIST_A}';")).await,
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
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album"))).await;
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
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album"))).await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE import_candidate_state SET edit_revision = 3 WHERE content_hash = '{0}';
             INSERT INTO import_candidate_failure (content_hash, error, failed_at)
             VALUES ('{0}', 'it broke', '2026-01-01T00:00:00Z');",
            hash("Album")
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album").state("import_error")))])
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
            "INSERT INTO import_candidate_standing (candidate_key, import_step, import_id)
             VALUES ({key}, 'queued', 'import-1');"
        ))
        .await,
        now("importing")
    );
    assert_eq!(
        lib.moved(&standing(&format!(
            "UPDATE import_candidate_standing SET import_step = 'reading_files', identification = 'queued', \
             content_hash = '{}', file_edit_revision = 0, admission = 'requested'",
            hash("Album")
        )))
        .await,
        moves([])
    );
    assert_eq!(
        lib.moved(&standing(
            "UPDATE import_candidate_standing SET import_step = NULL, import_id = NULL"
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
        lib.moved(&standing("DELETE FROM import_candidate_standing")).await,
        moves([(lib.release("Album"), Some(found("Album").needs_you("not_found")))])
    );
    // Off Found nothing running is a state.
    lib.run(&format!("INSERT INTO skipped_import_candidates (candidate_path) VALUES ({key});"))
        .await;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO import_candidate_standing (candidate_key, import_step, import_id)
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
        moves([(lib.release("Album"), Some(found("Album").shown("Tagged Title", Some(""))))])
    );
}

/// A release whose stored state is current, ready for a verdict.
async fn looked_up() -> Library {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album"))).await;
    lib
}

/// The one move a write to the verdict of `Album` makes.
fn now(lib: &Library, placed: Placed) -> Moved {
    moves([(lib.release("Album"), Some(placed))])
}

/// The verdict kinds that hold no find place the release by themselves.
#[tokio::test]
async fn each_verdict_kind_places_its_release() {
    for (kind, track_count, placed) in [
        ("not_found", None, found("Album").needs_you("not_found")),
        ("manual_only", Some(11), found("Album").needs_you("nothing_to_look_up")),
        ("failed", Some(11), found("Album").state("lookup_error")),
        ("error", None, found("Album").state("error")),
        // Nothing found that agreement kept.
        ("found", Some(11), found("Album").needs_you("not_found")),
    ] {
        let lib = looked_up().await;
        assert_eq!(
            lib.moved(&lib.verdict("Album", kind, track_count)).await,
            now(&lib, placed),
            "{kind}"
        );
    }
}

/// The person keeping their own draft over the verdict leaves the release
/// unmatched, and taking the verdict away leaves it not looked up.
#[tokio::test]
async fn keeping_ones_own_draft_leaves_the_release_unmatched() {
    let lib = looked_up().await;
    lib.run(&lib.verdict("Album", "not_found", None)).await;
    let verdict = |set: &str| format!("{set} WHERE content_hash = '{}';", hash("Album"));
    assert_eq!(
        lib.moved(&verdict("UPDATE import_candidate_verdict SET kept_own_draft = 1")).await,
        now(&lib, found("Album").state("unmatched"))
    );
    assert_eq!(
        lib.moved(&verdict(
            "UPDATE import_candidate_verdict SET disc_id_step = 'absent', \
             barcode_step = 'absent', catalog_step = 'none_found', isrc_step = 'absent', \
             search_step = 'not_needed'"
        ))
        .await,
        moves([])
    );
    assert_eq!(
        lib.moved(&verdict("DELETE FROM import_candidate_verdict")).await,
        now(&lib, found("Album"))
    );
}

/// A found verdict picks its one pressing when its lead's tracklist fits the
/// folder; otherwise it says why it picks none.
#[tokio::test]
async fn a_found_verdict_is_judged_by_its_lead() {
    for (source_tracks, placed) in [
        ((Some("listed"), Some(11)), found("Album").state("unmatched")),
        ((Some("listed"), Some(12)), found("Album").needs_you("not_found")),
        ((Some("unreadable"), None), found("Album").needs_you("not_found")),
        ((Some("nothing"), None), found("Album").needs_you("no_tracklist")),
        ((None, None), found("Album").needs_you("no_tracklist")),
    ] {
        let lib = looked_up().await;
        lib.run(&lib.verdict("Album", "found", Some(11))).await;
        // Before it found anything, it found nothing that could be the
        // folder's.
        let expected = match placed == found("Album").needs_you("not_found") {
            true => moves([]),
            false => now(&lib, placed),
        };
        assert_eq!(
            lib.moved(&lib.found_release("Album", 0, 0, source_tracks)).await,
            expected,
            "{source_tracks:?}"
        );
    }
}

/// Several pressings are the person's to pick between; the lead is the
/// release at the lowest position agreement kept; a record whose document
/// went unread is a lookup error; and the folder's own files ruling out
/// every release is named over all of it.
#[tokio::test]
async fn a_found_verdict_names_what_holds_its_pick_back() {
    let lib = looked_up().await;
    let fits = (Some("listed"), Some(11));
    lib.run(&format!(
        "{}{}",
        lib.verdict("Album", "found", Some(11)),
        lib.found_release("Album", 1, 0, fits)
    ))
    .await;
    let found_releases =
        |set: &str| format!("{set} WHERE content_hash = '{}' AND position = 0;", hash("Album"));
    assert_eq!(
        lib.moved(&lib.found_release("Album", 0, 1, (Some("listed"), Some(12)))).await,
        now(&lib, found("Album").needs_you("matches"))
    );
    assert_eq!(
        lib.moved(&found_releases("UPDATE import_candidate_match SET listing = 'narrowed_out'"))
            .await,
        now(&lib, found("Album").state("unmatched"))
    );
    assert_eq!(
        lib.moved(&found_releases(
            "UPDATE import_candidate_match SET listing = 'unlisted', pressing = NULL"
        ))
        .await,
        moves([]),
        "a release only the ledger lists is no more offered than one set aside"
    );
    assert_eq!(
        lib.moved(&found_releases(
            "UPDATE import_candidate_match SET listing = 'offered', pressing = 0"
        ))
        .await,
        now(&lib, found("Album").needs_you("not_found"))
    );
    assert_eq!(
        lib.moved(&found_releases(
            "UPDATE import_candidate_match SET source_tracks_count = 11, \
             document_failure = 'network'"
        ))
        .await,
        now(&lib, found("Album").state("lookup_error"))
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE import_candidate_verdict SET medium_conflict = 'cd_rip'
             WHERE content_hash = '{}';",
            hash("Album")
        ))
        .await,
        now(&lib, found("Album").needs_you("medium_mismatch"))
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE import_candidate_verdict SET medium_conflict = NULL
             WHERE content_hash = '{}';
             {}",
            hash("Album"),
            found_releases("DELETE FROM import_candidate_match")
        ))
        .await,
        now(&lib, found("Album").state("unmatched"))
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE import_candidate_match SET title = 'Retitled'
             WHERE content_hash = '{}';",
            hash("Album")
        ))
        .await,
        moves([])
    );
}

/// A verdict counts only through a state at the folder's current files.
#[tokio::test]
async fn a_verdict_of_older_files_places_nothing() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}UPDATE import_candidate_state SET edit_revision = 2 WHERE content_hash = '{}';",
        lib.scan("Album"),
        lib.state("Album"),
        hash("Album")
    ))
    .await;
    assert_eq!(lib.moved(&lib.verdict("Album", "not_found", None)).await, moves([]));
}
