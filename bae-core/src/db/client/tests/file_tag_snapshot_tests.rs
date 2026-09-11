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
