//! Each key's runtime as one read of the tables composes it, and the live
//! queries that follow it, the identification counts and the import
//! percents.

use super::*;
use crate::candidate_runtime::tests::*;
use crate::types::{ImportPhase, ImportProgress, PrepareStep};
use crate::ImportInFlight;
use crate::ImportStep;
use crate::{Admission, IdentificationRuntime};
// ── What identification is doing, as the pane shows it ──────────────────────

use crate::IdentificationInFlight;
use crate::ImportEvent;
use bae_util::test_channels::HANG_GUARD;

/// A reader that joins after imports started reads every one of them.
#[tokio::test]
async fn a_late_reader_reads_every_running_key() {
    let runtime = CandidateRuntime::for_test().await;
    claim(&runtime, &crate::CandidateKey::test("/watch/a/rel1")).await;
    measuring(&runtime, &crate::CandidateKey::test("/watch/a/rel1"), 10).await;
    claim(&runtime, &crate::CandidateKey::test("/watch/a/rel2")).await;

    let running = runtime.snapshots().await.unwrap();
    assert_eq!(running.len(), 2);
    assert_eq!(
        running[&crate::CandidateKey::test("/watch/a/rel1")].import,
        Some(ImportInFlight { step: ImportStep::Running(ImportPhase::MeasuringLoudness) })
    );
    assert!(running[&crate::CandidateKey::test("/watch/a/rel2")].import.is_some());
}

/// What the runtime says identification is doing for `key`.
async fn in_flight(
    runtime: &CandidateRuntime,
    key: &crate::CandidateKey,
) -> Option<IdentificationInFlight> {
    runtime.snapshot(key).await.unwrap()?.identification_in_flight()
}

/// A key waiting on the queue reads as queued rather than as nothing started;
/// its run's first report makes it the run, whose answer shows while it is
/// written.
#[tokio::test]
async fn a_queued_key_reads_as_queued_until_its_run_reports() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, Some(IdentificationInFlight::Queued));

    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(published(triangulating())))
    );

    assert!(runtime.tables.publish_candidate_run(key, run(1), &manual_only(), None).await.unwrap());
    runtime.run_answered(key, run(1), false).await.unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(published(manual_only())))
    );

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, None, "nothing is identifying it");
}

/// A key asked for again while its last answer is being written waits for
/// its next run: the request supersedes the answer, whose write then ends
/// nothing.
#[tokio::test]
async fn a_request_over_an_answer_being_written_waits_for_the_next_run() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    runtime.admit(vec![(key.clone(), files.clone())], Admission::Automatic).await.unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, Some(IdentificationInFlight::Queued));

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, Some(IdentificationInFlight::Queued));
}

/// A library release's re-identify run is no list row's: its row holds it
/// whole, its run in flight and then its answer, until its sheet closes.
#[tokio::test]
async fn a_re_identify_run_is_held_by_the_runtime_alone() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::reidentify("release-1");

    runtime.tables.publish_release_run(key, run(1), &triangulating(), None).await.unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(published(triangulating())))
    );
    runtime.tables.publish_release_run(key, run(1), &manual_only(), None).await.unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(published(manual_only())))
    );
    assert_eq!(runtime.standing_facts(key).await.unwrap(), Default::default());

    runtime.cancel_identification(key).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, None);
}

// ── Readers of the current state ────────────────────────────────────────────
//
// A screen follows one key's runtime as a live query of the tables, so
// however many writes land before it pulls, it reads where they left off.

/// The next value `query` delivers; the test fails if none ever comes.
async fn next_runtime(
    query: &mut coven::LiveQuery<Option<crate::CandidateRuntimeSnapshot>, bae_store::DbError>,
) -> Option<crate::CandidateRuntimeSnapshot> {
    tokio::time::timeout(HANG_GUARD, query.next())
        .await
        .expect("the query delivers")
        .expect("the query stays open")
        .expect("the query reads")
}

/// Pull from `query` until a value satisfies `wanted`, and return it.
async fn runtime_until(
    query: &mut coven::LiveQuery<Option<crate::CandidateRuntimeSnapshot>, bae_store::DbError>,
    wanted: impl Fn(&Option<crate::CandidateRuntimeSnapshot>) -> bool,
) -> Option<crate::CandidateRuntimeSnapshot> {
    loop {
        let value = next_runtime(query).await;
        if wanted(&value) {
            return value;
        }
    }
}

/// Wait for the percents watch to satisfy a condition, failing the test if
/// it never does.
async fn percents_reach<T, E: std::fmt::Debug>(
    waiting: impl std::future::Future<Output = Result<T, E>>,
) {
    tokio::time::timeout(HANG_GUARD, waiting)
        .await
        .expect("the percents get there")
        .expect("the percents watch stays open");
}

/// The step `runtime`'s import is at, where one runs.
fn import_step(runtime: &Option<crate::CandidateRuntimeSnapshot>) -> Option<ImportStep> {
    runtime.as_ref()?.import.as_ref().map(|import| import.step)
}

/// A key's runtime query reads nothing in flight, then Queued, then Running
/// with the state the run published, from the table, and nothing once the
/// run ends.
#[tokio::test]
async fn a_key_s_runtime_query_shows_queued_then_running_from_the_table() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    let mut query = runtime.tables.subscribe_candidate_runtime(key);
    assert_eq!(next_runtime(&mut query).await, None);

    runtime.admit(vec![(key.clone(), files.clone())], Admission::Requested).await.unwrap();
    let queued = runtime_until(&mut query, Option::is_some).await.unwrap();
    assert_eq!(queued.identification, Some(IdentificationRuntime::Queued(Admission::Requested)));

    assert!(runtime.run_started(key, &files, run(1), &triangulating()).await.unwrap());
    let running = runtime_until(&mut query, |value| {
        matches!(
            value.as_ref().and_then(|runtime| runtime.identification.as_ref()),
            Some(IdentificationRuntime::Running(_))
        )
    })
    .await
    .unwrap();
    assert_eq!(
        running.identification,
        Some(IdentificationRuntime::Running(published(triangulating())))
    );

    runtime.run_ended(key, run(1)).await.unwrap();
    runtime_until(&mut query, Option::is_none).await;
}

/// The import counts' live query delivers each write that moves them: an
/// admission opens the batch at its total, a withdrawal ends one of it, and
/// a claim puts an import in flight.
#[tokio::test]
async fn the_import_counts_query_delivers_each_write_that_moves_them() {
    let runtime = CandidateRuntime::for_test().await;
    let mut counts = runtime.tables.subscribe_import_counts();
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
    let marks = vec![
        (first.clone(), scanned_key(&runtime, first).await),
        (second.clone(), scanned_key(&runtime, second).await),
    ];
    let mut next_counts = async || {
        tokio::time::timeout(HANG_GUARD, counts.next())
            .await
            .expect("the query delivers")
            .expect("the query stays open")
            .expect("the query reads")
    };
    assert_eq!(next_counts().await, crate::ImportCounts::default());

    runtime.admit(marks, Admission::Automatic).await.unwrap();
    assert_eq!(
        next_counts().await,
        crate::ImportCounts { identified: 0, admitted: 2, imports_in_flight: 0 }
    );

    runtime.withdraw(first).await.unwrap();
    assert_eq!(
        next_counts().await,
        crate::ImportCounts { identified: 1, admitted: 2, imports_in_flight: 0 }
    );

    claim(&runtime, &crate::CandidateKey::test("/watch/a/rel3")).await;
    assert_eq!(
        next_counts().await,
        crate::ImportCounts { identified: 1, admitted: 2, imports_in_flight: 1 }
    );
}

/// A key's runtime query hears its own key's writes and no other key's.
#[tokio::test]
async fn a_key_s_runtime_query_hears_only_its_key() {
    let runtime = CandidateRuntime::for_test().await;
    let first = &crate::CandidateKey::test("/watch/a/rel1");
    let second = &crate::CandidateKey::test("/watch/a/rel2");
    claim(&runtime, first).await;
    let mut query = runtime.tables.subscribe_candidate_runtime(first);
    assert_eq!(
        import_step(&next_runtime(&mut query).await),
        Some(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles))
    );

    claim(&runtime, second).await;
    measuring(&runtime, second, 40).await;

    // The next value is the sentinel: a write of the other key that this key's
    // query read would have changed what it delivers before its own write's.
    measuring(&runtime, first, 40).await;
    assert_eq!(
        import_step(&next_runtime(&mut query).await),
        Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
    );
}

/// A running import's percent moves in the percents watch by its key; the
/// runtime query hears only the step it reached, so a percent tick at the
/// same step tells it nothing.
#[tokio::test]
async fn an_import_percent_moves_the_percents_not_the_runtime() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::CandidateKey::test("/watch/a/rel1");
    let other = &crate::CandidateKey::test("/watch/a/rel2");
    claim(&runtime, key).await;
    claim(&runtime, other).await;
    let mut percents = runtime.watch_import_percents();
    measuring(&runtime, key, 40).await;
    percents_reach(percents.wait_for(|percents| percents.get(key) == Some(&40))).await;
    let mut query = runtime.tables.subscribe_candidate_runtime(key);
    assert_eq!(
        import_step(&next_runtime(&mut query).await),
        Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
    );

    runtime.record_event(&progress(key, 41));
    percents_reach(percents.wait_for(|percents| percents.get(key) == Some(&41))).await;
    assert!(!percents.percents().contains_key(other), "a claim no report moved has no percent");
    // A percent tick writes no row, so the runtime query's next value is the
    // one the import's move below produces.

    runtime.move_import(key, "imp-1", None).await.unwrap();
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.clone(),
        progress: ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
        },
    });
    percents_reach(percents.wait_for(|percents| !percents.contains_key(key))).await;
    runtime_until(&mut query, Option::is_none).await;
}
