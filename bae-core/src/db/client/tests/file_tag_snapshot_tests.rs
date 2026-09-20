use super::super::*;
use super::watched_root;
use crate::import::file_tag_snapshot::{
    EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot,
};
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, InvalidCandidate, InvalidReason,
    ReleaseFileScope, ScanItem, ScannedFile,
};
use crate::util::content_type::ContentType;
use std::path::PathBuf;

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
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();

    let empty = db
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(empty.scan_generation, generation);
    assert_eq!(empty.candidate.file_edit_revision(), 0);
    assert_eq!(empty.snapshot, None);

    let expected = snapshot(generation, 0);
    assert!(db
        .replace_candidate_file_tag_snapshot(&root, &key, &expected)
        .await
        .unwrap());

    let loaded = db
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.scan_generation, generation);
    assert_eq!(loaded.candidate, candidate.into());
    assert_eq!(loaded.snapshot, Some(expected));
}

#[tokio::test]
async fn replacement_removes_every_prior_file_and_embedded_cover() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    db.replace_candidate_file_tag_snapshot(&root, &key, &snapshot(generation, 0))
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
        .replace_candidate_file_tag_snapshot(&root, &key, &replacement)
        .await
        .unwrap());

    let loaded = db
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.snapshot, Some(replacement));
}

/// A scan that finds the same files carries the stored reading forward with
/// the row it hangs off — it re-read the very files that reading was taken
/// from. A write still stamped with the generation before it is refused.
#[tokio::test]
async fn a_rescan_carries_the_reading_forward_and_refuses_an_older_stamp() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, first_generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let stored = snapshot(first_generation, 0);
    db.replace_candidate_file_tag_snapshot(&root, &key, &stored)
        .await
        .unwrap();

    let current_generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, current_generation, &ScanItem::Valid(candidate))
        .await
        .unwrap();
    db.finish_folder_scan(&root, current_generation, None)
        .await
        .unwrap();

    let loaded = db
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.scan_generation, current_generation);
    assert_eq!(loaded.candidate.file_edit_revision(), 0);
    let carried = loaded
        .snapshot
        .clone()
        .expect("the reading is still stored");
    assert_eq!(carried.scan_generation, current_generation);
    assert_eq!(carried.files, stored.files);

    assert!(!db
        .replace_candidate_file_tag_snapshot(&root, &key, &stored)
        .await
        .unwrap());
    assert_eq!(
        db.load_candidate_file_tag_snapshot(&root, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        Some(carried)
    );
}

#[tokio::test]
async fn stale_file_edit_revision_cannot_replace_snapshot() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let stored = snapshot(generation, 0);
    db.replace_candidate_file_tag_snapshot(&root, &key, &stored)
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
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.candidate.file_edit_revision(), 1);
    assert_eq!(loaded.snapshot, Some(stored.clone()));
    assert!(!db
        .replace_candidate_file_tag_snapshot(&root, &key, &stored)
        .await
        .unwrap());
}

#[tokio::test]
async fn failed_whole_replacement_preserves_the_previous_snapshot() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let stored = snapshot(generation, 0);
    db.replace_candidate_file_tag_snapshot(&root, &key, &stored)
        .await
        .unwrap();

    let mut invalid = stored.clone();
    invalid.files[0].observation.modified_at_ns = -1;
    assert!(db
        .replace_candidate_file_tag_snapshot(&root, &key, &invalid)
        .await
        .is_err());

    assert_eq!(
        db.load_candidate_file_tag_snapshot(&root, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        Some(stored)
    );
}

/// How many file-tag rows the candidate at `key` holds: the reading's own row
/// and one row per file it was read from.
async fn stored_reading_rows(db: &Database, root: &str, key: &str) -> (i64, i64) {
    let root = root.to_string();
    let key = key.to_string();
    db.read(move |sql| {
        let readings = sql.query_row(
            "SELECT count(*) FROM scan_candidate_tag_snapshot \
             WHERE watched_folder_path = ? AND candidate_path = ?",
            params![root, key],
            |row| row.get::<_, i64>(0),
        )?;
        let facts = sql.query_row(
            "SELECT count(*) FROM scan_candidate_file_tag \
             WHERE watched_folder_path = ? AND candidate_path = ?",
            params![root, key],
            |row| row.get::<_, i64>(0),
        )?;
        Ok((readings, facts))
    })
    .await
    .unwrap()
}

/// A folder that fails validation on a later pass — a corrupt image beside its
/// audio — is stored as an invalid candidate, which carries no files at all.
/// The reading stored for the release it used to be was taken from files that
/// row does not have, so it goes with the row it belonged to.
#[tokio::test]
async fn a_candidate_that_turns_invalid_drops_the_reading_it_carried() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    db.replace_candidate_file_tag_snapshot(&root, &key, &snapshot(generation, 0))
        .await
        .unwrap();
    assert_eq!(stored_reading_rows(&db, &root, &key).await, (1, 2));

    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(
        &root,
        generation,
        &ScanItem::Invalid(InvalidCandidate {
            path: candidate.path.clone(),
            name: candidate.name.clone(),
            watched_folder_path: root.clone(),
            display_path: candidate.display_path.clone(),
            resolved_boundaries: Vec::new(),
            reason: InvalidReason::CorruptImage {
                path: "front.jpg".to_string(),
            },
        }),
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    assert_eq!(stored_reading_rows(&db, &root, &key).await, (0, 0));
}

/// The same rule where the folder stays a release: a pass that finds different
/// audio than the reading was taken from stores the files it found, and the
/// reading, which describes files that are no longer the candidate's, does not
/// come across with it.
#[tokio::test]
async fn a_rescan_that_finds_other_audio_drops_the_reading_it_carried() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    db.replace_candidate_file_tag_snapshot(&root, &key, &snapshot(generation, 0))
        .await
        .unwrap();
    assert_eq!(stored_reading_rows(&db, &root, &key).await, (1, 2));

    let mut without_second_file = candidate.clone();
    without_second_file.files.files.pop();
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, generation, &ScanItem::Valid(without_second_file))
        .await
        .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    assert_eq!(stored_reading_rows(&db, &root, &key).await, (0, 0));
}

fn save_expectation(
    candidate: &FolderCandidate,
    preparation: &crate::import::preparation::CandidatePreparation,
) -> crate::db::CandidateSaveExpectation {
    crate::db::CandidateSaveExpectation {
        edit_revision: preparation.file_edits.revision,
        metadata_revision: preparation.metadata_revision,
        scanned: Some(crate::db::CandidateScanExpectation::Current(
            crate::db::ScannedCandidateKey {
                watched_folder_path: candidate.watched_folder_path.clone(),
                candidate_path: candidate.path.to_string_lossy().into_owned(),
            },
        )),
    }
}

#[tokio::test]
async fn superseded_preparation_leaves_snapshot_facts_and_cover_unchanged() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let hash = candidate.files.content_hash();
    let reading = snapshot(generation, 0);
    db.replace_candidate_file_tag_snapshot(&root, &key, &reading)
        .await
        .unwrap();
    let mut stale = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    let expected = save_expectation(&candidate, &stale);
    crate::import::CandidatePreparations::new(db.clone())
        .set_field(
            &hash,
            crate::import::CandidateEditField::AlbumTitle,
            "Newer album",
        )
        .await
        .unwrap();
    let before = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    stale.metadata_revision += 1;
    let mut replacement = reading.clone();
    replacement.files[0].title = Some("Stale title".into());
    replacement.embedded_cover.as_mut().unwrap().data = vec![9, 8, 7];
    let result = db
        .save_candidate_preparation(
            stale,
            expected,
            crate::db::CandidateSaveExtras {
                file_tag_snapshot: Some(replacement),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(matches!(result, crate::db::CandidateSaved::Superseded));
    assert_eq!(
        db.load_candidate_preparation(&hash).await.unwrap().unwrap(),
        before
    );
    assert_eq!(
        db.load_candidate_file_tag_snapshot(&root, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        Some(reading)
    );
}

#[tokio::test]
async fn preparation_reshape_stores_complete_tags_after_their_file_rows() {
    let (db, _tmp, root) = watched_root().await;
    let (candidate, generation) = scanned_candidate(&db, &root).await;
    let key = candidate.path.to_string_lossy().into_owned();
    let hash = candidate.files.content_hash();
    let mut prep = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
    let expected = save_expectation(&candidate, &prep);
    prep.file_edits.revision += 1;
    prep.metadata_revision += 1;
    let reading = snapshot(generation, prep.file_edits.revision);
    let result = db
        .save_candidate_preparation(
            prep,
            expected,
            crate::db::CandidateSaveExtras {
                file_tag_snapshot: Some(reading.clone()),
                reshaped_files: Some(vec![(key.clone(), candidate.files)]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(matches!(result, crate::db::CandidateSaved::Landed(_)));
    let stored = db
        .load_candidate_file_tag_snapshot(&root, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.candidate.file_edit_revision(), 1);
    assert_eq!(stored.snapshot, Some(reading));
    assert_eq!(stored_reading_rows(&db, &root, &key).await, (1, 2));
}

#[tokio::test]
async fn preparation_save_rejects_incomplete_or_unowned_tag_readings_atomically() {
    for invalid_cover in [false, true] {
        let (db, _tmp, root) = watched_root().await;
        let (candidate, generation) = scanned_candidate(&db, &root).await;
        let key = candidate.path.to_string_lossy().into_owned();
        let hash = candidate.files.content_hash();
        let original = snapshot(generation, 0);
        db.replace_candidate_file_tag_snapshot(&root, &key, &original)
            .await
            .unwrap();
        let before = db.load_candidate_preparation(&hash).await.unwrap().unwrap();
        let expected = save_expectation(&candidate, &before);
        let mut prep = before.clone();
        prep.metadata_revision += 1;
        let mut invalid = original.clone();
        if invalid_cover {
            invalid
                .embedded_cover
                .as_mut()
                .unwrap()
                .source_relative_path = "unread.flac".into();
        } else {
            invalid.files.pop();
        }
        assert!(db
            .save_candidate_preparation(
                prep,
                expected,
                crate::db::CandidateSaveExtras {
                    file_tag_snapshot: Some(invalid),
                    ..Default::default()
                }
            )
            .await
            .is_err());
        assert_eq!(
            db.load_candidate_preparation(&hash).await.unwrap().unwrap(),
            before
        );
        assert_eq!(
            db.load_candidate_file_tag_snapshot(&root, &key)
                .await
                .unwrap()
                .unwrap()
                .snapshot,
            Some(original)
        );
    }
}
