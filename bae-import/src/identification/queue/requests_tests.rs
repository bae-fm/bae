//! What a person's request does to the queue: a run of its own, ahead of
//! every waiting automatic job and at interactive priority, that supersedes a
//! run going, and runs while automatic identification is off.

use super::*;
use crate::identification::tests::*;
use crate::tables::NewImportCandidateVerdict;
use bae_catalogs::test_releases::mb_release_json;
use bae_util::test_channels::HANG_GUARD;
use std::sync::Arc;

/// The priority the next run of `key` reports.
async fn await_run_priority(
    events: &mut crate::ImportEvents,
    key: &crate::CandidateKey,
) -> CallPriority {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if let ImportEvent::IdentifyStateChanged { candidate_key, priority, .. } =
                events.recv().await.expect("the import event bus stays open")
            {
                if candidate_key == key.clone() {
                    return priority;
                }
            }
        }
    })
    .await
    .expect("a run of the candidate reports a state")
}

/// The failing response is a 400 rather than a 5xx so the client's own retry
/// policy stays out of it; what is under test is what identification does with
/// a failure, not how many times the client repeats one.
/// A verdict is where its run ends, and a re-run afterwards is a run of its
/// own: its own id, its own inputs, its own answer. The run it replaces says
/// nothing further, so nothing a later watcher hears can be mistaken for the
/// re-run's answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_rerun_after_a_verdict_is_a_run_of_its_own() {
    let fixture = Fixture::new("rerun-is-its-own-run").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    let mut events = fixture.import.every_event();
    fixture.drain_automatic().await;
    assert!(matches!(
        fixture.identified_for(&dir).await.map(|row| row.verdict),
        Some(TerminalVerdict::Failed { .. })
    ));
    assert!(
        !fixture.import.is_identifying(&key),
        "the failed run ended at its verdict rather than parking on its inbox"
    );
    let failed_run = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged { candidate_key, run, .. }
                if candidate_key == key =>
            {
                Some(run)
            }
            _ => None,
        })
        .next_back()
        .expect("the failed run reported its states");

    fixture.provider.set_routes(vec![
        ("/discid/", 200, discid_json("mb-retry-2", "rg-retry-2", &[probed, 0])),
        ("/release/mb-retry-2?", 200, mb_release_json("mb-retry-2", "rg-retry-2", &[probed, 0])),
    ]);
    fixture.identification().rerun_identify(key.clone());
    fixture
        .await_state(&dir, |row| {
            matches!(
                row.identify.as_ref().map(|result| &result.verdict),
                Some(TerminalVerdict::Found { .. })
            )
        })
        .await;
    let runs: Vec<IdentifyRunId> = drain_events(&mut events)
        .into_iter()
        .filter_map(|event| match event {
            ImportEvent::IdentifyStateChanged { candidate_key, run, .. }
                if candidate_key == key =>
            {
                Some(run)
            }
            _ => None,
        })
        .collect();
    assert!(
        !runs.is_empty() && runs.iter().all(|run| *run != failed_run),
        "the run that already answered reported nothing further: {runs:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn interactive_lookup_runs_while_automatic_lookup_is_off() {
    let fixture = Fixture::new("interactive-with-automatic-off").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-interactive-off", "rg-interactive-off", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-interactive-off?",
        200,
        mb_release_json("mb-interactive-off", "rg-interactive-off", &[probed, 0]),
    );
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(HANG_GUARD, fixture.await_identified_row(&dir))
        .await
        .expect("interactive lookup stores its verdict");
    assert!(fixture.provider.count_containing("/discid/") > 0);
}

/// A run a person asked for stores its verdict for a candidate whose draft is
/// already filled, and leaves nothing pending on the key once it has.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_stores_its_verdict_for_a_pre_filled_candidate() {
    let fixture = Fixture::new("explicit-with-pre-filled-draft").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-default-none", "rg-default-none", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-default-none?",
        200,
        mb_release_json("mb-default-none", "rg-default-none", &[probed, 0]),
    );
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);

    tokio::time::timeout(HANG_GUARD, fixture.await_identified_row(&dir))
        .await
        .expect("an explicit lookup stores its verdict over a pre-filled draft");
    fixture.await_not_standing(&[key]).await;
}

/// Asking to identify an answered candidate runs it again: the person asked
/// for a run, and a stored result is what they are asking to replace. Only a
/// release found on its own treats a result as a reason not to run.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_for_an_answered_candidate_runs_it_again() {
    let fixture = Fixture::new("resume-answered").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-asked-again", "rg-asked-again", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-asked-again?",
        200,
        mb_release_json("mb-asked-again", "rg-asked-again", &[probed, 0]),
    );
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    let verdict = multi_match_verdict(&["mb-resume-1", "mb-resume-2"], "rg-resume-1");
    let wrote = fixture
        .import
        .save_candidate_verdict_if_current(
            &crate::CandidateKey::folder(&dir),
            IdentifyRunId::for_test(1),
            &NewImportCandidateVerdict {
                content_hash: fixture.content_hash(&dir),
                file_edit_revision: 0,
                candidate_key: crate::CandidateKey::folder(&dir),
                verdict,
                signals: settled_signals(),
                pick: None,
                releases: Vec::new(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(wrote, crate::VerdictWrite::Stored { .. }), "the seeded verdict lands");

    fixture.start_explicit_lookup(&dir);

    fixture
        .await_state(&dir, |row| {
            row.identify.as_ref().is_some_and(|result| {
                matches!(
                    &result.verdict,
                    TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. }
                        if matches.iter().any(|m| m.release_id == "mb-asked-again")
                )
            })
        })
        .await;
    assert!(fixture.provider.count_containing("/discid/") > 0);
}

/// Asking again while a run is going supersedes it: the person asked for a
/// run that reads the candidate as it is now, so the one in flight is
/// cancelled before the new one starts and its result can never land.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_during_an_active_run_supersedes_it() {
    let fixture = Fixture::new("explicit-supersedes-active-run").await;
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
    let first_run = loop {
        let event = tokio::time::timeout(HANG_GUARD, events.recv())
            .await
            .expect("the active run reports its state")
            .expect("the import event bus remains open");
        if let ImportEvent::IdentifyStateChanged { candidate_key, run, .. } = event {
            if candidate_key == key {
                break run;
            }
        }
    };

    // The person presses again while the first run is still at the provider.
    fixture.start_explicit_lookup(&dir);

    let replacement = tokio::time::timeout(HANG_GUARD, async {
        loop {
            if let ImportEvent::IdentifyStateChanged { candidate_key, run, .. } =
                events.recv().await.expect("the import event bus remains open")
            {
                if candidate_key == key && run != first_run {
                    return run;
                }
            }
        }
    })
    .await;
    fixture.provider.release();

    let replacement = replacement.expect("pressing again starts a run of its own");
    assert_ne!(replacement, first_run, "the run in flight was superseded rather than joined");
    assert!(
        fixture.import.is_identifying(&key),
        "and the candidate is identifying under the new run"
    );
    // The superseded run reached the provider and was cancelled there; nothing
    // it would have concluded is stored.
    assert!(fixture.identified_for(&dir).await.is_none());
}

/// Re-run on a candidate whose driver is gone starts a fresh interactive run
/// instead of no-op'ing — the stored answer is what a re-run exists to
/// replace, so it is not consulted.
#[tokio::test(flavor = "multi_thread")]
async fn a_rerun_with_no_driver_runs_identification_again() {
    let fixture = Fixture::new("rerun-no-driver").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    fixture.archive("mb-rerun-1", "rg-rerun-1", &[probed, 0]).await;
    fixture.store_settled_verdict(&dir, "mb-rerun-1", "rg-rerun-1").await;
    fixture.provider.route("/discid/", 200, discid_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-rerun-2?",
        200,
        mb_release_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );

    fixture.identification().rerun_identify(crate::CandidateKey::folder(&dir));

    fixture.provider.wait_for_request("/discid/", 1).await;
}

/// A request on a candidate the automatic admission has waiting moves it to
/// the front of the queue as the same entry, and it takes the next slot at
/// interactive priority — it does not open a fifth slot: the cap bounds local
/// work whoever asked for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_on_a_waiting_candidate_takes_the_next_slot() {
    let fixture = Fixture::new("request-upgrades-waiting").await;
    fixture.import.register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let mut dirs = Vec::new();
    for index in 0..MAX_IN_FLIGHT + 1 {
        let dir = fixture.barcode_candidate(&format!("Album {index}"));
        std::fs::write(dir.join(format!("playlist-{index}.m3u")), format!("{index}")).unwrap();
        dirs.push(dir);
    }
    fixture.provider.route("/release?", 200, "{}");
    fixture.provider.hold("/release?");
    fixture.scan(MAX_IN_FLIGHT + 1).await;

    let sweep = fixture.drain_automatic_task();
    // The cap's worth of lookups are out, and one candidate is still waiting.
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;
    let waiting =
        fixture.first_queued(&dirs).await.expect("one candidate is over the cap and waiting");
    let waiting_key = crate::CandidateKey::folder(&waiting);
    // Read from here, so the first state the waiting candidate reports is the
    // requested run's.
    let mut events = fixture.import.every_event();

    fixture.start_explicit_lookup(&waiting);

    // Still waiting: every slot is held, and a request is not a fifth one.
    tokio::time::timeout(HANG_GUARD, fixture.identification().commands_taken_for_test())
        .await
        .expect("the queue takes the request");
    assert_eq!(
        fixture.identification_status(&waiting_key).await,
        Some(crate::IdentificationStatus::Queued),
        "the request waits for a slot like any other job"
    );
    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "no lookup went out past the cap: {:?}",
        fixture.provider.requests()
    );

    fixture.provider.release();
    let priority = await_run_priority(&mut events, &waiting_key).await;
    assert_eq!(
        priority,
        CallPriority::Interactive,
        "the requested candidate takes the next slot at the priority a person \
         waiting on it deserves"
    );
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT + 1).await;
    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT + 1,
        "one lookup for it, not a second entry's as well: {:?}",
        fixture.provider.requests()
    );

    fixture.identification().shut_down();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("the sweep ends once identification shuts down")
        .expect("the sweep does not panic");
}

/// A request that supersedes an automatic run keeps the rest of the identity
/// group waiting on the answer it will store, rather than dropping them: they
/// are the same bytes, and one answer settles all of them.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_superseding_a_run_keeps_the_rest_of_its_group() {
    let fixture = Fixture::new("request-keeps-group").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    assert_eq!(
        fixture.content_hash(&first),
        fixture.content_hash(&second),
        "the two folders hold the same bytes"
    );
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route("/discid/", 200, discid_json("mb-group-1", "rg-group-1", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-group-1?",
        200,
        mb_release_json("mb-group-1", "rg-group-1", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;
    // Whichever of them the automatic admission is running, the person asks
    // about the other one.
    let running = if fixture.import.is_identifying(&crate::CandidateKey::folder(&first)) {
        first.clone()
    } else {
        second.clone()
    };
    let other = if running == first { second } else { first };

    fixture.start_explicit_lookup(&other);

    fixture
        .await_identification_status(
            &crate::CandidateKey::folder(&other),
            crate::IdentificationStatus::Running,
        )
        .await;
    assert_eq!(
        fixture.identification_status(&crate::CandidateKey::folder(&running)).await,
        Some(crate::IdentificationStatus::Queued),
        "the candidate whose run was superseded waits on the answer this one \
         stores rather than being dropped"
    );

    fixture.provider.release();
    fixture.await_identified_row(&running).await;
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("the automatic admission has nothing left outstanding")
        .unwrap();
    fixture
        .await_not_standing(&[
            crate::CandidateKey::folder(&running),
            crate::CandidateKey::folder(&other),
        ])
        .await;
}

/// A run a person asked for is counted like any other: it opens a batch of its
/// own, and the batch is over when its answer lands.
#[tokio::test(flavor = "multi_thread")]
async fn a_requested_run_is_counted() {
    let fixture = Fixture::new("requested-is-counted").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-counted", "rg-counted", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-counted?",
        200,
        mb_release_json("mb-counted", "rg-counted", &[probed, 0]),
    );
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    let mut counts = fixture.import.every_identification_count_for_test();
    fixture.start_explicit_lookup(&dir);
    fixture.await_identified_row(&dir).await;

    let progress = tokio::time::timeout(HANG_GUARD, async {
        let mut progress = Vec::new();
        loop {
            progress.push(counts.recv().await.expect("the count stays open"));
            if progress.last() == Some(&(0, 0)) {
                return progress;
            }
        }
    })
    .await
    .expect("the batch a request opened drains");
    assert_eq!(progress.first(), Some(&(0, 1)), "the request opens a batch of one: {progress:?}");
}
