//! The worker opening a library: no standing a crashed process left
//! survives it.

use crate::identification::tests::*;
use crate::IdentifyRunId;

/// What a run and an import leave standing as the process that held them
/// dies, in `fixture`'s library.
async fn standings_a_crash_left(fixture: &Fixture) {
    let tables = fixture.tables();
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let running = crate::CandidateKey::folder(&dir);
    let files =
        crate::CandidateFiles { content_hash: fixture.content_hash(&dir), file_edit_revision: 0 };
    // What a run and an import left as the process that held them died.
    tables
        .change_candidate_identifications(vec![
            (
                running.clone(),
                crate::candidate_runtime::IdentificationChange::Admit {
                    files: files.clone(),
                    admission: crate::Admission::Requested,
                },
            ),
            (
                running,
                crate::candidate_runtime::IdentificationChange::RunStarted {
                    files,
                    run: IdentifyRunId::for_test(1),
                },
            ),
        ])
        .await
        .unwrap();
    tables
        .claim_candidate_import(&crate::CandidateKey::test("/watched/Writing"), "import-1", None)
        .await
        .unwrap();
    tables
        .move_candidate_import(
            &crate::CandidateKey::test("/watched/Writing"),
            "import-1",
            Some(crate::ImportStep::Running(crate::ImportPhase::Finalizing)),
        )
        .await
        .unwrap();
    assert_eq!(tables.load_candidate_standings().await.unwrap().len(), 2);
}

/// A standing a crash left behind is gone once the library opens again.
#[tokio::test(flavor = "multi_thread")]
async fn the_library_opens_with_no_standing_a_crash_left() {
    let fixture = Fixture::new("standings-cleared-on-open").await;
    standings_a_crash_left(&fixture).await;

    let reopened = fixture.parts.start().await.unwrap();

    assert!(fixture.tables().load_candidate_standings().await.unwrap().is_empty());
    tokio::task::spawn_blocking(move || reopened.stop_and_join())
        .await
        .expect("the reopened import service stops");
}

/// A service a test starts with a tag reader of its own opens the library as
/// any start does.
#[tokio::test(flavor = "multi_thread")]
async fn a_start_reading_tags_its_own_way_opens_with_no_standing_a_crash_left() {
    let fixture = Fixture::new("standings-cleared-on-open-reading-tags").await;
    standings_a_crash_left(&fixture).await;

    let reopened = fixture
        .parts
        .start_reading_tags_with(std::sync::Arc::new(crate::file_tag_reader::LoftyFileTagReader))
        .await
        .unwrap();

    assert!(fixture.tables().load_candidate_standings().await.unwrap().is_empty());
    tokio::task::spawn_blocking(move || reopened.stop_and_join())
        .await
        .expect("the reopened import service stops");
}
