use super::super::*;
use crate::import::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot,
};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedFile,
};
use crate::util::content_type::ContentType;
use coven::FixedClock;
use std::path::PathBuf;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

async fn empty_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(FixedClock(now())),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    (db, tmp)
}

fn candidate(root: &str) -> FolderCandidate {
    let candidate_path = format!("{root}/candidate-a");
    let files = [("01.flac", 1_000), ("02.flac", 2_000)]
        .into_iter()
        .map(|(relative_path, size)| CandidateFile {
            file: ScannedFile::new(
                PathBuf::from(format!("{candidate_path}/{relative_path}")),
                relative_path.to_string(),
                size,
                1,
            )
            .with_test_flac_audio(),
            role: FileRole::Audio,
            proposed_audio: true,
        })
        .collect();
    FolderCandidate {
        path: PathBuf::from(&candidate_path),
        file_root: PathBuf::from(&candidate_path),
        name: "Candidate A".to_string(),
        files: CategorizedFiles { files },
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: "candidate-a".to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

async fn scanned_candidate(db: &Database, root: &str) -> (FolderCandidate, u64) {
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let candidate = candidate(root);
    db.save_folder_scan_item(root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();
    (candidate, generation)
}

fn snapshot(generation: u64, revision: u64) -> FileTagSnapshot {
    FileTagSnapshot {
        scan_generation: generation,
        file_edit_revision: revision,
        files: vec![
            FileTagFact {
                observation: FileObservation {
                    relative_path: "01.flac".to_string(),
                    size: 1_000,
                    modified_at_ns: 100,
                },
                title: Some("Track Title A".to_string()),
                track_artist: Some("Artist Name".to_string()),
                album_title: Some("Album Title".to_string()),
                album_artist: Some("Album Artist".to_string()),
                year: Some(2020),
                track_number: Some(1),
                disc_number: Some(1),
            },
            FileTagFact {
                observation: FileObservation {
                    relative_path: "02.flac".to_string(),
                    size: 2_000,
                    modified_at_ns: 200,
                },
                title: Some("Track Title B".to_string()),
                track_artist: None,
                album_title: None,
                album_artist: None,
                year: None,
                track_number: Some(2),
                disc_number: None,
            },
        ],
        embedded_cover: Some(EmbeddedCoverFact {
            source_relative_path: "01.flac".to_string(),
            content_type: ContentType::Png,
            data: vec![1, 2, 3, 4],
        }),
    }
}

#[tokio::test]
async fn file_tag_snapshot_round_trips_with_current_candidate_stamp() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();

    let empty = db
        .load_candidate_file_tag_snapshot(root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(empty.scan_generation, generation);
    assert_eq!(empty.candidate.file_edit_revision(), 0);
    assert_eq!(empty.snapshot, None);

    let expected = snapshot(generation, 0);
    assert!(db
        .replace_candidate_file_tag_snapshot(root, &key, &expected)
        .await
        .unwrap());

    let loaded = db
        .load_candidate_file_tag_snapshot(root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.scan_generation, generation);
    assert_eq!(loaded.candidate, candidate.into());
    assert_eq!(loaded.snapshot, Some(expected));
}

#[tokio::test]
async fn replacement_removes_every_prior_file_and_embedded_cover() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    db.replace_candidate_file_tag_snapshot(root, &key, &snapshot(generation, 0))
        .await
        .unwrap();

    let replacement = FileTagSnapshot {
        scan_generation: generation,
        file_edit_revision: 0,
        files: vec![
            FileTagFact {
                observation: FileObservation {
                    relative_path: "01.flac".to_string(),
                    size: 1_000,
                    modified_at_ns: 300,
                },
                title: None,
                track_artist: None,
                album_title: None,
                album_artist: None,
                year: None,
                track_number: None,
                disc_number: None,
            },
            FileTagFact {
                observation: FileObservation {
                    relative_path: "02.flac".to_string(),
                    size: 2_000,
                    modified_at_ns: 400,
                },
                title: None,
                track_artist: None,
                album_title: None,
                album_artist: None,
                year: None,
                track_number: None,
                disc_number: None,
            },
        ],
        embedded_cover: None,
    };
    assert!(db
        .replace_candidate_file_tag_snapshot(root, &key, &replacement)
        .await
        .unwrap());

    let loaded = db
        .load_candidate_file_tag_snapshot(root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.snapshot, Some(replacement));
}

#[tokio::test]
async fn stale_generation_is_reported_and_cannot_replace_snapshot() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, first_generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let stored = snapshot(first_generation, 0);
    db.replace_candidate_file_tag_snapshot(root, &key, &stored)
        .await
        .unwrap();

    let current_generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, current_generation, &ScanItem::Valid(candidate))
        .await
        .unwrap();
    db.finish_folder_scan(root, current_generation, None)
        .await
        .unwrap();

    let loaded = db
        .load_candidate_file_tag_snapshot(root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.scan_generation, current_generation);
    assert_eq!(loaded.candidate.file_edit_revision(), 0);
    assert_eq!(loaded.snapshot, Some(stored.clone()));
    assert_ne!(
        loaded.scan_generation,
        loaded.snapshot.as_ref().unwrap().scan_generation
    );

    assert!(!db
        .replace_candidate_file_tag_snapshot(root, &key, &stored)
        .await
        .unwrap());
    assert_eq!(
        db.load_candidate_file_tag_snapshot(root, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        Some(stored)
    );
}

#[tokio::test]
async fn stale_file_edit_revision_cannot_replace_snapshot() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let stored = snapshot(generation, 0);
    db.replace_candidate_file_tag_snapshot(root, &key, &stored)
        .await
        .unwrap();

    let root_owned = root.to_string();
    let key_owned = key.clone();
    db.call(move |sql| {
        sql.execute(
            "UPDATE scan_candidate SET file_edit_revision = 1 \
             WHERE watched_folder_path = ? AND path = ?",
            params![root_owned, key_owned],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let loaded = db
        .load_candidate_file_tag_snapshot(root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.candidate.file_edit_revision(), 1);
    assert_eq!(loaded.snapshot, Some(stored.clone()));
    assert!(!db
        .replace_candidate_file_tag_snapshot(root, &key, &stored)
        .await
        .unwrap());
}

#[tokio::test]
async fn failed_whole_replacement_preserves_the_previous_snapshot() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let stored = snapshot(generation, 0);
    db.replace_candidate_file_tag_snapshot(root, &key, &stored)
        .await
        .unwrap();

    let mut invalid = stored.clone();
    invalid.files[0].observation.modified_at_ns = -1;
    assert!(db
        .replace_candidate_file_tag_snapshot(root, &key, &invalid)
        .await
        .is_err());

    assert_eq!(
        db.load_candidate_file_tag_snapshot(root, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        Some(stored)
    );
}

#[tokio::test]
async fn stale_preparation_save_preserves_file_tags_and_embedded_cover() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let hash = candidate.files.content_hash();
    let stored = snapshot(generation, candidate.file_edit_revision);
    db.replace_candidate_file_tag_snapshot(root, &key, &stored)
        .await
        .unwrap();
    let stale = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    let expected = CandidateSaveExpectation {
        edit_revision: stale.file_edits.revision,
        metadata_revision: stale.metadata_revision,
        scanned: Some(ScannedCandidateKey {
            watched_folder_path: root.to_string(),
            candidate_path: key.clone(),
        }),
    };
    crate::import::CandidatePreparations::new(db.clone())
        .set_field(
            &hash,
            crate::import::CandidateEditField::AlbumTitle,
            "Current album",
        )
        .await
        .unwrap();
    let current = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    assert_ne!(current.metadata_revision, stale.metadata_revision);
    let mut replacement = stored.clone();
    replacement.files[0].title = Some("Stale track".into());
    replacement.embedded_cover.as_mut().unwrap().data = vec![9, 8, 7];

    let result = db
        .save_candidate_preparation(
            stale,
            expected,
            true,
            CandidateSaveExtras {
                file_tag_snapshot: Some(replacement),
                reshaped_files: None,
            },
        )
        .await
        .unwrap();
    assert!(matches!(result, CandidateSaved::Superseded));
    assert_eq!(
        db.load_candidate_preparation(&hash).await.unwrap().unwrap(),
        current
    );
    assert_eq!(
        db.load_candidate_file_tag_snapshot(root, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        Some(stored)
    );
}

#[tokio::test]
async fn recreated_candidate_rejects_the_removed_preparation() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, _) = scanned_candidate(&db, root).await;
    let hash = candidate.files.content_hash();
    let mut stale = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    let expected = CandidateSaveExpectation {
        edit_revision: stale.file_edits.revision,
        metadata_revision: stale.metadata_revision,
        scanned: None,
    };
    db.remove_watched_import_folder(root).await.unwrap();
    assert!(db
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .is_none());
    let (recreated, _) = scanned_candidate(&db, root).await;
    assert_eq!(recreated.files.content_hash(), hash);
    let current = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    stale.metadata.draft.album_title = "Removed candidate's draft".into();

    let result = db
        .save_candidate_preparation(stale, expected, true, CandidateSaveExtras::default())
        .await
        .unwrap();
    assert!(
        matches!(result, CandidateSaved::Superseded),
        "a removed candidate's save must not land on its replacement"
    );
    assert_eq!(
        db.load_candidate_preparation(&hash).await.unwrap().unwrap(),
        current
    );
}

#[tokio::test]
async fn recreated_candidate_keeps_its_failure_when_an_old_attempt_fails() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, _) = scanned_candidate(&db, root).await;
    let hash = candidate.files.content_hash();
    let stale = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    db.remove_watched_import_folder(root).await.unwrap();
    scanned_candidate(&db, root).await;
    let current = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    let current_failure = crate::import::ImportFailure {
        error: "Current attempt failed".into(),
        failed_at: now(),
        artist_identity_conflict: None,
    };
    db.save_import_candidate_failure(&read_preparation(&current), &current_failure)
        .await
        .unwrap();
    let stale_failure = crate::import::ImportFailure {
        error: "Removed attempt failed".into(),
        failed_at: now(),
        artist_identity_conflict: None,
    };
    assert!(
        db.save_import_candidate_failure(&read_preparation(&stale), &stale_failure)
            .await
            .is_err(),
        "the old attempt must not attach its failure to the recreated candidate"
    );
    assert_eq!(
        db.load_candidate_preparation(&hash).await.unwrap().unwrap(),
        current
    );
    let failure = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .unwrap();
    assert_eq!(failure, current_failure);
}

fn read_preparation(
    preparation: &crate::import::preparation::CandidatePreparation,
) -> crate::import::CandidateAsRead {
    crate::import::CandidateAsRead {
        content_hash: preparation.content_hash.clone(),
        file_edit_revision: preparation.file_edits.revision,
        metadata_revision: preparation.metadata_revision,
    }
}

#[tokio::test]
async fn removed_attempt_cannot_clear_a_recreated_candidates_failure() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, _) = scanned_candidate(&db, root).await;
    let hash = candidate.files.content_hash();
    let stale = read_preparation(&db.load_candidate_preparation(&hash).await.unwrap().unwrap());
    db.remove_watched_import_folder(root).await.unwrap();
    scanned_candidate(&db, root).await;
    let current = read_preparation(&db.load_candidate_preparation(&hash).await.unwrap().unwrap());
    let failure = crate::import::ImportFailure::error_only("Current failure", now());
    db.save_import_candidate_failure(&current, &failure)
        .await
        .unwrap();
    assert!(db.clear_import_candidate_failure(&stale).await.is_err());
    assert_eq!(
        db.load_import_candidate_pane_rows(&hash)
            .await
            .unwrap()
            .failure,
        Some(failure)
    );
}

#[tokio::test]
async fn exhausted_candidate_revision_preserves_the_stored_preparation() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let (candidate, _) = scanned_candidate(&db, root).await;
    let hash = candidate.files.content_hash();
    let current = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    db.call(|sql| {
        sql.execute("UPDATE import_candidate_revision SET last_revision = 9223372036854775807 WHERE singleton = 1", [])?;
        Ok(())
    }).await.unwrap();
    let error = crate::import::CandidatePreparations::new(db.clone())
        .set_field(
            &hash,
            crate::import::CandidateEditField::AlbumTitle,
            "Rejected title",
        )
        .await
        .expect_err("an exhausted revision cannot store a changed draft");
    assert!(error.to_string().contains("exhausted"), "{error}");
    assert_eq!(
        db.load_candidate_preparation(&hash).await.unwrap().unwrap(),
        current
    );
}

#[tokio::test]
async fn replacing_a_snapshot_in_the_same_scan_invalidates_the_import_guard() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let facts = snapshot(generation, candidate.file_edit_revision);
    db.replace_candidate_file_tag_snapshot(root, &key, &facts)
        .await
        .unwrap();
    let prep = db
        .load_candidate_preparation(&candidate.files.content_hash())
        .await
        .unwrap()
        .unwrap();
    let guard = ImportCommitGuard::Candidate {
        candidate_key: key.clone(),
        source: crate::import::release_candidate::ReleaseCandidate::from(candidate).source(),
        expectation: crate::import::service::ImportExpectation {
            candidate: read_preparation(&prep),
            file_tag_snapshot_revision: db
                .candidate_file_tag_snapshot_revision(root, &key)
                .await
                .unwrap(),
        },
    };
    let accepted = guard.clone();
    db.call(move |sql| {
        // The guard runs within the finalizer's write transaction.
        sql.execute(
            "UPDATE import_candidate_state SET folder_path = folder_path",
            [],
        )?;
        super::super::import_state::require_import_commit_guard(sql, &accepted)
    })
    .await
    .unwrap();
    db.replace_candidate_file_tag_snapshot(root, &key, &facts)
        .await
        .unwrap();
    let error = db
        .call(move |sql| {
            // The guard runs within the finalizer's write transaction.
            sql.execute(
                "UPDATE import_candidate_state SET folder_path = folder_path",
                [],
            )?;
            super::super::import_state::require_import_commit_guard(sql, &guard)
        })
        .await
        .expect_err("replacing the stored reading retires the accepted snapshot identity");
    assert!(
        error.to_string().contains("file-tag reading changed"),
        "{error}"
    );
}

#[tokio::test]
async fn snapshot_identity_can_be_read_without_loading_tag_facts() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    db.replace_candidate_file_tag_snapshot(root, &key, &snapshot(generation, 0))
        .await
        .unwrap();
    let revision = db
        .candidate_file_tag_snapshot_revision(root, &key)
        .await
        .unwrap()
        .unwrap();
    db.call(|sql| {
        sql.execute("UPDATE scan_candidate_file_tag SET year = -1", [])?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(
        db.candidate_file_tag_snapshot_revision(root, &key)
            .await
            .unwrap(),
        Some(revision)
    );
    assert!(
        db.load_candidate_file_tag_snapshot_at_revision(root, &key, revision)
            .await
            .is_err(),
        "execution still validates the stored bytes after admission reads their identity"
    );
}

#[tokio::test]
async fn changed_snapshot_is_rejected_before_execution_loads_its_bytes() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let facts = snapshot(generation, 0);
    db.replace_candidate_file_tag_snapshot(root, &key, &facts)
        .await
        .unwrap();
    let revision = db
        .candidate_file_tag_snapshot_revision(root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        db.load_candidate_file_tag_snapshot_at_revision(root, &key, revision)
            .await
            .unwrap(),
        facts
    );
    db.replace_candidate_file_tag_snapshot(root, &key, &facts)
        .await
        .unwrap();
    let error = db
        .load_candidate_file_tag_snapshot_at_revision(root, &key, revision)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("file-tag reading changed"),
        "{error}"
    );
}

#[tokio::test]
async fn unchanged_rescan_preserves_an_accepted_file_tag_snapshot() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    let root = root.to_str().unwrap();
    let (candidate, generation) = scanned_candidate(&db, root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let facts = snapshot(generation, candidate.file_edit_revision);
    db.replace_candidate_file_tag_snapshot(root, &key, &facts)
        .await
        .unwrap();
    let revision = db
        .candidate_file_tag_snapshot_revision(root, &key)
        .await
        .unwrap()
        .unwrap();
    let preparation = db
        .load_candidate_preparation(&candidate.files.content_hash())
        .await
        .unwrap()
        .unwrap();
    let guard = ImportCommitGuard::Candidate {
        candidate_key: key.clone(),
        source: crate::import::release_candidate::ReleaseCandidate::from(candidate.clone())
            .source(),
        expectation: crate::import::service::ImportExpectation {
            candidate: read_preparation(&preparation),
            file_tag_snapshot_revision: Some(revision),
        },
    };

    let next_generation = db.begin_folder_scan(root).await.unwrap();
    assert_ne!(next_generation, generation);
    db.save_folder_scan_item(root, next_generation, &ScanItem::Valid(candidate))
        .await
        .unwrap();
    db.finish_folder_scan(root, next_generation, None)
        .await
        .unwrap();

    assert_eq!(
        db.candidate_file_tag_snapshot_revision(root, &key)
            .await
            .unwrap(),
        None,
        "new admission still requires a reading from the current scan"
    );
    let accepted_read = db
        .load_candidate_file_tag_snapshot_at_revision(root, &key, revision)
        .await;
    let accepted_commit = db
        .call(move |sql| {
            // The guard runs within the finalizer's write transaction.
            sql.execute(
                "UPDATE import_candidate_state SET folder_path = folder_path",
                [],
            )?;
            super::super::import_state::require_import_commit_guard(sql, &guard)
        })
        .await;
    assert_eq!(
        accepted_read.expect("an unchanged rewalk preserves the accepted reading"),
        facts
    );
    accepted_commit.expect("an unchanged rewalk preserves the accepted finalization guard");
}
