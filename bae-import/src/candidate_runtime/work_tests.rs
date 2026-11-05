use super::*;

const WORK: CandidateWork = CandidateWork::Extraction;

#[tokio::test]
async fn starting_a_key_again_advances_the_generation_and_cancels_the_last() {
    let runtime = CandidateRuntime::for_test().await;
    let (first, first_generation) =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |token, generation| {
            (token, generation)
        });
    let (second, second_generation) =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |token, generation| {
            (token, generation)
        });

    assert!(second_generation > first_generation);
    assert!(first.is_cancelled());
    assert!(!second.is_cancelled());
}

#[tokio::test]
async fn generations_are_unique_across_keys_and_kinds() {
    let runtime = CandidateRuntime::for_test().await;
    let a = runtime.start_work(WORK, crate::CandidateKey::test("a"), |_, generation| generation);
    let b = runtime.start_work(WORK, crate::CandidateKey::test("b"), |_, generation| generation);
    let c = runtime.start_work(
        CandidateWork::Identify,
        crate::CandidateKey::test("a"),
        |_, generation| generation,
    );
    assert!(a != b && b != c && a != c);
}

#[tokio::test]
async fn only_the_current_generation_emits() {
    let runtime = CandidateRuntime::for_test().await;
    let stale =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |_, generation| generation);
    assert_eq!(
        runtime.while_work_current(WORK, &crate::CandidateKey::test("cand"), stale, || "sent"),
        Some("sent")
    );

    let live =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |_, generation| generation);
    assert_eq!(
        runtime.while_work_current(WORK, &crate::CandidateKey::test("cand"), stale, || "sent"),
        None
    );
    assert_eq!(
        runtime.while_work_current(WORK, &crate::CandidateKey::test("cand"), live, || "sent"),
        Some("sent")
    );

    runtime.cancel_work(WORK, &crate::CandidateKey::test("cand"));
    assert_eq!(
        runtime.while_work_current(WORK, &crate::CandidateKey::test("cand"), live, || "sent"),
        None
    );
}

#[tokio::test]
async fn a_release_removes_only_the_current_generation() {
    let runtime = CandidateRuntime::for_test().await;
    let stale =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |_, generation| generation);
    let (live, live_generation) =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |token, generation| {
            (token, generation)
        });

    runtime
        .release_work(WORK, &crate::CandidateKey::test("cand"), stale)
        .expect("the lock is sound");
    assert!(runtime.is_working(WORK, &crate::CandidateKey::test("cand")));
    assert!(!live.is_cancelled());

    runtime
        .release_work(WORK, &crate::CandidateKey::test("cand"), live_generation)
        .expect("the lock is sound");
    assert!(!runtime.is_working(WORK, &crate::CandidateKey::test("cand")));
}

#[tokio::test]
async fn a_removal_or_rebind_ends_every_kind_of_work_for_its_key_alone() {
    let runtime = CandidateRuntime::for_test().await;
    let extraction = runtime.start_work(
        CandidateWork::Extraction,
        crate::CandidateKey::test("gone"),
        |token, _| token,
    );
    let identify = runtime.start_work(
        CandidateWork::Identify,
        crate::CandidateKey::test("gone"),
        |token, _| token,
    );
    let other = runtime.start_work(
        CandidateWork::Identify,
        crate::CandidateKey::test("kept"),
        |token, _| token,
    );

    runtime.record_event(&crate::ImportEvent::Scan(crate::ScanEvent::CandidateRemoved {
        candidate_key: crate::CandidateKey::test("gone"),
    }));

    assert!(extraction.is_cancelled() && identify.is_cancelled());
    assert!(!other.is_cancelled());
    assert_eq!(
        runtime.working_keys(CandidateWork::Identify),
        vec![crate::CandidateKey::test("kept")]
    );
}

/// A release made while an unwind is under way must not panic again, which
/// ends the process: a poisoned lock is an error the caller reports.
#[tokio::test]
async fn a_released_extraction_reports_a_poisoned_lock_without_panicking() {
    let runtime = CandidateRuntime::for_test().await;
    let generation =
        runtime.start_work(WORK, crate::CandidateKey::test("cand"), |_, generation| generation);
    runtime.poison_work_lock();

    let released = runtime.release_work(WORK, &crate::CandidateKey::test("cand"), generation);

    assert!(released.is_err());
}
