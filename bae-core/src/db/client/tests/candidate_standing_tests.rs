//! Each candidate's standing as the standings table stores and reads it back.

use super::empty_db;
use crate::import::triage::{IdentificationStatus, ImportStanding, TriageRuntimeFacts};
use crate::import::SaveFailure;

fn standing(
    identification: Option<IdentificationStatus>,
    import: Option<ImportStanding>,
) -> TriageRuntimeFacts {
    TriageRuntimeFacts {
        identification,
        import,
    }
}

/// Every standing a candidate can have reads back as it was written.
#[tokio::test]
async fn every_standing_reads_back_as_written() {
    let (db, _tmp) = empty_db().await;
    let standings = vec![
        (
            "/music/Queued".to_string(),
            standing(Some(IdentificationStatus::Queued), None),
        ),
        (
            "/music/Running".to_string(),
            standing(Some(IdentificationStatus::Running), None),
        ),
        (
            "/music/Finalizing".to_string(),
            standing(Some(IdentificationStatus::Finalizing), None),
        ),
        (
            "/music/Reading".to_string(),
            standing(
                Some(IdentificationStatus::ReadingPick {
                    import_asked: false,
                }),
                None,
            ),
        ),
        (
            "/music/Asked".to_string(),
            standing(
                Some(IdentificationStatus::ReadingPick { import_asked: true }),
                None,
            ),
        ),
        (
            "/music/Not Written".to_string(),
            standing(
                Some(IdentificationStatus::FinalizationFailed {
                    failure: SaveFailure::NotWritten {
                        error: "the disk is full".to_string(),
                    },
                }),
                None,
            ),
        ),
        (
            "/music/Inapplicable".to_string(),
            standing(
                Some(IdentificationStatus::FinalizationFailed {
                    failure: SaveFailure::Inapplicable {
                        error: "a track has no title".to_string(),
                    },
                }),
                None,
            ),
        ),
        (
            "/music/Import Queued".to_string(),
            standing(None, Some(ImportStanding::Queued)),
        ),
        (
            "/music/Import Running".to_string(),
            standing(None, Some(ImportStanding::Running)),
        ),
        (
            "/music/Import Writing".to_string(),
            standing(None, Some(ImportStanding::Writing)),
        ),
        (
            "/music/Both".to_string(),
            standing(
                Some(IdentificationStatus::Queued),
                Some(ImportStanding::Queued),
            ),
        ),
    ];
    db.write_candidate_standings(standings.clone())
        .await
        .unwrap();

    assert_eq!(
        db.load_candidate_standings().await.unwrap(),
        standings.into_iter().collect()
    );
}

/// A candidate nothing is running for has no row: writing that standing
/// removes the row it had.
#[tokio::test]
async fn nothing_running_removes_the_row() {
    let (db, _tmp) = empty_db().await;
    let key = "/music/Album".to_string();
    db.write_candidate_standings(vec![(
        key.clone(),
        standing(Some(IdentificationStatus::Running), None),
    )])
    .await
    .unwrap();
    db.write_candidate_standings(vec![(key, TriageRuntimeFacts::default())])
        .await
        .unwrap();

    assert!(db.load_candidate_standings().await.unwrap().is_empty());
}

/// Clearing the table, as the library opens, removes every standing.
#[tokio::test]
async fn clearing_removes_every_standing() {
    let (db, _tmp) = empty_db().await;
    db.write_candidate_standings(vec![
        (
            "/music/One".to_string(),
            standing(Some(IdentificationStatus::Running), None),
        ),
        (
            "/music/Two".to_string(),
            standing(None, Some(ImportStanding::Writing)),
        ),
    ])
    .await
    .unwrap();
    db.clear_candidate_standings().await.unwrap();

    assert!(db.load_candidate_standings().await.unwrap().is_empty());
}
