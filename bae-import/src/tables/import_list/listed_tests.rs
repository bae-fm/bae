//! Where the list places a release and what its row offers: its tab, its
//! state on Found and why it needs the person, and what is running for it
//! deciding over the stored tables.

use super::*;
use crate::list::ImportListItem;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use crate::TriageTab;
use crate::{CandidateAction, TriageImportStatus, TriagePlacement};
use crate::{NeedsYouReason, PendingStanding};
use bae_catalogs::InternalFailure;
use bae_identify::MediumConflict;
use bae_identify::TerminalVerdict;
use bae_store::test_fixtures::fixed_now;

/// Whether the row, with what is running for it, offers to import.
fn offers_import(live: &crate::CandidateLiveState) -> bool {
    live.actions.contains(&CandidateAction::Import)
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
    assert_eq!(live.standing, Some(crate::PendingStanding::Identified));
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
    assert_eq!(live.standing, Some(crate::PendingStanding::NotLookedUp));
    assert!(!offers_import(live), "a draft for other files is not this row's");
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
    lib.run(&format!("{}{}", lib.scan("Release"), lib.skip("Release"))).await;
    let read = lib.shown(view(TriageTab::Skipped)).await;
    assert_eq!(row_at(&read, "Release").0.placement, TriagePlacement::Skipped);
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
        lib.importing("Release", "reading_files")
    ))
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    let (row, live) = row_at(&read, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.import_status, None);
    assert_eq!(live.standing, Some(crate::PendingStanding::Importing));
    assert_eq!(live.actions, [CandidateAction::CancelImport, CandidateAction::RevealFolder]);
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
        Some(TriageImportStatus::Error { failure: crate::ImportFailureReason::error("it broke") })
    );
    assert_eq!(live.standing, Some(crate::PendingStanding::ImportError));
    assert!(offers_import(live));
    assert_eq!(read.summary.counts.pending, 1);

    lib.run("DELETE FROM import_candidate_failure;").await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(row_at(&read, "Release").0.placement, TriagePlacement::Pending);

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
    for (display_path, offers) in
        [("Valid", true), ("Several", false), ("Picked", true), ("Not Found", false)]
    {
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
        row_at(&lib.shown(view(TriageTab::Skipped)).await, "Skipped").0.placement,
        TriagePlacement::Skipped
    );
    assert_eq!(
        row_at(&lib.shown(view(TriageTab::Done)).await, "Imported").0.placement,
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

impl Library {
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
    assert_eq!(standing("Several"), needs_you(NeedsYouReason::Matches { count: 3 }));
    assert_eq!(standing("No Tracklist"), needs_you(NeedsYouReason::NoTracklist));
    assert_eq!(
        standing("Medium"),
        needs_you(NeedsYouReason::MediumMismatch { folder: MediumConflict::CdRip, releases: 2 })
    );
    assert_eq!(standing("Not Found"), needs_you(NeedsYouReason::NotFound));
    assert_eq!(standing("Nothing Found"), needs_you(NeedsYouReason::NotFound));
    assert_eq!(standing("Manual"), needs_you(NeedsYouReason::NothingToLookUp));
    assert_eq!(
        standing("Broke"),
        Some(PendingStanding::Error {
            failure: InternalFailure { detail: "bae broke".to_string() }
        })
    );
    assert_eq!(standing("Lookup Failed"), Some(PendingStanding::LookupError));
    assert_eq!(standing("Unread"), Some(PendingStanding::LookupError));
    assert_eq!(standing("Kept"), Some(PendingStanding::Unmatched));
    assert_eq!(standing("Kept Then Linked"), Some(PendingStanding::Identified));
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
            failure: InternalFailure { detail: "disk full".to_string() }
        })
    );
    assert_eq!(
        row_at(&read, "Not Written").1.standing.as_ref().and_then(PendingStanding::badge),
        Some(crate::PendingBadge::Error)
    );

    lib.run("DELETE FROM import_candidate_standing;").await;
    let at_rest = lib.shown(view(TriageTab::Pending)).await;
    for display_path in releases {
        assert_eq!(
            row_at(&at_rest, display_path).1.standing,
            Some(PendingStanding::NeedsYou { reason: NeedsYouReason::NotFound }),
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
    use crate::IdentifyingStep;
    assert_eq!(
        shown("Queued"),
        (Some(PendingStanding::Identifying), Some(IdentifyingStep::Queued))
    );
    assert_eq!(
        shown("Running"),
        (Some(PendingStanding::Identifying), Some(IdentifyingStep::Running))
    );
    assert_eq!(
        shown("Finalizing"),
        (Some(PendingStanding::Identifying), Some(IdentifyingStep::Running))
    );
    assert_eq!(
        shown("Reading"),
        (Some(PendingStanding::Identified), None),
        "a pick being read is Identified, and shows no lookup at work"
    );
    assert_eq!(
        entry_counts(&read.pending_filter_entries),
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
    let releases = ["Queued", "Running", "Finalizing", "Reading", "Asked", "Idle"];
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
        assert_eq!(row_at(&read, display_path).1.reading_pick(), reading, "{display_path}");
    }
}

/// Each candidate row of one read, by key, with where it stands on Found.
fn standings(
    projection: &crate::ImportListProjection,
) -> Vec<(crate::CandidateKey, Option<crate::PendingStanding>)> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, live, .. } => {
                Some((row.candidate_key.clone(), live.standing.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A sole release whose document could not be read is not picked for the
/// folder, and the stored rows the list reads say so: the lookup failed to
/// read it, which a retry answers.
#[tokio::test]
async fn a_sole_release_with_an_unread_document_reads_back_as_a_lookup_error() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    let mut unread = verdict("mb-verdict", None);
    let TerminalVerdict::Found { findings, .. } = &mut unread else {
        unreachable!("verdict builds a found verdict");
    };
    findings.matches[0].document_failure = Some(bae_catalogs::LookupFailure::Network);
    store_verdict(&db, &candidate, unread).await;

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(
        standings(&projection),
        vec![(candidate.key(), Some(crate::PendingStanding::LookupError))]
    );
}

/// A run that ended because bae broke reads back as an error the row states
/// with its text.
#[tokio::test]
async fn an_error_verdict_reads_back_as_an_error_with_its_text() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    let failure = bae_catalogs::InternalFailure {
        detail: "checking the library: the store is locked".to_string(),
    };
    store_verdict(&db, &candidate, TerminalVerdict::Error { failure: failure.clone() }).await;

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(
        standings(&projection),
        vec![(candidate.key(), Some(crate::PendingStanding::Error { failure }))]
    );
}

/// A stored import failure keeps the row on Pending, saying what went wrong.
#[tokio::test]
async fn a_stored_failure_keeps_the_row_pending_saying_why() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    let hash = candidate.files.content_hash();

    async fn tab(tables: &ImportTables, tab: TriageTab) -> Vec<crate::TriageRow> {
        rows(&tables.load_import_list_read(request(tab).await).await.unwrap())
    }

    let pending = tab(&tables, TriageTab::Pending).await;
    assert_eq!(pending.len(), 1, "before any attempt the row is pending");
    assert!(pending[0].import_status.is_none());

    tables
        .save_import_candidate_failure(
            &hash,
            0,
            &crate::ImportFailure::error_only("the disk filled", fixed_now()),
        )
        .await
        .unwrap();

    assert_eq!(
        tables.load_import_list_read(request(TriageTab::Done).await).await.unwrap().total_count,
        0,
        "a failed attempt imported nothing, so nothing is done"
    );
    let failed = tab(&tables, TriageTab::Pending).await;
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].placement, crate::TriagePlacement::Failed);
    assert_eq!(
        failed[0].import_status,
        Some(crate::TriageImportStatus::Error {
            failure: crate::ImportFailureReason::error("the disk filled")
        }),
        "with no runtime entry of its own"
    );
}
