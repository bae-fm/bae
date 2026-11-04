//! The event bus: the feed holds every event and is taken once, and a reader
//! that hears an event finds it already recorded.

use super::*;

// Fixture row ids are v4 UUIDs because coven checks every synced row's
// primary key is one.
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882";

/// The event feed holds every event sent after it was taken, however many,
/// and only one reader takes it.
#[tokio::test]
async fn the_event_feed_holds_every_event_and_is_taken_once() {
    let bus = ImportEventBus::new(CandidateRuntime::for_test().await);
    let mut feed = bus.take_feed().expect("the feed is there to take");
    for _ in 0..3 {
        bus.send(ImportEvent::Scan(ScanEvent::Finished));
    }

    let held = std::iter::from_fn(|| feed.try_recv().ok())
        .filter(|event| matches!(event, ImportEvent::Scan(ScanEvent::Finished)))
        .count();
    assert_eq!(held, 3, "the feed dropped none of them");
    assert!(bus.take_feed().is_none(), "the feed is taken once");
}

/// A reader that hears how far an import has got and then asks the runtime
/// finds it already there: the bus records an event before any reader hears
/// it.
#[tokio::test]
async fn an_import_s_progress_is_recorded_before_any_reader_hears_it() {
    let runtime = CandidateRuntime::for_test().await;
    let bus = ImportEventBus::new(runtime.clone());
    let mut events = bus.every_event();
    runtime
        .claim_for_import(&crate::CandidateKey::test("candidate"), "import-1", false)
        .await
        .unwrap();
    runtime
        .move_import(
            &crate::CandidateKey::test("candidate"),
            "import-1",
            Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles)),
        )
        .await
        .unwrap();

    bus.send(ImportEvent::ImportProgress {
        candidate_key: crate::CandidateKey::test("candidate"),
        progress: crate::ImportProgress::Progress {
            id: REL_1.to_string(),
            percent: Some(40),
            phase: crate::ImportPhase::ReadingFiles,
            import_id: "import-1".to_string(),
        },
    });

    events.recv().await.unwrap();
    assert_eq!(
        runtime
            .snapshot(&crate::CandidateKey::test("candidate"))
            .await
            .unwrap()
            .and_then(|candidate| candidate.import)
            .map(|import| import.step),
        Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles))
    );
}
