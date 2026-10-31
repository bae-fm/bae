//! Where each of Found's rows stands, what is running for it included: the
//! state and why it needs the person, the filter entry that holds it, and
//! what the entries count.

use super::*;
use crate::identify::MediumConflict;
use crate::import::{NeedsYouReason, PendingFilterEntry, PendingStanding};
use crate::signals::InternalFailure;

impl Library {
    /// Write `title` into the draft of the release at `display_path` and
    /// credit it to an artist: a draft the row shows.
    fn drafted(&self, display_path: &str) -> String {
        format!(
            "{}INSERT INTO import_candidate_album_artist_assignment
                 (content_hash, position, assignment_kind, name)
             VALUES ('{}', 0, 'credit', 'Artist');",
            self.draft(display_path, "Album"),
            hash(display_path)
        )
    }

    /// The draft of the release at `display_path` read from its files' tags,
    /// by the tag prefill.
    fn read_from_tags(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO import_candidate_draft_provenance (content_hash, kind)
             VALUES ('{h}', 'file_tags');
             UPDATE import_candidate_edit SET author = 'prefill' WHERE content_hash = '{h}';",
            h = hash(display_path)
        )
    }

    /// A found verdict over eleven tracks, whose releases agreement kept make
    /// `pressings` pressings, each listing `source_tracks`.
    fn found(
        &self,
        display_path: &str,
        pressings: i64,
        source_tracks: (Option<&str>, Option<i64>),
    ) -> String {
        let mut sql = self.verdict(display_path, "found", Some(11));
        for pressing in 0..pressings {
            sql.push_str(&self.found_release(display_path, pressing, pressing, source_tracks));
        }
        sql
    }

    /// A run of the release at `display_path` whose answer bae could not
    /// write, saying `error`.
    fn not_written(&self, display_path: &str, error: &str) -> String {
        format!(
            "INSERT INTO import_candidate_standing
                 (candidate_key, identification, content_hash, file_edit_revision,
                  save_failure_error)
             VALUES ({}, 'not_written', '{}', 0, '{error}');",
            self.sql(display_path),
            hash(display_path)
        )
    }

    /// A run of the release at `display_path` that picked its release
    /// unattended, which is being read in full.
    fn reading_pick(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO import_candidate_standing
                 (candidate_key, identification, content_hash, file_edit_revision, identify_run,
                  import_asked)
             VALUES ({}, 'reading_pick', '{}', 0, 1, 0);",
            self.sql(display_path),
            hash(display_path)
        )
    }
}

/// The releases a fit of eleven tracks lists.
const FITS: (Option<&str>, Option<i64>) = (Some("listed"), Some(11));

/// Every kind of Found row the rules tell apart, and a Done and a Skipped
/// one, written as the rows the tables hold.
async fn every_kind() -> Library {
    let lib = Library::new().await;
    let all = [
        "Identified",
        "Picked Among Several",
        "Track Count Differs",
        "Several",
        "Several Tagged",
        "Nothing Found",
        "Lookup Failed",
        "Failed Import",
        "Tagged",
        "Unidentified",
        "Pick Set Aside",
        "Unread",
        "Nothing To Look Up",
        "Broke",
        "Imported",
        "Set Aside",
    ];
    let mut sql = scanned_with_states(&lib, &all);
    for display_path in [
        "Identified",
        "Picked Among Several",
        "Track Count Differs",
        "Several Tagged",
        "Failed Import",
        "Tagged",
        "Pick Set Aside",
    ] {
        sql.push_str(&lib.drafted(display_path));
    }
    for display_path in [
        "Identified",
        "Picked Among Several",
        "Track Count Differs",
        "Failed Import",
    ] {
        sql.push_str(&lib.linked(display_path));
        sql.push_str(&lib.valid_draft(display_path));
    }
    for display_path in ["Several Tagged", "Tagged", "Pick Set Aside"] {
        sql.push_str(&lib.read_from_tags(display_path));
    }
    sql.push_str(&lib.valid_draft("Several Tagged"));
    sql.push_str(&lib.valid_draft("Tagged"));
    sql.push_str(
        "UPDATE import_candidate_edit SET author = 'person' WHERE content_hash = 'hash-Pick Set Aside';",
    );
    sql.push_str(&lib.found("Identified", 1, FITS));
    sql.push_str(&lib.found("Picked Among Several", 2, FITS));
    sql.push_str(&lib.found("Track Count Differs", 1, (Some("listed"), Some(10))));
    sql.push_str(&lib.found("Several", 2, FITS));
    sql.push_str(&lib.found("Several Tagged", 2, FITS));
    sql.push_str(&lib.found("Pick Set Aside", 1, FITS));
    sql.push_str(&lib.found("Unread", 1, FITS));
    sql.push_str(&format!(
        "UPDATE import_candidate_match SET document_failure = 'timeout'
         WHERE content_hash = '{}';",
        hash("Unread")
    ));
    sql.push_str(&lib.verdict("Nothing Found", "not_found", None));
    sql.push_str(&lib.verdict("Lookup Failed", "failed", Some(11)));
    sql.push_str(&lib.verdict("Nothing To Look Up", "manual_only", Some(11)));
    sql.push_str(&lib.verdict("Broke", "error", None));
    sql.push_str(&lib.import_failed("Failed Import"));
    sql.push_str(&lib.imported("Imported", "2026-01-02T00:00:00Z"));
    sql.push_str(&lib.skip("Set Aside"));
    lib.run(&sql).await;
    lib
}

/// Where three of [`every_kind`]'s candidates stand: a run for Several, an
/// import of Identified, and a run whose answer bae could not store for
/// Tagged.
async fn running(lib: &Library) {
    lib.run(&format!(
        "{}{}{}",
        lib.identifying("Several", "running"),
        lib.importing("Identified", "running"),
        lib.not_written("Tagged", "writing the answer: the disk is full"),
    ))
    .await;
}

/// The release rows `view` shows, sorted, one readable line each.
async fn sorted(lib: &Library, view: ImportListView) -> Vec<String> {
    let mut shown = lib.items(view).await;
    shown.sort();
    shown
}

/// Each entry's count of one read, in the menu's order.
fn counts(entries: &[PendingFilterEntry]) -> Vec<(PendingFilter, u32)> {
    entries
        .iter()
        .map(|entry| (entry.filter, entry.count))
        .collect()
}

/// Each entry shows exactly the rows in the states it covers, with what is
/// running for a candidate deciding its state over the tables; the entries
/// past All split Found's rows between them.
#[tokio::test]
async fn each_entry_shows_exactly_the_rows_of_its_states() {
    let lib = every_kind().await;
    running(&lib).await;
    let cases = [
        (
            PendingFilter::NeedsYou,
            vec![
                "candidate Broke",
                "candidate Failed Import",
                "candidate Lookup Failed",
                "candidate Nothing Found",
                "candidate Nothing To Look Up",
                "candidate Several Tagged",
                "candidate Tagged",
                "candidate Unread",
            ],
        ),
        (
            PendingFilter::InProgress,
            vec!["candidate Identified", "candidate Several"],
        ),
        (
            PendingFilter::Identified,
            vec![
                "candidate Picked Among Several",
                "candidate Track Count Differs",
            ],
        ),
        (PendingFilter::Unmatched, vec!["candidate Pick Set Aside"]),
        (PendingFilter::NotLookedUp, vec!["candidate Unidentified"]),
    ];
    let mut every: Vec<String> = Vec::new();
    for (filter, expected) in cases {
        let shown = sorted(&lib, under(TriageTab::Pending, filter)).await;
        assert_eq!(shown, expected, "{filter:?}");
        every.extend(shown);
    }
    every.sort();
    assert_eq!(
        every,
        sorted(&lib, under(TriageTab::Pending, PendingFilter::All)).await,
        "each row shows under exactly one entry past All, and All shows them all"
    );
}

/// Each entry counts the rows it holds, from a read under any view, with
/// where each candidate stands deciding its row's state over the tables: the
/// count is the rows the list shows under that entry, All's is Found's total,
/// and an entry holding a row can be chosen.
#[tokio::test]
async fn each_entry_counts_the_rows_it_holds() {
    let lib = every_kind().await;
    let at_rest = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        counts(&at_rest.pending_filter_entries),
        [
            (PendingFilter::All, 14),
            (PendingFilter::NeedsYou, 8),
            (PendingFilter::InProgress, 0),
            (PendingFilter::Identified, 3),
            (PendingFilter::Unmatched, 1),
            (PendingFilter::NotLookedUp, 2),
        ]
    );
    let in_progress = at_rest
        .pending_filter_entries
        .iter()
        .find(|entry| entry.filter == PendingFilter::InProgress)
        .expect("In Progress is an entry");
    assert!(
        !in_progress.selectable,
        "an entry holding no row cannot be chosen"
    );

    running(&lib).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(
        counts(&read.pending_filter_entries),
        [
            (PendingFilter::All, 14),
            (PendingFilter::NeedsYou, 8),
            (PendingFilter::InProgress, 2),
            (PendingFilter::Identified, 2),
            (PendingFilter::Unmatched, 1),
            (PendingFilter::NotLookedUp, 1),
        ]
    );
    assert_eq!(read.summary.counts.pending, 14);
    assert!(read
        .pending_filter_entries
        .iter()
        .all(|entry| entry.selectable));
    for entry in &read.pending_filter_entries {
        let shown = lib.shown(under(TriageTab::Pending, entry.filter)).await;
        assert_eq!(shown.total_count as u32, entry.count, "{:?}", entry.filter);
        if entry.filter != PendingFilter::All {
            assert_eq!(
                shown.summary.narrowed,
                Some(crate::import::NarrowedCount {
                    shown: entry.count,
                    total: 14
                }),
                "the list's count under {:?} is the entry's",
                entry.filter
            );
        }
    }
    for other in [
        ImportListView {
            filter_text: "several".to_string(),
            ..under(TriageTab::Pending, PendingFilter::NeedsYou)
        },
        view(TriageTab::Done),
        view(TriageTab::Skipped),
    ] {
        assert_eq!(
            lib.shown(other.clone()).await.pending_filter_entries,
            read.pending_filter_entries,
            "the counts are Found's whatever {other:?} shows"
        );
    }
}

/// An entry chosen while it held rows stays chosen once it holds none: the
/// list shows nothing under it, and says so against Found's total.
#[tokio::test]
async fn a_chosen_entry_holding_no_row_stays_chosen() {
    let lib = every_kind().await;
    running(&lib).await;
    let list = lib
        .open(under(TriageTab::Pending, PendingFilter::InProgress))
        .await;
    assert_eq!(lib.read(list).await.total_count, 2);
    lib.run("DELETE FROM import_candidate_standing;").await;
    let ended = lib.read(list).await;
    assert_eq!(ended.total_count, 0);
    assert_eq!(
        ended.summary.narrowing.pending_filter,
        PendingFilter::InProgress
    );
    assert_eq!(
        ended.summary.narrowed,
        Some(crate::import::NarrowedCount {
            shown: 0,
            total: 14
        })
    );
}

/// Done and Skipped rows are past identification, so an entry chosen on
/// Found leaves them alone.
#[tokio::test]
async fn the_pending_filter_leaves_done_and_skipped_alone() {
    let lib = every_kind().await;
    for filter in PendingFilter::ENTRIES {
        assert_eq!(
            lib.items(under(TriageTab::Done, filter)).await,
            ["candidate Imported"]
        );
        assert_eq!(
            lib.items(under(TriageTab::Skipped, filter)).await,
            ["candidate Set Aside"]
        );
        assert_eq!(
            lib.shown(under(TriageTab::Done, filter))
                .await
                .summary
                .narrowed,
            None,
            "the entry leaves Done alone"
        );
    }
}

/// The entry composes with the text filter: both have to keep a row.
#[tokio::test]
async fn the_text_filter_composes_with_the_pending_filter() {
    let lib = every_kind().await;
    let texted = |filter: PendingFilter, text: &str| ImportListView {
        filter_text: text.to_string(),
        ..under(TriageTab::Pending, filter)
    };
    let needs_you = lib.shown(texted(PendingFilter::NeedsYou, "album")).await;
    let mut shown = sequence(&needs_you);
    shown.sort();
    assert_eq!(
        shown,
        ["candidate Failed Import", "candidate Several Tagged"],
        "of the Needs You rows only the drafted ones read Album"
    );
    assert_eq!(needs_you.summary.counts.pending, 14);
    assert!(lib
        .items(texted(PendingFilter::Identified, "nothing"))
        .await
        .is_empty());
    assert_eq!(
        lib.shown(texted(PendingFilter::All, "several"))
            .await
            .summary
            .narrowed,
        Some(crate::import::NarrowedCount {
            shown: 1,
            total: 14
        }),
        "only the row with no draft reads its folder's name"
    );
    assert_eq!(
        lib.shown(ImportListView {
            filter_text: "set aside".to_string(),
            ..view(TriageTab::Skipped)
        })
        .await
        .summary
        .narrowed,
        Some(crate::import::NarrowedCount { shown: 1, total: 1 })
    );
}

/// Locating a candidate clears the filters, so a row the entry hides is
/// still found where it sits.
#[tokio::test]
async fn locating_a_candidate_ignores_the_pending_filter() {
    let lib = every_kind().await;
    let list = lib
        .open(under(TriageTab::Pending, PendingFilter::Identified))
        .await;
    let location = lib
        .db
        .locate_import_candidate(list, &lib.key("Unidentified"))
        .await
        .unwrap()
        .expect("the candidate is found");
    assert_eq!(location.tab, TriageTab::Pending);
}

/// Each stored state reads back as the standing the row wears: why a row
/// needs the person, with how many pressings it could be and what the
/// folder's files say; how bae broke; a link answering whatever the lookup
/// came to.
#[tokio::test]
async fn each_row_reads_back_in_the_standing_its_tables_put_it_in() {
    let lib = Library::new().await;
    let releases = [
        "Several",
        "No Tracklist",
        "Medium",
        "Not Found",
        "Nothing Found",
        "Manual",
        "Broke",
        "Lookup Failed",
        "Unread",
        "Kept",
        "Kept Then Linked",
        "Fits",
        "Unidentified",
    ];
    let mut sql = scanned_with_states(&lib, &releases);
    sql.push_str(&lib.found("Several", 3, FITS));
    sql.push_str(&lib.found("No Tracklist", 1, (Some("nothing"), None)));
    sql.push_str(&lib.found("Medium", 2, FITS));
    sql.push_str(&format!(
        "UPDATE import_candidate_verdict SET medium_conflict = 'cd_rip'
         WHERE content_hash = '{}';",
        hash("Medium")
    ));
    sql.push_str(&lib.verdict("Not Found", "not_found", None));
    sql.push_str(&lib.verdict("Nothing Found", "found", Some(11)));
    sql.push_str(&lib.verdict("Manual", "manual_only", Some(11)));
    sql.push_str(&lib.verdict("Broke", "error", None));
    sql.push_str(&lib.verdict("Lookup Failed", "failed", Some(11)));
    sql.push_str(&lib.found("Unread", 1, FITS));
    sql.push_str(&format!(
        "UPDATE import_candidate_match SET document_failure = 'timeout'
         WHERE content_hash = '{}';",
        hash("Unread")
    ));
    for kept in ["Kept", "Kept Then Linked"] {
        sql.push_str(&lib.found(kept, 3, FITS));
        sql.push_str(&format!(
            "UPDATE import_candidate_verdict SET kept_own_draft = 1 WHERE content_hash = '{}';",
            hash(kept)
        ));
    }
    sql.push_str(&lib.linked("Kept Then Linked"));
    sql.push_str(&lib.found("Fits", 1, FITS));
    lib.run(&sql).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let standing = |display_path: &str| row_at(&read, display_path).1.standing.clone();
    let needs_you = |reason| Some(PendingStanding::NeedsYou { reason });
    assert_eq!(
        standing("Several"),
        needs_you(NeedsYouReason::Matches { count: 3 })
    );
    assert_eq!(
        standing("No Tracklist"),
        needs_you(NeedsYouReason::NoTracklist)
    );
    assert_eq!(
        standing("Medium"),
        needs_you(NeedsYouReason::MediumMismatch {
            folder: MediumConflict::CdRip,
            releases: 2
        })
    );
    assert_eq!(standing("Not Found"), needs_you(NeedsYouReason::NotFound));
    assert_eq!(
        standing("Nothing Found"),
        needs_you(NeedsYouReason::NotFound)
    );
    assert_eq!(
        standing("Manual"),
        needs_you(NeedsYouReason::NothingToLookUp)
    );
    assert_eq!(
        standing("Broke"),
        Some(PendingStanding::Error {
            failure: InternalFailure {
                detail: "bae broke".to_string()
            }
        })
    );
    assert_eq!(
        standing("Lookup Failed"),
        Some(PendingStanding::LookupError)
    );
    assert_eq!(standing("Unread"), Some(PendingStanding::LookupError));
    assert_eq!(standing("Kept"), Some(PendingStanding::Unmatched));
    assert_eq!(
        standing("Kept Then Linked"),
        Some(PendingStanding::Identified)
    );
    assert_eq!(
        standing("Fits"),
        Some(PendingStanding::Unmatched),
        "a release the lookup took for the folder, no longer linked, was set aside"
    );
    assert_eq!(standing("Unidentified"), Some(PendingStanding::NotLookedUp));
    for (display_path, badge) in [
        ("Several", true),
        ("Broke", true),
        ("Unread", true),
        ("Kept", false),
        ("Unidentified", false),
    ] {
        assert_eq!(
            row_at(&read, display_path)
                .1
                .standing
                .as_ref()
                .and_then(PendingStanding::badge)
                .is_some(),
            badge,
            "{display_path}"
        );
    }
}

/// What is running for a candidate decides its state over the tables: an
/// import over a run, a run over any stored state, a pick being read
/// Identified, and an answer bae could not store an error with why. Once it
/// ends, the stored state is back, why it needs the person with it.
#[tokio::test]
async fn running_work_decides_over_the_tables() {
    let lib = Library::new().await;
    let releases = ["Running", "Importing", "Reading", "Not Written"];
    let mut sql = scanned_with_states(&lib, &releases);
    for display_path in releases {
        sql.push_str(&lib.verdict(display_path, "not_found", None));
    }
    sql.push_str(&lib.identifying("Running", "running"));
    sql.push_str(&lib.importing("Importing", "queued"));
    sql.push_str(&format!(
        "UPDATE import_candidate_standing SET identification = 'queued', content_hash = '{}',
             file_edit_revision = 0, admission = 'requested'
         WHERE candidate_key = {};",
        hash("Importing"),
        lib.sql("Importing")
    ));
    sql.push_str(&lib.reading_pick("Reading"));
    sql.push_str(&lib.not_written("Not Written", "disk full"));
    lib.run(&sql).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let standing = |display_path: &str| row_at(&read, display_path).1.standing.clone();
    assert_eq!(standing("Running"), Some(PendingStanding::Identifying));
    assert_eq!(standing("Importing"), Some(PendingStanding::Importing));
    assert_eq!(standing("Reading"), Some(PendingStanding::Identified));
    assert_eq!(
        standing("Not Written"),
        Some(PendingStanding::Error {
            failure: InternalFailure {
                detail: "disk full".to_string()
            }
        })
    );
    assert_eq!(
        row_at(&read, "Not Written")
            .1
            .standing
            .as_ref()
            .and_then(PendingStanding::badge),
        Some(crate::import::PendingBadge::Error)
    );

    lib.run("DELETE FROM import_candidate_standing;").await;
    let at_rest = lib.shown(view(TriageTab::Pending)).await;
    for display_path in releases {
        assert_eq!(
            row_at(&at_rest, display_path).1.standing,
            Some(PendingStanding::NeedsYou {
                reason: NeedsYouReason::NotFound
            }),
            "{display_path}"
        );
    }
}

/// A row shows its lookup at work exactly while its state is Identifying,
/// which In Progress counts: one waiting for its run shows it queued, one
/// running or writing an answer that picks nothing shows it running, and one
/// whose run picked its release unattended, read in full now, is Identified
/// and shows none.
#[tokio::test]
async fn a_row_shows_its_lookup_at_work_only_while_in_progress_counts_it() {
    let lib = Library::new().await;
    let releases = ["Queued", "Running", "Finalizing", "Reading"];
    let mut sql = scanned_with_states(&lib, &releases);
    for display_path in releases {
        sql.push_str(&lib.verdict(display_path, "not_found", None));
    }
    sql.push_str(&lib.identifying("Queued", "queued"));
    sql.push_str(&lib.identifying("Running", "running"));
    sql.push_str(&lib.identifying("Finalizing", "finalizing"));
    sql.push_str(&lib.reading_pick("Reading"));
    lib.run(&sql).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let shown = |display_path: &str| {
        let live = row_at(&read, display_path).1;
        (live.standing.clone(), live.identifying())
    };
    use crate::import::IdentifyingStep;
    assert_eq!(
        shown("Queued"),
        (
            Some(PendingStanding::Identifying),
            Some(IdentifyingStep::Queued)
        )
    );
    assert_eq!(
        shown("Running"),
        (
            Some(PendingStanding::Identifying),
            Some(IdentifyingStep::Running)
        )
    );
    assert_eq!(
        shown("Finalizing"),
        (
            Some(PendingStanding::Identifying),
            Some(IdentifyingStep::Running)
        )
    );
    assert_eq!(
        shown("Reading"),
        (Some(PendingStanding::Identified), None),
        "a pick being read is Identified, and shows no lookup at work"
    );
    assert_eq!(
        counts(&read.pending_filter_entries),
        [
            (PendingFilter::All, 4),
            (PendingFilter::NeedsYou, 0),
            (PendingFilter::InProgress, 3),
            (PendingFilter::Identified, 1),
            (PendingFilter::Unmatched, 0),
            (PendingFilter::NotLookedUp, 0),
        ]
    );
}

/// The pane shows the release a run picked as being read exactly while that
/// pick is read in full, an Import asked of it or not: never while the lookup
/// waits, runs, or writes an answer that picks nothing.
#[tokio::test]
async fn a_pane_shows_a_pick_being_read_only_while_it_is_read() {
    let lib = Library::new().await;
    let releases = [
        "Queued",
        "Running",
        "Finalizing",
        "Reading",
        "Asked",
        "Idle",
    ];
    let mut sql = scanned_with_states(&lib, &releases);
    sql.push_str(&lib.identifying("Queued", "queued"));
    sql.push_str(&lib.identifying("Running", "running"));
    sql.push_str(&lib.identifying("Finalizing", "finalizing"));
    sql.push_str(&lib.reading_pick("Reading"));
    sql.push_str(&lib.reading_pick("Asked"));
    sql.push_str(&format!(
        "UPDATE import_candidate_standing SET import_asked = 1 WHERE candidate_key = {};",
        lib.sql("Asked")
    ));
    lib.run(&sql).await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    for (display_path, reading) in [
        ("Queued", false),
        ("Running", false),
        ("Finalizing", false),
        ("Reading", true),
        ("Asked", true),
        ("Idle", false),
    ] {
        assert_eq!(
            row_at(&read, display_path).1.reading_pick(),
            reading,
            "{display_path}"
        );
    }
}
