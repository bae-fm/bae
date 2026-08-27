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
            ),
            role: FileRole::Audio,
            proposed_audio: true,
        })
        .collect();
    FolderCandidate {
        path: PathBuf::from(&candidate_path),
        file_root: PathBuf::from(&candidate_path),
        name: "Candidate A".to_string(),
        files: CategorizedFiles {
            files,
            format_label: "FLAC".to_string(),
        },
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
                content_type: Some(ContentType::Flac),
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
                content_type: None,
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
    assert_eq!(empty.candidate.file_edit_revision, 0);
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
    assert_eq!(loaded.candidate, candidate);
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
                content_type: Some(ContentType::Other("audio/example".to_string())),
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
                content_type: None,
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
    assert_eq!(loaded.candidate.file_edit_revision, 0);
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
    assert_eq!(loaded.candidate.file_edit_revision, 1);
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
