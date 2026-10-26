//! What a stored verdict places a Found release as: the lookup's own answer,
//! and why a found one picks none of its releases unattended.

use super::*;

/// A release whose stored state is current, ready for a verdict.
async fn looked_up() -> Library {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Album"), lib.state("Album")))
        .await;
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
        (
            "manual_only",
            Some(11),
            found("Album").needs_you("nothing_to_look_up"),
        ),
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
        lib.moved(&verdict(
            "UPDATE import_candidate_verdict SET kept_own_draft = 1"
        ))
        .await,
        now(&lib, found("Album").state("unmatched"))
    );
    assert_eq!(
        lib.moved(&verdict(
            "UPDATE import_candidate_verdict SET identified_at = '2026-02-01T00:00:00Z'"
        ))
        .await,
        moves([])
    );
    assert_eq!(
        lib.moved(&verdict("DELETE FROM import_candidate_verdict"))
            .await,
        now(&lib, found("Album"))
    );
}

/// A found verdict picks its one pressing when its lead's tracklist fits the
/// folder; otherwise it says why it picks none.
#[tokio::test]
async fn a_found_verdict_is_judged_by_its_lead() {
    for (source_tracks, placed) in [
        (
            (Some("listed"), Some(11)),
            found("Album").state("unmatched"),
        ),
        (
            (Some("listed"), Some(12)),
            found("Album").needs_you("not_found"),
        ),
        (
            (Some("unreadable"), None),
            found("Album").needs_you("not_found"),
        ),
        (
            (Some("nothing"), None),
            found("Album").needs_you("no_tracklist"),
        ),
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
            lib.moved(&lib.found_release("Album", 0, 0, source_tracks))
                .await,
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
    let found_releases = |set: &str| {
        format!(
            "{set} WHERE content_hash = '{}' AND position = 0;",
            hash("Album")
        )
    };
    assert_eq!(
        lib.moved(&lib.found_release("Album", 0, 1, (Some("listed"), Some(12))))
            .await,
        now(&lib, found("Album").needs_you("matches"))
    );
    assert_eq!(
        lib.moved(&found_releases(
            "UPDATE import_candidate_match SET narrowed_out = 1"
        ))
        .await,
        now(&lib, found("Album").state("unmatched"))
    );
    assert_eq!(
        lib.moved(&found_releases(
            "UPDATE import_candidate_match SET narrowed_out = 0, pressing = 0"
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
    assert_eq!(
        lib.moved(&lib.verdict("Album", "not_found", None)).await,
        moves([])
    );
}
