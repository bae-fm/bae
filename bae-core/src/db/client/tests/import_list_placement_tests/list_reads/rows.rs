//! What each row says and offers, which rows the text filter keeps, where a
//! candidate is revealed, and the first one being identified.

use super::*;
use crate::import::{CandidateAction, TriageImportStatus, TriagePlacement};

impl Library {
    /// Credit the draft of the release at `display_path` to `artist`.
    fn credited(&self, display_path: &str, artist: &str) -> String {
        format!(
            "INSERT INTO import_candidate_album_artist_assignment
                 (content_hash, position, assignment_kind, name)
             VALUES ('{}', 0, 'credit', '{artist}');",
            hash(display_path)
        )
    }

    /// What the tags of the folder at `display_path`, which failed
    /// validation, call it.
    fn tagged(&self, display_path: &str, album_title: &str) -> String {
        format!(
            "INSERT INTO scan_skipped_tags (watched_folder_path, candidate_path, album_title)
             VALUES ('{}', '{}', '{album_title}');",
            self.root,
            self.path(display_path)
        )
    }

    /// The items a list on `tab` shows with `needle` typed into its filter.
    async fn found_by(&self, tab: TriageTab, needle: &str) -> Vec<String> {
        self.items(ImportListView {
            filter_text: needle.to_string(),
            ..view(tab)
        })
        .await
    }
}

/// Whether the row, with what is running for it, offers to import.
fn offers_import(live: &crate::import::CandidateLiveState) -> bool {
    live.actions.contains(&CandidateAction::Import)
}

/// A row with no draft shows its folder's name and nothing else, so that is
/// what finds it; its path is not on screen and finds nothing.
#[tokio::test]
async fn the_filter_finds_an_undrafted_row_by_its_folder_name_not_its_path() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Group/Wanted"),
        lib.scan("Other")
    ))
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Pending, "want").await,
        ["group Group", "candidate Group/Wanted"],
        "an emptied group drops its header, a matching one keeps it"
    );
    assert!(lib.found_by(TriageTab::Pending, "group/").await.is_empty());
}

/// A drafted row shows its draft's title and artists — not the verdict's
/// lead, and not its folder — so those are what find it, whatever their case.
#[tokio::test]
async fn the_filter_finds_a_drafted_row_by_the_draft_it_shows() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}",
        scanned_with_states(&lib, &["Folder", "Other"]),
        lib.draft("Folder", "Album"),
        lib.credited("Folder", "Artist"),
        lib.verdict("Folder", "found", Some(11)),
        lib.found_release("Folder", 0, 0, (Some("listed"), Some(11))),
    ))
    .await;
    for needle in ["album", "ARTIST"] {
        assert_eq!(
            lib.found_by(TriageTab::Pending, needle).await,
            ["candidate Folder"],
            "{needle} is on the row"
        );
    }
    for needle in ["album title", "folder"] {
        assert!(
            lib.found_by(TriageTab::Pending, needle).await.is_empty(),
            "{needle} is the verdict's lead or the folder, which the row does not show"
        );
    }
}

/// The filter folds case as library search does, beyond ASCII: a needle in
/// one case finds the text in another.
#[tokio::test]
async fn the_filter_folds_case_beyond_ascii() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        scanned_with_states(&lib, &["Folder"]),
        lib.draft("Folder", "ÉTÉ Œuvre"),
    ))
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Pending, "été œuvre").await,
        ["candidate Folder"]
    );
}

/// What is typed is matched as itself: `*`, `?` and `[`, which a `GLOB`
/// pattern reads as wildcards, find only text holding them.
#[tokio::test]
async fn the_filter_matches_wildcard_characters_as_themselves() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        scanned_with_states(&lib, &["Starred", "Asked", "Bracketed", "Plain"]),
        lib.draft("Starred", "A*B"),
        lib.draft("Asked", "Why?"),
        lib.draft("Bracketed", "[Live]"),
    ))
    .await;
    for (needle, found) in [
        ("*", "candidate Starred"),
        ("a*b", "candidate Starred"),
        ("?", "candidate Asked"),
        ("why?", "candidate Asked"),
        ("[", "candidate Bracketed"),
        ("[live]", "candidate Bracketed"),
    ] {
        assert_eq!(
            lib.found_by(TriageTab::Pending, needle).await,
            [found],
            "{needle}"
        );
    }
}

/// A search of one or two characters, shorter than any run the search index
/// holds, finds every row holding it all the same.
#[tokio::test]
async fn the_filter_finds_one_or_two_characters() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        scanned_with_states(&lib, &["Folder", "Other"]),
        lib.draft("Folder", "Xy"),
        lib.draft("Other", "Yz"),
    ))
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Pending, "y").await,
        ["candidate Folder", "candidate Other"]
    );
    assert_eq!(
        lib.found_by(TriageTab::Pending, "XY").await,
        ["candidate Folder"]
    );
    assert!(lib.found_by(TriageTab::Pending, "q").await.is_empty());
}

/// A Done row shows the row it was in Found — its draft, or its folder while
/// it has none — so that is what finds it, and nothing the library release
/// says does.
#[tokio::test]
async fn the_filter_finds_a_done_row_by_its_found_row() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}",
        scanned_with_states(&lib, &["Folder", "Other"]),
        lib.draft("Other", "Draft Title"),
        lib.credited("Other", "Draft Artist"),
        lib.imported("Folder", "2026-01-02T00:00:00Z"),
        lib.imported("Other", "2026-01-03T00:00:00Z"),
    ))
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Done, "folder").await,
        ["candidate Folder"]
    );
    for needle in ["draft title", "draft artist"] {
        assert_eq!(
            lib.found_by(TriageTab::Done, needle).await,
            ["candidate Other"],
            "{needle} is on the row"
        );
    }
    assert!(
        lib.found_by(TriageTab::Done, "album title")
            .await
            .is_empty(),
        "the library release is not shown"
    );
}

#[tokio::test]
async fn the_filter_matches_an_invalid_folder_by_its_name_and_its_tags() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan_as("Broken", "invalid"),
        lib.scan_as("Tagged", "invalid"),
        lib.tagged("Tagged", "Tagged Title"),
    ))
    .await;
    assert_eq!(
        lib.found_by(TriageTab::Skipped, "broken").await,
        ["invalid Broken"]
    );
    assert_eq!(
        lib.found_by(TriageTab::Skipped, "tagged title").await,
        ["invalid Tagged"]
    );
}

/// A row the text filter or the filter entry narrows is counted beside the
/// tab's total, and only while one narrows it.
#[tokio::test]
async fn a_narrowed_tab_counts_what_it_shows_of_its_total() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Group/Wanted"),
        lib.scan("Group/Other"),
        lib.scan("Loose")
    ))
    .await;
    let narrowed = lib
        .shown(ImportListView {
            filter_text: "wanted".to_string(),
            ..view(TriageTab::Pending)
        })
        .await;
    assert_eq!(
        narrowed.summary.narrowed,
        Some(crate::import::NarrowedCount { shown: 1, total: 3 })
    );
    assert_eq!(
        narrowed.total_count, 2,
        "the header and its matching member"
    );
    assert_eq!(
        narrowed.summary.counts.pending, 3,
        "the counts are the whole queue's"
    );
    assert!(narrowed.summary.has_groups);
    let whole = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(whole.summary.narrowed, None);
    assert_eq!(whole.total_count, 4);
    assert_eq!(whole.summary.watched_folders.len(), 1);
    assert_eq!(
        whole.summary.narrowing,
        crate::import::ImportListNarrowing {
            tab: TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
        }
    );
}

/// A stored verdict places nothing: an identified row is Pending and offers
/// to import its draft.
#[tokio::test]
async fn an_identified_row_is_pending_and_offers_import() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        scanned_with_states(&lib, &["Release"]),
        lib.linked("Release"),
        lib.valid_draft("Release"),
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let (row, live) = row_at(&read, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(
        live.standing,
        Some(crate::import::PendingStanding::Identified)
    );
    assert!(offers_import(live));
}

/// A state for a file shape the candidate has moved past is not the
/// candidate's answer: its draft offers nothing, and its lookup and link
/// place nothing.
#[tokio::test]
async fn a_state_at_a_stale_edit_revision_is_not_the_row_s_answer() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}UPDATE scan_candidate SET file_edit_revision = 2;",
        scanned_with_states(&lib, &["Release"]),
        lib.linked("Release"),
        lib.valid_draft("Release"),
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let (row, live) = row_at(&read, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.release_link, None);
    assert_eq!(row.metadata_summary, None);
    assert_eq!(
        live.standing,
        Some(crate::import::PendingStanding::NotLookedUp)
    );
    assert!(
        !offers_import(live),
        "a draft for other files is not this row's"
    );
}

#[tokio::test]
async fn an_imported_content_hash_puts_its_row_in_done() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        scanned_with_states(&lib, &["Release"]),
        lib.imported("Release", "2026-01-02T00:00:00Z")
    ))
    .await;
    let read = lib.shown(view(TriageTab::Done)).await;
    let (row, live) = row_at(&read, "Release");
    assert_eq!(row.placement, TriagePlacement::Done);
    assert_eq!(live.standing, None, "only Found's rows have a state");
    assert!(matches!(
        row.import_status,
        Some(TriageImportStatus::Complete { ref release }) if release.release_id == lib.release_id("Release")
    ));
    assert_eq!(read.summary.counts.done, 1);
}

#[tokio::test]
async fn a_skipped_candidate_lands_in_skipped() {
    let lib = Library::new().await;
    lib.run(&format!("{}{}", lib.scan("Release"), lib.skip("Release")))
        .await;
    let read = lib.shown(view(TriageTab::Skipped)).await;
    assert_eq!(
        row_at(&read, "Release").0.placement,
        TriagePlacement::Skipped
    );
    assert_eq!(read.summary.counts.skipped, 1);
}

/// An import running for a candidate places nothing: until it writes the
/// release the row is Pending, and the import is the row's live state — which
/// offers nothing but its cancel while it runs.
#[tokio::test]
async fn a_claimed_import_leaves_the_row_pending() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        scanned_with_states(&lib, &["Release"]),
        lib.valid_draft("Release"),
        lib.importing("Release", "running")
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let (row, live) = row_at(&read, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.import_status, None);
    assert_eq!(
        live.standing,
        Some(crate::import::PendingStanding::Importing)
    );
    assert_eq!(
        live.actions,
        [CandidateAction::CancelImport, CandidateAction::RevealFolder]
    );
}

/// The failure is a row, so it survives the session that produced it: a
/// relaunched queue still says why the attempt failed. It stays on Pending —
/// nothing was imported — and importing it again is the ordinary import.
/// Retrying clears it, and the release it writes outranks any failure left.
#[tokio::test]
async fn a_failed_import_stays_pending_until_its_retry_lands_in_done() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        scanned_with_states(&lib, &["Release"]),
        lib.valid_draft("Release"),
        lib.import_failed("Release"),
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let (row, live) = row_at(&read, "Release");
    assert_eq!(row.placement, TriagePlacement::Failed);
    assert_eq!(
        row.import_status,
        Some(TriageImportStatus::Error {
            failure: crate::import::ImportFailureReason::error("it broke")
        })
    );
    assert_eq!(
        live.standing,
        Some(crate::import::PendingStanding::ImportError)
    );
    assert!(offers_import(live));
    assert_eq!(read.summary.counts.pending, 1);

    lib.run("DELETE FROM import_candidate_failure;").await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        row_at(&read, "Release").0.placement,
        TriagePlacement::Pending
    );

    lib.run(&format!(
        "{}{}",
        lib.import_failed("Release"),
        lib.imported("Release", "2026-01-02T00:00:00Z")
    ))
    .await;
    let done = lib.shown(view(TriageTab::Done)).await;
    assert!(matches!(
        row_at(&done, "Release").0.import_status,
        Some(TriageImportStatus::Complete { .. })
    ));
    assert_eq!(done.summary.counts.pending, 0);
}

/// Import is offered by one rule, whoever wrote the draft and whatever
/// identification found: a draft that would import offers it, and one that
/// would not does not.
#[tokio::test]
async fn a_row_offers_import_exactly_when_its_draft_would_import() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}{}{}{}",
        scanned_with_states(&lib, &["Valid", "Several", "Picked", "Not Found"]),
        lib.valid_draft("Valid"),
        lib.verdict("Several", "found", Some(11)),
        lib.found_release("Several", 0, 0, (Some("listed"), Some(11))),
        lib.found_release("Several", 1, 1, (Some("listed"), Some(11))),
        lib.linked("Picked"),
        lib.valid_draft("Picked"),
        lib.verdict("Not Found", "not_found", None),
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    for (display_path, offers) in [
        ("Valid", true),
        ("Several", false),
        ("Picked", true),
        ("Not Found", false),
    ] {
        let (row, live) = row_at(&read, display_path);
        assert_eq!(row.placement, TriagePlacement::Pending, "{display_path}");
        assert_eq!(offers_import(live), offers, "{display_path}");
    }
}

/// A pick does not outrank the two facts above it: a skipped candidate stays
/// skipped, and an imported one stays done.
#[tokio::test]
async fn a_pick_does_not_outrank_skipped_or_done() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}",
        scanned_with_states(&lib, &["Skipped", "Imported"]),
        lib.linked("Skipped"),
        lib.linked("Imported"),
        lib.skip("Skipped"),
        lib.imported("Imported", "2026-01-02T00:00:00Z"),
    ))
    .await;
    assert_eq!(
        row_at(&lib.shown(view(TriageTab::Skipped)).await, "Skipped")
            .0
            .placement,
        TriagePlacement::Skipped
    );
    assert_eq!(
        row_at(&lib.shown(view(TriageTab::Done)).await, "Imported")
            .0
            .placement,
        TriagePlacement::Done
    );
}

/// A row a run is identifying stays where the tables place it, and only its
/// own live state keeps an import off it while the run is in flight.
#[tokio::test]
async fn a_candidate_under_identification_offers_only_its_cancel_and_skip() {
    for stage in ["queued", "running", "finalizing"] {
        let lib = Library::new().await;
        lib.run(&format!(
            "{}{}{}",
            scanned_with_states(&lib, &["Release"]),
            lib.valid_draft("Release"),
            lib.identifying("Release", stage),
        ))
        .await;
        let read = lib.shown(view(TriageTab::Pending)).await;
        let (row, live) = row_at(&read, "Release");
        assert_eq!(row.placement, TriagePlacement::Pending, "{stage}");
        assert_eq!(
            live.actions,
            [
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ],
            "{stage}"
        );
    }
}

/// The first of several identifying candidates is the one the queue reaches
/// first, not the one named first, whatever the filter hides or the tab on
/// show; a group's is its first member.
#[tokio::test]
async fn the_first_identifying_candidate_follows_the_queue_order() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Release 1"),
        lib.scan("Release 2"),
        lib.identifying("Release 2", "running")
    ))
    .await;
    let first = |view: ImportListView| async {
        let list = lib.open(view).await;
        lib.db
            .first_identifying_import_candidate(list)
            .await
            .unwrap()
    };
    assert_eq!(
        first(view(TriageTab::Pending)).await,
        Some(lib.path("Release 2"))
    );
    let filtered = ImportListView {
        filter_text: "Release 1".to_string(),
        ..view(TriageTab::Pending)
    };
    assert_eq!(first(filtered).await, Some(lib.path("Release 2")));
    assert_eq!(
        first(view(TriageTab::Done)).await,
        Some(lib.path("Release 2"))
    );
    lib.run(&lib.identifying("Release 1", "queued")).await;
    assert_eq!(
        first(view(TriageTab::Pending)).await,
        Some(lib.path("Release 1"))
    );
    lib.run("DELETE FROM import_candidate_standing;").await;
    assert_eq!(first(view(TriageTab::Pending)).await, None);
}

#[tokio::test]
async fn the_first_identifying_candidate_of_a_group_is_its_first_member() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        lib.scan("Group/Release 1"),
        lib.scan("Group/Release 2"),
        lib.identifying("Group/Release 2", "running"),
        lib.identifying("Group/Release 1", "running"),
    ))
    .await;
    let list = lib.open(view(TriageTab::Pending)).await;
    assert_eq!(
        lib.db
            .first_identifying_import_candidate(list)
            .await
            .unwrap(),
        Some(lib.path("Group/Release 1"))
    );
}

/// Locating a candidate clears the filters and opens its group, the list's
/// other folds kept.
#[tokio::test]
async fn candidate_location_opens_only_its_group() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}",
        lib.scan("Earlier/Release 1"),
        lib.scan("Earlier/Release 2"),
        lib.scan("Target/Release 1")
    ))
    .await;
    let list = lib
        .open(ImportListView {
            filter_text: "does not match".to_string(),
            pending_filter: PendingFilter::NeedsYou,
            ..view(TriageTab::Pending)
        })
        .await;
    let fold = |group: &str| crate::import::FolderReleaseDecisionKey {
        watched_folder_path: lib.root.clone(),
        relative_folder_path: group.to_string(),
    };
    lib.db
        .set_import_list_groups_folded(list, true)
        .await
        .unwrap();
    let location = lib
        .db
        .locate_import_candidate(list, &lib.path("Target/Release 1"))
        .await
        .unwrap()
        .expect("the candidate is in Pending");
    assert_eq!(location.tab, TriageTab::Pending);
    assert_eq!(location.group_key, Some(fold("Target")));
    assert_eq!(
        location.stable_key,
        format!("candidate:{}", lib.path("Target/Release 1"))
    );
    assert_eq!(location.visible_position, 2);
}

#[tokio::test]
async fn candidate_location_follows_an_import_from_pending_to_done() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        scanned_with_states(&lib, &["Release"]),
        lib.imported("Release", "2026-01-02T00:00:00Z")
    ))
    .await;
    let list = lib.open(view(TriageTab::Pending)).await;
    let location = lib
        .db
        .locate_import_candidate(list, &lib.path("Release"))
        .await
        .unwrap()
        .expect("the imported candidate is in Done");
    assert_eq!(location.tab, TriageTab::Done);
    assert_eq!(location.group_key, None);
    assert_eq!(location.visible_position, 0);
    assert_eq!(
        lib.db
            .locate_import_candidate(list, &lib.path("Nowhere"))
            .await
            .unwrap(),
        None
    );
}
