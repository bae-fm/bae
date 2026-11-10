//! The queue's order and slots: a planned candidate queued before its run
//! reports, one run for candidates that share files, a typed search ahead
//! of automatic runs, progress counts, what turning automatic
//! identification off leaves going, and a person cancelling identification.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::identification::tests::*;
use bae_catalogs::test_releases::{mb_release_json, mb_search_json};
use bae_util::test_channels::HANG_GUARD;
use std::path::PathBuf;
use std::sync::Arc;

/// Nothing is identifying `key`: its queue mark, its run and its answer are
/// all gone. A cancel's reply comes after it clears them, so a test reads
/// them once the cancel returned.
async fn assert_not_identifying(fixture: &Fixture, key: &crate::CandidateKey) {
    assert_eq!(fixture.identification_status(key).await, None, "{key} still stands");
    assert!(!fixture.import.is_identifying(key), "{key} is still identifying");
}

/// Folders over the cap, each asking the provider a distinct question, with
/// the lookups held so the cap's worth run and the rest wait.
async fn flooded_queue(fixture: &Fixture) -> Vec<PathBuf> {
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
    dirs
}

#[tokio::test(flavor = "multi_thread")]
async fn a_planned_candidate_is_queued_before_its_driver_reports() {
    let fixture = Fixture::new("queued-before-driver").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut changes = fixture.import.every_runtime_change_for_test();

    let pass = fixture.drain_automatic_task();

    let change = tokio::time::timeout(HANG_GUARD, changes.recv())
        .await
        .expect("the queue state is published before identification")
        .expect("candidate runtime remains open");
    let crate::CandidateRuntimeChange::Updated { key: changed, runtime } = change else {
        panic!("the queue writes its admission before starting drivers");
    };
    assert_eq!(changed, key);
    assert_eq!(
        runtime.identification,
        Some(crate::IdentificationRuntime::Queued(crate::Admission::Automatic))
    );

    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the pass finishes after the provider resumes")
        .unwrap();
}

/// Candidates hashing the same share one job, and one of them runs it. The
/// rest are not told that run's states — each key is its own run, and what
/// they are waiting for is the answer this one stores, which covers them. So
/// they stay queued until it does.
#[tokio::test(flavor = "multi_thread")]
async fn a_shared_identify_job_runs_one_member_and_leaves_the_rest_queued() {
    let fixture = Fixture::new("shared-job-status").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    let keys = [crate::CandidateKey::folder(&first), crate::CandidateKey::folder(&second)];
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    let pass = fixture.drain_automatic_task();

    // One member runs the shared job and the other waits on its answer.
    let status_of = |facts: &std::collections::HashMap<_, crate::TriageRuntimeFacts>,
                     wanted: crate::IdentificationStatus| {
        keys.iter()
            .filter(|key| {
                facts.get(*key).is_some_and(|facts| facts.identification.as_ref() == Some(&wanted))
            })
            .count()
    };
    fixture
        .await_facts(|facts| {
            status_of(facts, crate::IdentificationStatus::Running) == 1
                && status_of(facts, crate::IdentificationStatus::Queued) == 1
        })
        .await;

    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the pass finishes after the provider resumes")
        .unwrap();
}

/// With automatic identification's background lookups queued on the shared
/// limiter, a search the user typed is admitted next rather than after all of
/// them. Each of eight candidates has its own barcode, so each queues a lookup.
///
/// Order is read off the limiter's own admission record, with the waits
/// between requests held until the search is typed: it must go ahead of every
/// background ticket already waiting when it arrived. A single first-come
/// queue admits every one of them first.
#[tokio::test(flavor = "multi_thread")]
async fn a_typed_search_is_not_delayed_by_automatic_identification() {
    let fixture = Fixture::new("interactive-not-delayed").await;
    fixture.import.register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let mut dirs = Vec::new();
    for i in 0..8 {
        let dir = fixture.barcode_candidate(&format!("Album {i}"));
        std::fs::write(dir.join(format!("playlist-{i}.m3u")), format!("candidate {i}")).unwrap();
        dirs.push(dir);
    }
    let probed = fixture.probed_total_ms(&dirs[0]);
    // Added first, so the typed-search route below catches only what the
    // barcode lookups leave.
    fixture.provider.route(
        "/release?query=barcode",
        200,
        mb_search_json("mb-flood-0", "rg-flood-0"),
    );
    fixture.provider.route(
        "/release/mb-flood-0?",
        200,
        mb_release_json("mb-flood-0", "rg-flood-0", &[probed, 0]),
    );
    fixture.provider.route("/release?", 200, mb_search_json("mb-typed", "rg-typed"));
    fixture.scan(8).await;

    fixture.hold_request_waits();
    let sweep = fixture.drain_automatic_task();

    let musicbrainz = &fixture.musicbrainz;
    let waiting = tokio::time::timeout(
        HANG_GUARD,
        musicbrainz.queued_requests_reaching(CallPriority::Background, 2),
    )
    .await
    .expect("automatic identification queues background lookups on the limiter");

    let session = fixture.parts.catalogs().musicbrainz();
    let typing = session.search_musicbrainz(
        bae_catalogs::musicbrainz::ReleaseSearchParams {
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            ..Default::default()
        },
        CallClass::AWAITED,
    );
    let passing = async {
        tokio::time::timeout(
            HANG_GUARD,
            musicbrainz.queued_requests_reaching(CallPriority::Awaited, 1),
        )
        .await
        .expect("the typed search waits its turn");
        fixture.pass_armed_request_wait().await;
    };
    let (typed, ()) = tokio::join!(typing, passing);
    let typed = typed.expect("the typed search succeeds");
    let admitted = musicbrainz.admitted_requests();

    fixture.release_request_waits();
    fixture.identification().shut_down();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("the sweep ends once identification shuts down")
        .expect("the sweep does not panic");

    assert_eq!(typed.len(), 1);
    let typed_at = admitted
        .iter()
        .position(|(priority, _)| *priority == CallPriority::Awaited)
        .expect("the typed search queued behind background lookups");
    let ahead_of_typed = &admitted[..typed_at];
    let overtaken = waiting
        .iter()
        .filter(|ticket| !ahead_of_typed.contains(&(CallPriority::Background, **ticket)))
        .count();
    assert_eq!(
        overtaken,
        waiting.len(),
        "a background lookup waiting when the search was typed ({waiting:?}) \
         was admitted before it: {admitted:?}"
    );
}

/// A candidate a person opens in a pane is started ahead of the requests
/// waiting before it, and its run's lookups are admitted ahead of requested
/// calls already waiting on the limiter.
#[tokio::test(flavor = "multi_thread")]
async fn an_open_candidate_runs_first_and_its_lookups_go_ahead() {
    let fixture = Fixture::new("open-candidate-first").await;
    fixture.import.register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let keys: Vec<crate::CandidateKey> = (0..MAX_IN_FLIGHT + 2)
        .map(|index| {
            let dir = fixture.barcode_candidate(&format!("Album {index}"));
            std::fs::write(dir.join(format!("notes-{index}.txt")), format!("{index}")).unwrap();
            crate::CandidateKey::folder(&dir)
        })
        .collect();
    fixture.provider.route("/release?", 200, r#"{"releases":[]}"#);
    fixture.provider.hold("query=barcode");
    fixture.set_identify_automatically(false).await;
    fixture.scan(MAX_IN_FLIGHT + 2).await;
    // Asked for in order: the cap's worth run, and the last two wait, the
    // second behind the first.
    for key in &keys {
        fixture.identification().rerun_identify(key.clone());
    }
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;
    let (first_waiting, opened) = (&keys[MAX_IN_FLIGHT], &keys[MAX_IN_FLIGHT + 1]);

    // Requested calls queue on the limiter behind the lookups already sent,
    // their waits held until the open candidate's lookup has queued.
    fixture.hold_request_waits();
    let musicbrainz = &fixture.musicbrainz;
    for index in 0..4 {
        let session = fixture.parts.catalogs().musicbrainz();
        tokio::spawn(async move {
            let params = bae_catalogs::musicbrainz::ReleaseSearchParams {
                album: Some(format!("Asked {index}")),
                ..Default::default()
            };
            session.search_musicbrainz(params, CallClass::REQUESTED).await
        });
    }
    let requested = tokio::time::timeout(
        HANG_GUARD,
        musicbrainz.queued_requests_reaching(CallPriority::Requested, 3),
    )
    .await
    .expect("the requested searches queue on the limiter");

    let mut shown = fixture.identification().shown_candidate();
    shown.show(Some(opened.clone()));
    fixture.identification().cancel(vec![keys[0].clone()]).await.unwrap();
    tokio::time::timeout(HANG_GUARD, fixture.identification().commands_taken_for_test())
        .await
        .expect("the queue takes the pane's candidate");

    // The open candidate takes the freed slot: its run marks it running once
    // it starts, after the queue has started it.
    fixture.await_identification_status(opened, crate::IdentificationStatus::Running).await;
    assert_eq!(
        fixture.identification_status(first_waiting).await,
        Some(crate::IdentificationStatus::Queued),
        "the request ahead of it keeps waiting"
    );
    let opened_barcode =
        bae_types::barcode::with_check_digit(&format!("01234567{:04}", MAX_IN_FLIGHT + 1));
    tokio::time::timeout(
        HANG_GUARD,
        musicbrainz.queued_requests_reaching(CallPriority::Awaited, 1),
    )
    .await
    .expect("the open candidate's lookup waits its turn");
    fixture.pass_armed_request_wait().await;
    fixture.provider.wait_for_request(&opened_barcode, 1).await;
    let admitted = musicbrainz.admitted_requests();
    let opened_at = admitted
        .iter()
        .position(|(priority, _)| *priority == CallPriority::Awaited)
        .expect("the open candidate's lookup queued as awaited");
    let overtaken = requested
        .iter()
        .filter(|ticket| !admitted[..opened_at].contains(&(CallPriority::Requested, **ticket)))
        .count();
    assert_eq!(
        overtaken,
        requested.len(),
        "a requested call waiting when the pane opened ({requested:?}) was admitted \
         before the open candidate's lookup: {admitted:?}"
    );

    fixture.release_request_waits();
    drop(shown);
    fixture.identification().shut_down();
    fixture.provider.release();
}

/// Progress events carry both counts: a batch admitted together opens at its
/// full size and ends at `(0, 0)`. A pass with nothing to identify sends none.
#[tokio::test(flavor = "multi_thread")]
async fn progress_carries_both_counts() {
    let fixture = Fixture::new("progress").await;
    let first = fixture.disc_id_candidate("Album One");
    // A differing file gives the second folder its own content hash and row.
    let second = fixture.disc_id_candidate("Album Two");
    std::fs::write(second.join("notes.txt"), "different bytes").unwrap();
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route("/discid/", 200, discid_json("mb-prog-1", "rg-prog-1", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-prog-1?",
        200,
        mb_release_json("mb-prog-1", "rg-prog-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    let mut counts = fixture.import.every_identification_count_for_test();
    fixture.drain_automatic().await;
    let progress: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
    assert_eq!(
        progress.first(),
        Some(&(0, 2)),
        "planning announces the whole batch before any of it is answered"
    );
    assert!(progress.contains(&(1, 2)), "and every verdict advances the count: {progress:?}");
    assert_eq!(
        progress.last(),
        Some(&(0, 0)),
        "the batch is over once both have their answers: {progress:?}"
    );

    let mut counts = fixture.import.every_identification_count_for_test();
    fixture.drain_automatic().await;
    let replanned: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
    assert!(
        replanned.is_empty(),
        "a pass over an answered queue identifies nothing, so there is no batch: {replanned:?}"
    );
}

/// Candidates hashing the same are one job, and the answer one of them stores
/// answers all of them: the next admission finds both settled and asks nothing.
#[tokio::test(flavor = "multi_thread")]
async fn one_answer_covers_every_candidate_that_hashes_the_same() {
    let fixture = Fixture::new("shared-hash-answered").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    assert_eq!(
        fixture.content_hash(&first),
        fixture.content_hash(&second),
        "the two folders hold the same bytes"
    );
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-shared-1", "rg-shared-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-shared-1?",
        200,
        mb_release_json("mb-shared-1", "rg-shared-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    fixture.drain_automatic().await;

    let asked = fixture.provider.count_containing("/discid/");
    assert_eq!(asked, 1, "one run answered both: {:?}", fixture.provider.requests());
    assert!(fixture.identified_for(&first).await.is_some());
    assert!(fixture.identified_for(&second).await.is_some());

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        asked,
        "and a second admission finds both of them answered"
    );
}

/// A decision already announced when a person asks is heard before the ask:
/// the clear's announcement takes nothing off the queue the request then puts
/// on it, and the run asked for stores its answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_decision_announced_before_a_request_leaves_the_requested_run_going() {
    let fixture = Fixture::new("decision-then-request").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-asked", "rg-asked", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-asked?",
        200,
        mb_release_json("mb-asked", "rg-asked", &[probed, 0]),
    );
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    // The loop is handed both before it first looks: the announcement, then
    // the request, with every later event behind them.
    let (events, bus) = tokio::sync::mpsc::unbounded_channel();
    let mut bus = crate::ImportEvents::of_test_channel(bus);
    events
        .send(ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key: key.clone() }))
        .unwrap();
    let mut later = fixture.import.every_event();
    tokio::spawn(async move {
        while let Some(event) = later.recv().await {
            if events.send(event).is_err() {
                return;
            }
        }
    });
    let (commands, mut asked) = tokio::sync::mpsc::unbounded_channel();
    commands.send(Command::Request { candidate_key: key.clone() }).unwrap();
    let (drained, answered) = tokio::sync::oneshot::channel();
    commands.send(Command::AwaitDrained { drain: Drain::Every, drained }).unwrap();

    let context = fixture.context();
    let token = CancellationToken::new();
    let loop_token = token.clone();
    let queue = tokio::spawn(async move {
        super::run(&context, &loop_token, &mut bus, &mut asked).await;
    });
    tokio::time::timeout(HANG_GUARD, answered)
        .await
        .expect("the requested job ends")
        .expect("the queue says so");

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the run asked for after the clear was announced stored its answer; requests: {:?}",
        fixture.provider.requests()
    );
    token.cancel();
    queue.await.unwrap();
}

/// Switching automatic identification off takes nothing off the queue: a
/// candidate whose answer is already being written keeps its write, so the
/// row lands and nothing is left saying a commit is still pending.
#[tokio::test(flavor = "multi_thread")]
async fn switching_automatic_identification_off_lets_a_settling_write_land() {
    let fixture = Fixture::new("disable-during-settle").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-settling", "rg-settling", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-settling?",
        200,
        mb_release_json("mb-settling", "rg-settling", &[probed, 0]),
    );
    fixture.provider.hold("/release/mb-settling?");
    fixture.scan(1).await;

    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/release/mb-settling?", 1).await;

    fixture.set_identify_automatically(false).await;

    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the pass ends once the write it waited for has landed")
        .unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the answer being written when automatic identification went off still landed"
    );
    let runtime = fixture.import.candidate_runtime(&key).await.unwrap();
    assert!(
        runtime.as_ref().is_none_or(|runtime| runtime.identification.is_none()),
        "nothing is left saying the commit is still pending, nor any failure: {runtime:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn disabling_automatic_lookup_lets_what_it_queued_finish() {
    let fixture = Fixture::new("disable-lets-queued-finish").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-finishes", "rg-finishes", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-finishes?",
        200,
        mb_release_json("mb-finishes", "rg-finishes", &[probed, 0]),
    );
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;

    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;

    // A preference is not a cancel: the run the setting admitted is still
    // running after it turns off, and answers.
    fixture.set_identify_automatically(false).await;
    assert!(fixture.import.is_identifying(&key));
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass)
        .await
        .expect("the queued run finishes after the setting turns off")
        .unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the run the setting admitted stores its answer"
    );
    assert_eq!(fixture.identification_status(&key).await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn disabling_automatic_lookup_preserves_a_settled_result() {
    let fixture = Fixture::new("disable-preserves-settled").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-settled", "rg-settled", &[probed, 0]));
    fixture.provider.route(
        "/release/mb-settled?",
        200,
        mb_release_json("mb-settled", "rg-settled", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    let before = fixture.stored_for(&dir).await.expect("identification stores its settled result");

    fixture.set_identify_automatically(false).await;
    fixture.drain_automatic().await;

    let after = fixture
        .stored_for(&dir)
        .await
        .expect("disabling background work retains settled identification");
    assert_eq!(after, before);
}

/// Switching automatic identification off admits nothing further, and takes
/// nothing back: what is on the queue — asked for or admitted on its own —
/// runs to its answer. A preference is not a cancel.
#[tokio::test(flavor = "multi_thread")]
async fn automatic_off_leaves_the_queue_to_finish() {
    let fixture = Fixture::new("automatic-off-keeps-queue").await;
    let requested = fixture.disc_id_candidate("Requested");
    let automatic = fixture.disc_id_candidate("Automatic");
    std::fs::write(automatic.join("notes.txt"), "distinct candidate").unwrap();
    let requested_key = crate::CandidateKey::folder(&requested);
    let automatic_key = crate::CandidateKey::folder(&automatic);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    fixture.start_explicit_lookup_and_await_run(&requested).await;
    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 2).await;

    fixture.set_identify_automatically(false).await;

    assert!(
        fixture.import.is_identifying(&automatic_key),
        "the candidate the setting admitted keeps running after it turns off"
    );
    assert!(
        fixture.import.is_identifying(&requested_key),
        "the candidate a person asked for is still being answered"
    );
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("the automatic admission's queue finishes what it admitted")
        .unwrap();
    assert!(
        fixture.identified_for(&automatic).await.is_some(),
        "the run the setting admitted stores its answer"
    );
    assert_eq!(fixture.identification_status(&automatic_key).await, None);
}

/// Turning the setting off stops nothing already queued.
#[tokio::test(flavor = "multi_thread")]
async fn turning_automatic_identification_off_finishes_what_is_queued() {
    let fixture = Fixture::new("off-finishes-queue").await;
    let dirs = flooded_queue(&fixture).await;
    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;
    let waiting =
        fixture.first_queued(&dirs).await.expect("one candidate is over the cap and waiting");

    fixture.set_identify_automatically(false).await;
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("the queue finishes what it held")
        .unwrap();

    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT + 1,
        "the waiting candidate was still looked up: {:?}",
        fixture.provider.requests()
    );
    for dir in &dirs {
        assert!(
            fixture.identified_for(dir).await.is_some(),
            "{} is answered{}",
            dir.display(),
            if *dir == waiting {
                ", though it was still waiting when identification was turned off"
            } else {
                ""
            }
        );
    }
}

/// Admitting a candidate opens its pane on Find online — the page its run
/// reports on — whoever admitted it, so a person who clicks into a candidate
/// while it is being identified is on the run rather than on the draft it
/// started from.
#[tokio::test(flavor = "multi_thread")]
async fn an_admitted_candidate_opens_on_find_online() {
    let fixture = Fixture::new("admitted-opens-on-find-online").await;
    let dir = fixture.disc_id_candidate("Candidate");
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    assert_eq!(
        fixture.pane(&dir).await.expect("scanned").draft().unwrap().session.presentation,
        crate::MetadataPresentation::Draft,
        "a scanned candidate nobody has touched opens on its draft"
    );

    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;

    assert_eq!(
        fixture.pane(&dir).await.expect("still scanned").draft().unwrap().session.presentation,
        crate::MetadataPresentation::FindOnline,
        "the admission moved the pane to the page its run reports on"
    );

    fixture.identification().shut_down();
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("the sweep ends once identification shuts down")
        .expect("the sweep does not panic");
}

/// With every slot's run going but one, a run that answers and is reading its
/// pick lets the job waiting behind it start: the read holds no slot.
#[tokio::test(flavor = "multi_thread")]
async fn a_settle_reading_its_pick_holds_no_slot() {
    let fixture = Fixture::new("settle-holds-no-slot").await;
    fixture.set_identify_automatically(false).await;
    fixture.import.register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let settling = fixture.disc_id_candidate("Album 0");
    let settling_key = crate::CandidateKey::folder(&settling);
    fixture.route_disc_id_match(&settling, "mb-slot", "rg-slot", 2);
    let mut running = Vec::new();
    for index in 1..=MAX_IN_FLIGHT {
        let dir = fixture.barcode_candidate(&format!("Album {index}"));
        std::fs::write(dir.join(format!("playlist-{index}.m3u")), format!("{index}")).unwrap();
        running.push(dir);
    }
    fixture.provider.route("/release?", 200, "{}");
    // Both the barcode searches and the settle's read of the pick wait here.
    fixture.provider.hold_each(&["query=barcode", "/release/mb-slot?"]);
    fixture.scan(MAX_IN_FLIGHT + 1).await;

    fixture.start_explicit_lookup(&settling);
    fixture
        .await_identification_status(
            &settling_key,
            crate::IdentificationStatus::ReadingPick { import_asked: false },
        )
        .await;
    for dir in &running {
        fixture.start_explicit_lookup(dir);
    }

    // The settle's read and every slot's lookup are out at once.
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;
    assert_eq!(
        fixture.count_release_lookups("mb-slot"),
        1,
        "the settle is still reading the pick: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.identification_status(&settling_key).await,
        Some(crate::IdentificationStatus::ReadingPick { import_asked: false })
    );
    for dir in &running {
        assert_eq!(
            fixture.identification_status(&crate::CandidateKey::folder(dir)).await,
            Some(crate::IdentificationStatus::Running),
            "{} started while the pick was read",
            dir.display()
        );
    }

    fixture.finish_reading().await;
    assert_eq!(fixture.judgement_for(&settling).await, (true, None));
}

/// "Identify selected" asks for each row in turn. A row asked for while
/// every slot is taken waits, and the page shows it queued until a slot
/// frees and then running, at the list's one request revision. A row that
/// finds a slot free starts at once, so its queued moment is no state the
/// page promises to show.
///
/// Each row looks up its own barcode: rows sharing one lookup would answer
/// the waiting row's from the response cache once the first came back, and
/// its run could pass running before the page is delivered again.
#[tokio::test(flavor = "multi_thread")]
async fn a_batch_identify_shows_a_row_waiting_for_a_slot_queued_then_running() {
    let fixture = Fixture::new("batch-identify-rows").await;
    fixture.import.register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let dirs: Vec<PathBuf> = (0..MAX_IN_FLIGHT + 1)
        .map(|index| {
            let dir = fixture.barcode_candidate(&format!("Album {index}"));
            std::fs::write(dir.join(format!("notes-{index}.txt")), format!("{index}")).unwrap();
            dir
        })
        .collect();
    fixture.provider.route("/release?", 200, r#"{"releases":[]}"#);
    fixture.provider.hold("query=barcode");
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(MAX_IN_FLIGHT + 1).await;

    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    let keys: Vec<crate::CandidateKey> =
        dirs.iter().map(|dir| crate::CandidateKey::folder(dir)).collect();
    for key in &keys {
        assert_eq!(page_live(&initial)[key].facts.identification, None);
    }

    for key in &keys {
        fixture.identification().rerun_identify(key.clone());
    }
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;

    let status = |snapshot: &crate::ImportListSnapshot, key: &crate::CandidateKey| {
        page_live(snapshot)[key].facts.identification.clone()
    };
    let running = Some(crate::IdentificationStatus::Running);
    let queued = Some(crate::IdentificationStatus::Queued);
    // Every slot holds a lookup the provider has not answered, so the row
    // over the cap waits.
    let waiting = loop {
        let snapshot = next_page(&list).await;
        assert_eq!(
            snapshot.view_revision, initial.view_revision,
            "identifying changed the list's request"
        );
        let runs = keys.iter().filter(|key| status(&snapshot, key) == running).count();
        let waits: Vec<&crate::CandidateKey> =
            keys.iter().filter(|key| status(&snapshot, key) == queued).collect();
        if runs == MAX_IN_FLIGHT && waits.len() == 1 {
            break waits[0].clone();
        }
    };

    // One lookup answers, its run ends and frees its slot, and the waiting
    // row's run takes it, held at its own lookup.
    fixture.provider.let_one_through();
    loop {
        let snapshot = next_page(&list).await;
        assert_eq!(
            snapshot.view_revision, initial.view_revision,
            "identifying changed the list's request"
        );
        match status(&snapshot, &waiting) {
            Some(crate::IdentificationStatus::Queued) => {}
            Some(crate::IdentificationStatus::Running) => break,
            other => panic!("{waiting} went from queued to {other:?}"),
        }
    }
    fixture.provider.release();
}

/// A waiting candidate that is cancelled never reaches the provider, and the
/// candidates around it are answered as before.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_waiting_candidate_is_never_looked_up() {
    let fixture = Fixture::new("cancel-waiting").await;
    let dirs = flooded_queue(&fixture).await;
    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;
    let waiting =
        fixture.first_queued(&dirs).await.expect("one candidate is over the cap and waiting");
    let waiting_key = crate::CandidateKey::folder(&waiting);

    fixture.identification().cancel(vec![waiting_key.clone()]).await.unwrap();
    assert_not_identifying(&fixture, &waiting_key).await;
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, sweep).await.expect("the pass ends").unwrap();

    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "the cancelled candidate took no slot: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&waiting).await.is_none());
    for dir in dirs.iter().filter(|dir| **dir != waiting) {
        assert!(
            fixture.identified_for(dir).await.is_some(),
            "{} is answered as it would have been",
            dir.display()
        );
    }
}

/// A running identification that is cancelled writes nothing — no verdict, no
/// failure, no record of the cancel: the candidate's stored state is what it
/// was before the run — and a person asking identifies it.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_run_leaves_the_candidate_as_it_was_until_asked_again() {
    let fixture = Fixture::new("cancel-running").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let before = fixture.stored_for(&dir).await.expect("the scan stores the candidate's state row");
    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;

    fixture.identification().cancel(vec![key.clone()]).await.unwrap();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("cancelling the only job drains the pass")
        .unwrap();
    assert_not_identifying(&fixture, &key).await;
    fixture.provider.release();

    assert_eq!(
        fixture.stored_for(&dir).await,
        Some(before),
        "a cancel stores nothing: no verdict, and nothing about the cancel"
    );
    assert!(
        fixture.import.candidate_runtime(&key).await.unwrap().is_none(),
        "nothing is left running or failed for it"
    );
    fixture.drain_automatic().await;
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "nothing queues it again on its own"
    );

    fixture.identification().rerun_identify(key.clone());
    tokio::time::timeout(HANG_GUARD, fixture.await_identified_row(&dir))
        .await
        .expect("a person asking identifies it again");
}

/// Cancelling everything empties the queue — what was running and what was
/// waiting — and nothing of it is answered afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_everything_empties_the_queue() {
    let fixture = Fixture::new("cancel-all").await;
    let dirs = flooded_queue(&fixture).await;
    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("query=barcode", MAX_IN_FLIGHT).await;

    fixture.identification().cancel_all().await.unwrap();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("cancelling everything drains the pass")
        .unwrap();
    for dir in &dirs {
        assert_not_identifying(&fixture, &crate::CandidateKey::folder(dir)).await;
    }
    fixture.provider.release();
    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.count_containing("query=barcode"),
        MAX_IN_FLIGHT,
        "nothing cancelled went out again: {:?}",
        fixture.provider.requests()
    );
    for dir in &dirs {
        assert!(fixture.identified_for(dir).await.is_none(), "{} stores nothing", dir.display());
    }
}

/// A run cancelled while its offered release's document is being read, the
/// last thing it waits on before its answer is written, stores nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_answer_is_not_written() {
    let fixture = Fixture::new("cancel-settling").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("/discid/", 200, discid_json("mb-1", "rg-1", &[probed, 0]));
    // The release is read from the Disc ID answer; its group's page is the
    // document the read asks for.
    fixture.provider.hold("/release-group/rg-1?");
    fixture.scan(1).await;
    let sweep = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/release-group/rg-1?", 1).await;
    assert_eq!(
        fixture.identification_status(&key).await,
        Some(crate::IdentificationStatus::Running),
        "the run is reading its offered release's document"
    );

    fixture.identification().cancel(vec![key.clone()]).await.unwrap();
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, sweep)
        .await
        .expect("cancelling the only job drains the pass")
        .unwrap();
    assert_not_identifying(&fixture, &key).await;

    assert!(fixture.identified_for(&dir).await.is_none());
    assert!(fixture.import.candidate_runtime(&key).await.unwrap().is_none());
}

/// A run the queue never started — a re-identify sheet's — ends through the
/// same cancel: one cancel for a candidate's identification, however it began.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_the_queue_did_not_start_ends_through_the_same_cancel() {
    let fixture = Fixture::new("cancel-outside-queue").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    // Nothing found is queued, so the queue holds nothing for the key.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;
    let candidate = fixture
        .import
        .answerable_candidate(&key)
        .await
        .unwrap()
        .expect("the scanned candidate is answerable");
    let run = fixture.import.new_identification_run();
    assert!(fixture
        .import
        .start_identification(
            run,
            key.clone(),
            ExtractionSource::Candidate { candidate },
            CallClass::REQUESTED,
            LookupChoices::default(),
            None,
        )
        .await
        .unwrap());
    fixture.provider.wait_for_request("/discid/", 1).await;
    assert!(fixture.import.is_identifying(&key));

    fixture.identification().cancel(vec![key.clone()]).await.unwrap();
    assert_not_identifying(&fixture, &key).await;
    fixture.provider.release();

    assert!(fixture.identified_for(&dir).await.is_none());
}
