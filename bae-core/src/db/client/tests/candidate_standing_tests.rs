//! Each candidate's standing as the standings table stores and reads it back.

use super::{empty_db, exec_batch};
use crate::identify::IdentifyRunId;
use crate::import::candidate_runtime::{
    IdentificationChange, IdentificationStage, StoredIdentification, StoredImport, StoredStanding,
};
use crate::import::candidates::{Admission, CandidateFiles, SaveFailure};
use crate::import::triage::ImportStanding;

/// Store `key` as a candidate the scan holds with `files`, as the scan would.
async fn scanned(db: &super::super::Database, key: &str, files: &CandidateFiles) {
    exec_batch(
        db,
        &format!(
            "INSERT OR IGNORE INTO watched_import_folders (path, position) VALUES ('/music', 0);
             INSERT OR IGNORE INTO folder_scan_roots (watched_folder_path, generation, status, volume)
                 VALUES ('/music', 1, 'complete', 'local');
             DELETE FROM scan_candidate WHERE path = '{key}';
             INSERT INTO scan_candidate (watched_folder_path, path, generation, kind, name,
                 display_path, folder, file_root, scope, content_hash, file_edit_revision)
                 VALUES ('/music', '{key}', 1, 'valid', 'Album', '{key}', '{key}', '{key}',
                     'recursive', '{hash}', {revision});",
            hash = files.content_hash,
            revision = files.file_edit_revision,
        ),
    )
    .await;
}

fn files_of(key: &str) -> CandidateFiles {
    CandidateFiles {
        content_hash: format!("hash:{key}"),
        file_edit_revision: 0,
    }
}

fn run(number: u64) -> IdentifyRunId {
    IdentifyRunId::for_test(number)
}

/// Apply `changes` to `key` one write each.
async fn change(db: &super::super::Database, key: &str, changes: Vec<IdentificationChange>) {
    for change in changes {
        db.change_candidate_identifications(vec![(key.to_string(), change)])
            .await
            .unwrap();
    }
}

/// Every stage a candidate's identification can reach, and every standing of
/// its import, reads back as the change that reached it left it.
#[tokio::test]
async fn every_standing_reads_back_as_written() {
    let (db, _tmp) = empty_db().await;
    let admit = |key: &str| IdentificationChange::Admit {
        files: files_of(key),
        admission: Admission::Requested,
    };
    let start = |key: &str| IdentificationChange::RunStarted {
        files: files_of(key),
        run: run(1),
    };
    let answer = |reads_pick| IdentificationChange::RunAnswered {
        run: run(1),
        reads_pick,
    };
    let fail = |key: &str, failure| IdentificationChange::AnswerFailed {
        files: files_of(key),
        run: run(1),
        failure,
    };
    let not_written = SaveFailure::NotWritten {
        error: "the disk is full".to_string(),
    };
    let inapplicable = SaveFailure::Inapplicable {
        error: "a track has no title".to_string(),
    };
    let cases = [
        (
            "/music/Queued",
            vec![admit("/music/Queued")],
            IdentificationStage::Queued {
                admission: Admission::Requested,
            },
        ),
        (
            "/music/Running",
            vec![admit("/music/Running"), start("/music/Running")],
            IdentificationStage::Running { run: run(1) },
        ),
        (
            "/music/Finalizing",
            vec![
                admit("/music/Finalizing"),
                start("/music/Finalizing"),
                answer(false),
            ],
            IdentificationStage::Finalizing { run: run(1) },
        ),
        (
            "/music/Asked",
            vec![
                admit("/music/Asked"),
                start("/music/Asked"),
                answer(true),
                IdentificationChange::ImportAsked,
            ],
            IdentificationStage::ReadingPick {
                run: run(1),
                import_asked: true,
            },
        ),
        (
            "/music/Not Written",
            vec![fail("/music/Not Written", not_written.clone())],
            IdentificationStage::Failed {
                failure: not_written,
            },
        ),
        (
            "/music/Inapplicable",
            vec![fail("/music/Inapplicable", inapplicable.clone())],
            IdentificationStage::Failed {
                failure: inapplicable,
            },
        ),
    ];
    for (key, changes, stage) in cases {
        scanned(&db, key, &files_of(key)).await;
        change(&db, key, changes).await;
        assert_eq!(
            db.load_candidate_standing(key).await.unwrap(),
            StoredStanding {
                identification: Some(StoredIdentification {
                    files: files_of(key),
                    stage,
                }),
                import: None,
            },
            "{key}"
        );
    }

    let key = "/music/Import";
    db.claim_candidate_import(key, "import-1", None)
        .await
        .unwrap();
    for standing in [
        ImportStanding::Queued,
        ImportStanding::Running,
        ImportStanding::Writing,
    ] {
        db.move_candidate_import(key, "import-1", Some(standing))
            .await
            .unwrap();
        assert_eq!(
            db.load_candidate_standing(key).await.unwrap(),
            StoredStanding {
                identification: None,
                import: Some(StoredImport {
                    import_id: "import-1".to_string(),
                    standing,
                }),
            }
        );
    }
}

/// A candidate nothing is running for has no row: ending the last thing
/// going for it removes the row it had.
#[tokio::test]
async fn nothing_running_removes_the_row() {
    let (db, _tmp) = empty_db().await;
    let key = "/music/Album";
    scanned(&db, key, &files_of(key)).await;
    change(
        &db,
        key,
        vec![IdentificationChange::Admit {
            files: files_of(key),
            admission: Admission::Automatic,
        }],
    )
    .await;
    db.claim_candidate_import(key, "import-1", None)
        .await
        .unwrap();

    change(&db, key, vec![IdentificationChange::Withdraw]).await;
    assert_eq!(
        db.load_candidate_standing(key)
            .await
            .unwrap()
            .identification,
        None
    );
    db.move_candidate_import(key, "import-1", None)
        .await
        .unwrap();

    assert!(db.load_candidate_standings().await.unwrap().is_empty());
    assert_eq!(
        db.load_candidate_standing(key).await.unwrap(),
        StoredStanding::default()
    );
}

/// A second claim on a candidate an import owns is refused by the row, and
/// changes nothing — the identification change asked with it included.
#[tokio::test]
async fn a_claim_while_another_stands_is_refused_whole() {
    let (db, _tmp) = empty_db().await;
    let key = "/music/Album";
    scanned(&db, key, &files_of(key)).await;
    change(
        &db,
        key,
        vec![
            IdentificationChange::Admit {
                files: files_of(key),
                admission: Admission::Requested,
            },
            IdentificationChange::RunStarted {
                files: files_of(key),
                run: run(1),
            },
        ],
    )
    .await;
    db.claim_candidate_import(key, "import-1", None)
        .await
        .unwrap();
    let claimed = db.load_candidate_standing(key).await.unwrap();

    assert_eq!(
        db.claim_candidate_import(key, "import-2", Some(IdentificationChange::Cancelled))
            .await
            .unwrap(),
        crate::db::ImportClaim::Refused
    );
    assert_eq!(db.load_candidate_standing(key).await.unwrap(), claimed);
}

/// Clearing the table, as the library opens, removes every standing.
#[tokio::test]
async fn clearing_removes_every_standing() {
    let (db, _tmp) = empty_db().await;
    for key in ["/music/One", "/music/Two"] {
        scanned(&db, key, &files_of(key)).await;
        change(
            &db,
            key,
            vec![IdentificationChange::Admit {
                files: files_of(key),
                admission: Admission::Automatic,
            }],
        )
        .await;
    }
    db.claim_candidate_import("/music/Two", "import-1", None)
        .await
        .unwrap();
    db.clear_candidate_standings().await.unwrap();

    assert!(db.load_candidate_standings().await.unwrap().is_empty());
}

/// How many imports the standings read counts in flight.
async fn imports_in_flight(db: &super::super::Database) -> u32 {
    db.subscribe_candidate_standings()
        .next()
        .await
        .expect("the query stays open")
        .unwrap()
        .imports_in_flight
}

/// The imports in flight are the rows an import holds, whatever it has
/// reached: a candidate only being identified is not one, and an import's
/// ending takes its row out.
#[tokio::test]
async fn the_imports_in_flight_are_the_rows_an_import_holds() {
    let (db, _tmp) = empty_db().await;
    for key in ["/music/One", "/music/Two", "/music/Three"] {
        scanned(&db, key, &files_of(key)).await;
        change(
            &db,
            key,
            vec![IdentificationChange::Admit {
                files: files_of(key),
                admission: Admission::Automatic,
            }],
        )
        .await;
    }
    assert_eq!(imports_in_flight(&db).await, 0);

    db.claim_candidate_import("/music/One", "import-1", None)
        .await
        .unwrap();
    db.claim_candidate_import("/music/Two", "import-2", None)
        .await
        .unwrap();
    db.move_candidate_import("/music/Two", "import-2", Some(ImportStanding::Writing))
        .await
        .unwrap();
    assert_eq!(imports_in_flight(&db).await, 2);

    db.move_candidate_import("/music/One", "import-1", None)
        .await
        .unwrap();
    assert_eq!(imports_in_flight(&db).await, 1);
}
