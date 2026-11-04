//! What each event does to the queue: a person's decision about a candidate
//! (a pick, a clear, a skip, an import, changed lookup choices) ends its run
//! and takes it off the queue, a removed candidate leaves it, and a found
//! release goes on it.

use super::*;
use crate::extraction::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::identification::tests::*;
use bae_catalogs::test_releases::{mb_release_json, mb_search_json};
use bae_util::test_channels::HANG_GUARD;
use std::path::Path;
use std::sync::Arc;

/// An analyzer held at each image behind a [`crate::tests::gate::Gate`].
struct GatedAnalyzer(crate::tests::gate::Held);

impl ArtworkAnalyzer for GatedAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.0.pass();
        ArtworkAnalysis::empty()
    }
}

/// A pick names one candidate, so it ends one candidate's run. The pass keeps
/// answering everything else it had going.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_ends_only_the_picked_candidates_run() {
    let fixture = Fixture::new("pick-ends-one-run").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    let picked = fixture.disc_id_candidate("Picked");
    let other = fixture.barcode_candidate("Other");
    let probed = fixture.probed_total_ms(&other);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.route("/release?", 200, mb_search_json("mb-other-1", "rg-other-1"));
    fixture.provider.route(
        "/release/mb-other-1?",
        200,
        mb_release_json("mb-other-1", "rg-other-1", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    let picked_key = crate::CandidateKey::folder(&picked);
    let other_key = crate::CandidateKey::folder(&other);
    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;
    let mut events = fixture.import.every_event();

    fixture.import.select_candidate_file_tags(picked_key.clone()).await.expect("the pick lands");

    assert!(
        !fixture.import.is_identifying(&picked_key),
        "the pick ends the picked candidate's run before it returns"
    );
    // The other candidate is carried to its verdict. The picked one left the
    // queue with the run the pick ended.
    tokio::time::timeout(HANG_GUARD, fixture.await_identified_row(&other))
        .await
        .expect("the pass answers the candidate nobody picked");

    let picked_row = fixture.stored_for(&picked).await.expect("the pick is stored");
    assert_eq!(picked_row.metadata_provenance, Some(crate::MetadataProvenance::FileMetadata));
    assert!(picked_row.identify.is_none(), "a cancelled run writes no verdict");
    assert!(
        fixture.identified_for(&other).await.is_some(),
        "the candidate nobody picked reached its verdict"
    );
    let cancelled_other = drain_events(&mut events).into_iter().any(|event| {
        matches!(
            event,
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state: IdentifyState::Idle,
                ..
            } if candidate_key == other_key
        )
    });
    assert!(
        !cancelled_other,
        "the pick must not tear down the run of a candidate it does not name"
    );

    fixture.identification().shut_down();
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass).await.expect("the pass ends").unwrap();
}

/// Skipping is a decision about the candidate, so it ends its run. Unskipping
/// brings it back as it was — unqueued — and a person asking answers it.
#[tokio::test(flavor = "multi_thread")]
async fn skipping_a_candidate_ends_its_run_and_unskipping_queues_nothing() {
    let fixture = Fixture::new("skip-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    fixture.provider.wait_for_request("/discid/", 1).await;

    fixture
        .import
        .set_candidate_skipped(key.clone(), true)
        .await
        .expect("the candidate is skipped");

    assert!(!fixture.import.is_identifying(&key), "skipping ends the run before it returns");
    fixture.provider.release();
    assert!(fixture.identified_for(&dir).await.is_none(), "a cancelled run writes no verdict");

    fixture
        .import
        .set_candidate_skipped(key.clone(), false)
        .await
        .expect("the candidate is unskipped");
    fixture.drain_automatic().await;
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "unskipping queues nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
}

/// Clearing a candidate's metadata is a decision too: the run answering the
/// candidate as it was ends, and the change is announced so the pane reads
/// the candidate afresh.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_a_candidates_metadata_ends_its_run_and_announces_the_change() {
    let fixture = Fixture::new("clear-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    let mut events = fixture.import.every_event();
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    fixture.provider.wait_for_request("/discid/", 1).await;

    fixture.import.clear_candidate_metadata(key.clone()).await.expect("the metadata is cleared");

    assert!(!fixture.import.is_identifying(&key), "clearing ends the run before it returns");
    // The announce is sent inside the same write, so it is on the bus by the
    // time the command has returned.
    let announced = drain_events(&mut events).into_iter().any(|event| {
        matches!(
            event,
            ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key })
                if candidate_key == key
        )
    });
    assert!(announced, "the clear announces the candidate's change");
    fixture.provider.release();

    assert!(fixture.identified_for(&dir).await.is_none(), "a cancelled run writes no verdict");
}

/// A clear ends the identification the automatic admission had going, and
/// the candidate leaves the queue with it: nothing puts it back on its own,
/// and a person asking answers it.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_a_candidates_metadata_takes_it_off_the_queue() {
    let fixture = Fixture::new("clear-requeues").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-cleared", "rg-cleared", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-cleared?",
        200,
        mb_release_json("mb-cleared", "rg-cleared", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;

    fixture.import.clear_candidate_metadata(key.clone()).await.expect("the metadata is cleared");

    assert!(!fixture.import.is_identifying(&key), "the clear ends the run it found");
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the candidate left the queue, so the automatic admission holds nothing")
        .unwrap();
    fixture.provider.release();
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "nothing asked about the cleared candidate again: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;
}

/// An import claims the candidate, so nothing is left for identification to
/// answer. The run ends at the command that claimed it, and the import goes on.
#[tokio::test(flavor = "multi_thread")]
async fn starting_an_import_ends_the_candidates_run() {
    let fixture = Fixture::new("import-ends-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    fixture.provider.wait_for_request("/discid/", 1).await;

    let import_id = fixture
        .import
        .start_import(&key)
        .await
        .expect("the prepared candidate enters its import")
        .expect_queued();

    assert!(
        !fixture.import.is_identifying(&key),
        "starting the import ends the run before it returns"
    );
    assert!(!import_id.is_empty(), "the import was queued");
    fixture.provider.release();
    assert!(fixture.identified_for(&dir).await.is_none(), "a cancelled run writes no verdict");
}

/// A person's decision ends their own run like any other: it stops at `Idle`
/// and stores nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_during_an_explicit_lookup_stores_no_verdict() {
    let fixture = Fixture::new("pick-ends-explicit-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    let mut events = fixture.import.every_event();
    fixture.start_explicit_lookup_and_await_run(&dir).await;
    fixture.provider.wait_for_request("/discid/", 1).await;

    fixture.import.select_candidate_file_tags(key.clone()).await.expect("the pick lands");

    assert!(
        !fixture.import.is_identifying(&key),
        "the pick ends the person's own run before it returns"
    );
    await_run_state(&mut events, &key, |_, state| matches!(state, IdentifyState::Idle)).await;
    fixture.provider.release();

    let stored = fixture.stored_for(&dir).await.expect("the pick is stored");
    assert_eq!(stored.metadata_provenance, Some(crate::MetadataProvenance::FileMetadata));
    assert!(stored.identify.is_none(), "the watcher on a cancelled run writes nothing");
}

/// Changing what a candidate's identification asks about supersedes the run
/// that was going: the automatic run ends on `Idle`, a new run answers under a
/// new id, and the queue gives up the slot instead of waiting on a run that is
/// never coming back.
#[tokio::test(flavor = "multi_thread")]
async fn changing_the_choices_supersedes_the_run_and_frees_its_slot() {
    let fixture = Fixture::new("choices-supersede-run").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-super-1", "rg-super-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.route(
        "/release/mb-super-1?",
        200,
        mb_release_json("mb-super-1", "rg-super-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();
    let mut restart = fixture.import.every_event();

    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;
    let sweeps_run = await_run_state(&mut restart, &key, |_, _| true).await;

    // What a person changing a choice does: the change is made to the stored
    // choices, and the run that reads them is started.
    fixture
        .import
        .edit_candidate_lookup_choices(
            &key,
            bae_identify::LookupChoiceEdit::ToggleBarcode { code: "0123456789012".to_string() },
        )
        .await
        .unwrap();
    fixture.identification().rerun_identify(key.clone());
    await_run_state(&mut restart, &key, |run, _| run != sweeps_run).await;
    fixture.provider.release();

    // The pass returns rather than waiting forever on the run it lost.
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the pass frees the slot its superseded run held")
        .unwrap();

    fixture.await_identified_row(&dir).await;
    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the queue lets go of the answered job");
    let runs: Vec<(IdentifyRunId, IdentifyState)> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged { candidate_key, run, state, .. }
                if candidate_key == key =>
            {
                Some((run, state))
            }
            _ => None,
        })
        .collect();
    let superseded = sweeps_run;
    assert!(
        runs.iter().any(|(run, state)| *run == superseded && matches!(state, IdentifyState::Idle)),
        "the superseded run ends on Idle: {runs:?}"
    );
    assert!(
        runs.iter().any(|(run, state)| *run != superseded && state.is_terminal()),
        "a new run answers under a new id: {runs:?}"
    );
    assert_eq!(
        fixture.import.standing_facts_for_test(&key).await.identification,
        None,
        "the queue holds nothing for a candidate whose answer has landed"
    );
}

/// An import started on a queued candidate takes it off the queue: it gets no
/// result, and the progress count treats its identification as ended.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_start_takes_a_queued_candidate_out_of_work_and_progress() {
    let fixture = Fixture::new("import-mid-pass").await;
    let remaining = fixture.disc_id_candidate("Remaining");
    let importing = fixture.disc_id_candidate("Importing");
    std::fs::write(importing.join("notes.txt"), "distinct candidate").unwrap();
    let importing_hash = fixture.content_hash(&importing);
    let probed = fixture.probed_total_ms(&remaining);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-import-progress", "rg-import-progress", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-import-progress?",
        200,
        mb_release_json("mb-import-progress", "rg-import-progress", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.provider.hold("/discid/");

    let mut counts = fixture.import.every_identification_count_for_test();
    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;
    start_import_for(&fixture, &importing).await;
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("pass finishes after import ownership changes")
        .unwrap();

    let importing_state = fixture
        .stored()
        .await
        .remove(&importing_hash)
        .expect("the discovered draft remains available to the import");
    assert!(importing_state.identify.is_none());
    assert!(fixture.identified_for(&remaining).await.is_some());
    let progress: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
    assert!(
        progress.contains(&(1, 2)),
        "the candidate the import took is no longer waited on: {progress:?}"
    );
    assert_eq!(
        progress.last(),
        Some(&(0, 0)),
        "and the batch is over once the other one has its answer: {progress:?}"
    );
}

/// A candidate removed while it is being identified gives up its queue slot
/// and stores nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_removed_mid_flight_does_not_wedge_the_queue() {
    let fixture = Fixture::new("removed-mid-flight").await;
    let (gate, held, entered) = crate::tests::gate::closed();
    fixture.import.register_artwork_analyzer(Arc::new(GatedAnalyzer(held)));
    let dir = fixture.barcode_candidate("Vanishing");
    let hash = fixture.content_hash(&dir);
    fixture.scan(1).await;

    // Start the pass and hold extraction inside OCR, so the candidate is
    // genuinely mid-flight when the folder goes.
    let pass = fixture.drain_automatic_task();
    tokio::task::spawn_blocking(move || entered.recv_timeout(HANG_GUARD))
        .await
        .unwrap()
        .expect("extraction reaches the artwork");
    std::fs::remove_dir_all(&dir).unwrap();
    // What the folder watcher does when a candidate's directory goes: read
    // the root again, which removes the candidate that is no longer there.
    fixture.rescan(&fixture.import, 0).await;
    gate.open();

    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the pass must finish rather than wait on a candidate that is gone")
        .unwrap();

    let state = fixture
        .stored()
        .await
        .remove(&hash)
        .expect("the discovered draft remains keyed by the candidate's content");
    assert!(
        state.identify.is_none(),
        "a candidate that vanished mid-identification learned nothing"
    );
    // The queue still answers a later drain.
    fixture.drain_automatic().await;
}

/// A found release reaches the queue in the order its events were sent: a
/// removal of its key announced before it does not take it off the queue.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_found_after_its_key_was_removed_is_queued() {
    let fixture = Fixture::new("found-after-removal").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    fixture.set_identify_automatically(true).await;
    let candidate = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate is answerable");

    fixture
        .import
        .emit_event_for_test(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key }));
    fixture.import.emit_event_for_test(ImportEvent::Scan(ScanEvent::FolderCandidate {
        candidate,
        found_while_automatic: true,
    }));
    fixture.drain_automatic().await;

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the release found after the removal is identified: {:?}",
        fixture.provider.requests()
    );
}

/// A removal announced after a release was found takes it off the queue,
/// however many events the queue reads at once.
#[tokio::test(flavor = "multi_thread")]
async fn a_found_release_whose_key_is_then_removed_is_not_identified() {
    let fixture = Fixture::new("removed-after-found").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    let candidate = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate is answerable");

    fixture.import.emit_event_for_test(ImportEvent::Scan(ScanEvent::FolderCandidate {
        candidate,
        found_while_automatic: true,
    }));
    fixture
        .import
        .emit_event_for_test(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key: key }));
    fixture.drain_automatic().await;
    fixture.provider.release();

    assert!(fixture.identified_for(&dir).await.is_none());
}
