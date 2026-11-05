//! A candidate's file-tag snapshot as stored, replaced and dropped.

use crate::file_tag_snapshot::{EmbeddedCoverFact, FileObservation, FileTagFact, FileTagSnapshot};
use crate::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, InvalidCandidate, ReleaseFileScope,
    ScanItem, ScannedFile,
};
use crate::tables::tests::*;
use bae_store::Store;
use bae_util::content_type::ContentType;
use rusqlite::params;

pub(crate) fn tagged_candidate(root: &std::path::Path) -> FolderCandidate {
    let candidate_path = root.join("candidate-a");
    let files = [("01.flac", 1_000), ("02.flac", 2_000)]
        .into_iter()
        .map(|(relative_path, size)| CandidateFile {
            file: ScannedFile::new(
                candidate_path.join(relative_path),
                relative_path.to_string(),
                size,
                1,
            )
            .with_test_flac_audio(),
            role: FileRole::Audio,
        })
        .collect();
    FolderCandidate {
        path: candidate_path.clone(),
        file_root: candidate_path,
        name: "Candidate A".to_string(),
        files: CategorizedFiles { files, parts: Vec::new() },
        watched_folder_path: root.to_path_buf(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::test("candidate-a"),
        grouping: None,
    }
}

pub(crate) async fn scanned_tagged_candidate(
    db: &Store,
    root: &std::path::Path,
) -> (FolderCandidate, u64) {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let candidate = tagged_candidate(root);
    tables
        .save_folder_scan_item(root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    (candidate, generation)
}

pub(crate) fn tag_snapshot(generation: u64, revision: u64) -> FileTagSnapshot {
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
                tags: crate::file_tag_snapshot::FileTags {
                    title: Some("Track Title A".to_string()),
                    track_artists: vec!["Artist Name".to_string()],
                    album_title: Some("Album Title".to_string()),
                    album_artists: vec!["Album Artist".to_string()],
                    compilation: false,
                    years: vec![2020],
                    numbers: crate::track_numbers::TrackNumbers { disc: Some(1), track: Some(1) },
                },
            },
            FileTagFact {
                observation: FileObservation {
                    relative_path: "02.flac".to_string(),
                    size: 2_000,
                    modified_at_ns: 200,
                },
                tags: crate::file_tag_snapshot::FileTags {
                    title: Some("Track Title B".to_string()),
                    track_artists: Vec::new(),
                    album_title: None,
                    album_artists: Vec::new(),
                    compilation: false,
                    years: Vec::new(),
                    numbers: crate::track_numbers::TrackNumbers { disc: None, track: Some(2) },
                },
            },
        ],
        embedded_cover: Some(EmbeddedCoverFact {
            source_relative_path: "01.flac".to_string(),
            content_type: ContentType::Png,
            data: vec![1, 2, 3, 4],
        }),
    }
}

/// How many file-tag rows the candidate at `key` holds: the reading's own row
/// and one row per file it was read from.
pub(crate) async fn stored_reading_rows(
    db: &Store,
    root: &std::path::Path,
    key: &crate::CandidateKey,
) -> (i64, i64) {
    let root = coven::path_bytes(root);
    let key = key.clone();
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

#[tokio::test]
async fn file_tag_snapshot_round_trips_with_current_candidate_stamp() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);

    let empty = tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap();
    assert_eq!(empty.scan_generation, generation);
    assert_eq!(empty.candidate.file_edit_revision, 0);
    assert_eq!(empty.snapshot, None);

    let expected = tag_snapshot(generation, 0);
    assert!(tables.replace_candidate_file_tag_snapshot(&root, &key, &expected).await.unwrap());

    let loaded = tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap();
    assert_eq!(loaded.scan_generation, generation);
    assert_eq!(loaded.candidate, candidate);
    assert_eq!(loaded.snapshot, Some(expected));
}

#[tokio::test]
async fn replacement_removes_every_prior_file_and_embedded_cover() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    tables
        .replace_candidate_file_tag_snapshot(&root, &key, &tag_snapshot(generation, 0))
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
                tags: crate::file_tag_snapshot::FileTags {
                    title: None,
                    track_artists: Vec::new(),
                    album_title: None,
                    album_artists: Vec::new(),
                    compilation: false,
                    years: Vec::new(),
                    numbers: crate::track_numbers::TrackNumbers { disc: None, track: None },
                },
            },
            FileTagFact {
                observation: FileObservation {
                    relative_path: "02.flac".to_string(),
                    size: 2_000,
                    modified_at_ns: 400,
                },
                tags: crate::file_tag_snapshot::FileTags {
                    title: None,
                    track_artists: Vec::new(),
                    album_title: None,
                    album_artists: Vec::new(),
                    compilation: false,
                    years: Vec::new(),
                    numbers: crate::track_numbers::TrackNumbers { disc: None, track: None },
                },
            },
        ],
        embedded_cover: None,
    };
    assert!(tables.replace_candidate_file_tag_snapshot(&root, &key, &replacement).await.unwrap());

    let loaded = tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap();
    assert_eq!(loaded.snapshot, Some(replacement));
}

/// A scan that finds the same files carries the stored reading forward with
/// the row it hangs off — it re-read the very files that reading was taken
/// from. A write still stamped with the generation before it is refused.
#[tokio::test]
async fn a_rescan_carries_the_reading_forward_and_refuses_an_older_stamp() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, first_generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    let stored = tag_snapshot(first_generation, 0);
    tables.replace_candidate_file_tag_snapshot(&root, &key, &stored).await.unwrap();

    let current_generation =
        tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, current_generation, &ScanItem::Valid(candidate))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, current_generation, None).await.unwrap();

    let loaded = tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap();
    assert_eq!(loaded.scan_generation, current_generation);
    assert_eq!(loaded.candidate.file_edit_revision, 0);
    let carried = loaded.snapshot.clone().expect("the reading is still stored");
    assert_eq!(carried.scan_generation, current_generation);
    assert_eq!(carried.files, stored.files);

    assert!(!tables.replace_candidate_file_tag_snapshot(&root, &key, &stored).await.unwrap());
    assert_eq!(
        tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap().snapshot,
        Some(carried)
    );
}

#[tokio::test]
async fn stale_file_edit_revision_cannot_replace_snapshot() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    let stored = tag_snapshot(generation, 0);
    tables.replace_candidate_file_tag_snapshot(&root, &key, &stored).await.unwrap();

    let root_owned = coven::path_bytes(&root);
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

    let loaded = tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap();
    assert_eq!(loaded.candidate.file_edit_revision, 1);
    assert_eq!(loaded.snapshot, Some(stored.clone()));
    assert!(!tables.replace_candidate_file_tag_snapshot(&root, &key, &stored).await.unwrap());
}

#[tokio::test]
async fn failed_whole_replacement_preserves_the_previous_snapshot() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    let stored = tag_snapshot(generation, 0);
    tables.replace_candidate_file_tag_snapshot(&root, &key, &stored).await.unwrap();

    let mut invalid = stored.clone();
    invalid.files[0].observation.modified_at_ns = -1;
    assert!(tables.replace_candidate_file_tag_snapshot(&root, &key, &invalid).await.is_err());

    assert_eq!(
        tables.load_candidate_file_tag_snapshot(&root, &key).await.unwrap().unwrap().snapshot,
        Some(stored)
    );
}

/// A folder that fails validation on a later pass — a zero-byte track among
/// its audio — is stored as an invalid candidate, which carries no files at all.
/// The reading stored for the release it used to be was taken from files that
/// row does not have, so it goes with the row it belonged to.
#[tokio::test]
async fn a_candidate_that_turns_invalid_drops_the_reading_it_carried() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    tables
        .replace_candidate_file_tag_snapshot(&root, &key, &tag_snapshot(generation, 0))
        .await
        .unwrap();
    assert_eq!(stored_reading_rows(&db, &root, &key).await, (1, 2));

    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(
            &root,
            generation,
            &ScanItem::Invalid(InvalidCandidate {
                path: candidate.path.clone(),
                name: candidate.name.clone(),
                watched_folder_path: root.clone(),
                relative_folder: candidate.relative_folder.clone(),
                grouping: None,
                files: vec![crate::folder_scanner::SkippedFile::empty_track(
                    &candidate.path,
                    "02.flac",
                )],
                tags: None,
            }),
        )
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    assert_eq!(stored_reading_rows(&db, &root, &key).await, (0, 0));
}

/// The same rule where the folder stays a release: a pass that finds different
/// audio than the reading was taken from stores the files it found, and the
/// reading, which describes files that are no longer the candidate's, does not
/// come across with it.
#[tokio::test]
async fn a_rescan_that_finds_other_audio_drops_the_reading_it_carried() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (candidate, generation) = scanned_tagged_candidate(&db, &root).await;
    let key = crate::CandidateKey::folder(&candidate.path);
    tables
        .replace_candidate_file_tag_snapshot(&root, &key, &tag_snapshot(generation, 0))
        .await
        .unwrap();
    assert_eq!(stored_reading_rows(&db, &root, &key).await, (1, 2));

    let mut without_second_file = candidate.clone();
    without_second_file.files.files.pop();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(without_second_file))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    assert_eq!(stored_reading_rows(&db, &root, &key).await, (0, 0));
}
