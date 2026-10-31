//! Releases picked together, as the store keeps them: built from the releases
//! they take in, in the same write as whatever changes one of them.

use super::{candidate, empty_db};
use crate::import::folder_scanner::{
    AudioDamage, CandidateFile, FileRole, FolderCandidate, FolderSidecar, InvalidReason,
    ProposedRole, ScanItem, ScannedFile, SidecarFiles, SkippedFile,
};
use crate::import::grouping::GroupingBlock;
use crate::import::watched_folder::host_root;

/// `names` under one watched root, scanned to completion.
async fn scanned(
    names: &[&str],
) -> (
    super::super::Database,
    tempfile::TempDir,
    Vec<FolderCandidate>,
) {
    let root = host_root("/music");
    scanned_folders(names.iter().map(|name| candidate(&root, name)).collect()).await
}

/// `folders`, under the watched root `/music`, scanned to completion.
async fn scanned_folders(
    folders: Vec<FolderCandidate>,
) -> (
    super::super::Database,
    tempfile::TempDir,
    Vec<FolderCandidate>,
) {
    let (db, temp) = empty_db().await;
    let root = host_root("/music");
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let mut candidates = Vec::new();
    for mut folder in folders {
        // A file of its own per folder, so no two share a path on disk.
        for entry in &mut folder.files.files {
            entry.file.path = folder.path.join(&entry.file.relative_path);
        }
        db.save_folder_scan_item(&root, generation, &ScanItem::Valid(folder.clone()))
            .await
            .unwrap()
            .expect("the scan generation is current");
        candidates.push(folder);
    }
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    (db, temp, candidates)
}

async fn stored(
    db: &super::super::Database,
    key: &crate::import::CandidateKey,
) -> Option<ScanItem> {
    db.load_folder_scan_item(key).await.unwrap()
}

/// A release that one of a grouping's releases changes under is rebuilt in
/// the write that changes it: the grouping never lists files its release no
/// longer holds.
#[tokio::test]
async fn a_changed_release_rebuilds_the_grouping_in_the_same_write() {
    let (db, _temp, members) = scanned(&["Volume A", "Volume B"]).await;
    let root = host_root("/music");
    db.combine_releases(
        crate::import::CandidateKey::grouping("test"),
        members.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    let Some(ScanItem::Valid(before)) =
        stored(&db, &crate::import::CandidateKey::grouping("test")).await
    else {
        panic!("the grouping built its release");
    };

    let mut changed = members[1].clone();
    changed.files.files[0].file.size += 1;
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let write = db
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(changed.clone()))
        .await
        .unwrap()
        .unwrap();
    let rebuilt = write.regrouped().expect("the write stored the change");
    assert_eq!(rebuilt.written.len(), 1);
    let Some(ScanItem::Valid(after)) =
        stored(&db, &crate::import::CandidateKey::grouping("test")).await
    else {
        panic!("the grouping still has its release");
    };
    assert_ne!(after.files.content_hash(), before.files.content_hash());
    assert!(after
        .files
        .release_files()
        .any(|file| file.size == changed.files.files[0].file.size));
}

/// A release a grouping takes in that goes away leaves the grouping's release
/// as it was last built, saying why it cannot be imported, until the release
/// comes back or the grouping is undone.
#[tokio::test]
async fn a_grouping_missing_one_of_its_releases_says_so_and_can_be_undone() {
    let (db, _temp, members) = scanned(&["Volume A", "Volume B"]).await;
    let root = host_root("/music");
    db.combine_releases(
        crate::import::CandidateKey::grouping("test"),
        members.clone(),
    )
    .await
    .unwrap()
    .unwrap();

    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(&root, generation, &ScanItem::Valid(members[0].clone()))
        .await
        .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let error = db
        .load_release_candidate(&crate::import::CandidateKey::grouping("test"))
        .await
        .unwrap()
        .expect_err("a grouping missing a release cannot be worked on");
    assert!(error.to_string().contains("Volume B"), "{error}");
    let detail = db
        .load_import_candidate(&crate::import::CandidateKey::grouping("test"))
        .await
        .unwrap()
        .expect("the release is still listed");
    assert!(!detail.actionable);
    assert!(
        detail
            .resolve()
            .live
            .actions
            .contains(&crate::import::CandidateAction::Separate),
        "a grouping that cannot be worked on is still read apart again"
    );

    let (returned, _) = db
        .separate_picked_grouping(&crate::import::CandidateKey::grouping("test"))
        .await
        .unwrap();
    assert_eq!(returned, vec![ScanItem::Valid(members[0].clone())]);
    assert!(stored(&db, &crate::import::CandidateKey::grouping("test"))
        .await
        .is_none());
}

/// Two folders whose names show the same once escaped — a Latin-1 `Café`
/// and a folder spelled `Caf\xE9` — combine into a release two of whose
/// files show one name, which no release can hold: it is stored as one that
/// cannot be imported, saying so.
#[cfg(unix)]
#[tokio::test]
async fn folders_whose_names_show_the_same_combine_into_no_importable_release() {
    use std::os::unix::ffi::OsStrExt;
    let root = host_root("/music");
    let named = |name: &std::ffi::OsStr| {
        let name = std::path::Path::new(name);
        let mut folder = candidate(&root, "Album");
        folder.path = root.join(name);
        folder.file_root = folder.path.clone();
        folder.name = coven::path_text(name);
        folder.relative_folder = crate::import::RelativeFolder::of(name).unwrap();
        folder
    };
    let (db, _temp, members) = scanned_folders(vec![
        named(std::ffi::OsStr::from_bytes(b"Caf\xE9")),
        named(std::ffi::OsStr::from_bytes(b"Caf\\xE9")),
    ])
    .await;
    let key = crate::import::CandidateKey::grouping("test");
    db.combine_releases(key.clone(), members)
        .await
        .unwrap()
        .unwrap();
    let Some(ScanItem::Invalid(release)) = stored(&db, &key).await else {
        panic!("the grouping's release cannot be imported");
    };
    assert_eq!(
        release.reason(),
        InvalidReason::SameName {
            name: "Caf\\xE9/01.flac".to_string()
        }
    );
}

/// A folder already in one grouping cannot be taken into another.
#[tokio::test]
async fn a_release_is_taken_into_one_grouping_at_most() {
    let (db, _temp, members) = scanned(&["Volume A", "Volume B", "Volume C"]).await;
    db.combine_releases(
        crate::import::CandidateKey::grouping("first"),
        members[..2].to_vec(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(db
        .combine_releases(
            crate::import::CandidateKey::grouping("second"),
            members[1..].to_vec()
        )
        .await
        .is_err());
}

/// One grouping table family holds every reading of folders as releases:
/// the two it replaced are gone from the schema.
#[tokio::test]
async fn the_schema_holds_one_grouping_model() {
    let (db, _temp) = empty_db().await;
    let tables: Vec<String> = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger')",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    for gone in [
        "folder_release_decisions",
        "candidate_combination",
        "candidate_combination_member",
        "scan_candidate_resolved_boundary",
    ] {
        assert!(
            !tables.iter().any(|table| table == gone),
            "{gone} is still in the schema"
        );
    }
    for kept in ["release_grouping", "release_grouping_member"] {
        assert!(
            tables.iter().any(|table| table == kept),
            "{kept} is missing"
        );
    }
}

// ── The files of the folder a grouping's releases sit in ────────────────────

/// The sidecar of `folder` under `/music`, holding `names` as artwork.
fn sidecar_of(folder: &str, names: &[&str]) -> FolderSidecar {
    let root = host_root("/music");
    let folder = std::path::PathBuf::from(&root).join(folder);
    FolderSidecar {
        watched_folder_path: root,
        files: SidecarFiles::Valid(
            names
                .iter()
                .map(|name| CandidateFile {
                    file: ScannedFile::new(folder.join(name), name.to_string(), 10, 1),
                    role: FileRole::Artwork,
                })
                .collect(),
        ),
        folder,
    }
}

/// Store `item` under a scan generation opened for it, and say what the
/// write rebuilt.
async fn scan_in(db: &super::super::Database, item: ScanItem) -> crate::db::ScanItemWrite {
    let root = host_root("/music");
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(&root, generation, &item)
        .await
        .unwrap()
        .expect("the scan generation is current")
}

/// The relative paths of the files the release at `key` holds, when it can be
/// imported.
async fn release_files(
    db: &super::super::Database,
    key: &crate::import::CandidateKey,
) -> Vec<String> {
    let Some(ScanItem::Valid(release)) = stored(db, key).await else {
        panic!("{key} is a release that can be imported");
    };
    release
        .files
        .files
        .iter()
        .map(|entry| entry.file.relative_path.clone())
        .collect()
}

/// Two discs picked out of three in one album folder read the album's own
/// files, which follow the folder as it changes; discs from different folders
/// read none; and the album's files go with one release at most.
#[tokio::test]
async fn releases_in_one_folder_read_its_files_and_only_one_grouping_does() {
    let (db, _temp, members) = scanned(&[
        "Album/Disc 1",
        "Album/Disc 2",
        "Album/Disc 3",
        "Album/Disc 4",
        "Other/Disc 1",
    ])
    .await;
    scan_in(&db, ScanItem::Sidecar(sidecar_of("Album", &["cover.jpg"]))).await;

    db.combine_releases(
        crate::import::CandidateKey::grouping("discs"),
        members[..2].to_vec(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("discs")).await,
        ["cover.jpg", "Disc 1/01.flac", "Disc 2/01.flac"]
    );

    let write = scan_in(
        &db,
        ScanItem::Sidecar(sidecar_of("Album", &["booklet.pdf", "cover.jpg"])),
    )
    .await;
    assert_eq!(write.regrouped().unwrap().written.len(), 1);
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("discs")).await,
        [
            "booklet.pdf",
            "cover.jpg",
            "Disc 1/01.flac",
            "Disc 2/01.flac"
        ]
    );

    let error = db
        .combine_releases(
            crate::import::CandidateKey::grouping("more"),
            members[2..4].to_vec(),
        )
        .await
        .unwrap()
        .expect_err("the album's files already go with a release");
    assert!(
        matches!(&error, GroupingBlock::FolderFilesTaken { folder, .. } if folder == "Album"),
        "{error}"
    );

    db.combine_releases(
        crate::import::CandidateKey::grouping("apart"),
        vec![members[2].clone(), members[4].clone()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("apart")).await,
        ["Album/Disc 3/01.flac", "Other/Disc 1/01.flac"]
    );

    // A zero-byte track among the folder's files makes the release that
    // reads them one that cannot be imported, as a folder holding it would be.
    let album = host_root("/music").join("Album");
    let broken = FolderSidecar {
        files: SidecarFiles::Invalid(vec![SkippedFile {
            relative_path: "bonus.flac".into(),
            file: crate::import::folder_scanner::FileVersion {
                path: album.join("bonus.flac"),
                size: 0,
                modified_at_ns: 1,
            },
            role: ProposedRole::DamagedAudio {
                damage: AudioDamage::Empty,
            },
        }]),
        ..sidecar_of("Album", &[])
    };
    scan_in(&db, ScanItem::Sidecar(broken)).await;
    let Some(ScanItem::Invalid(invalid)) =
        stored(&db, &crate::import::CandidateKey::grouping("discs")).await
    else {
        panic!("the release reading the broken files is invalid");
    };
    assert_eq!(invalid.reason(), InvalidReason::DamagedAudio { count: 1 });
    assert!(
        invalid
            .files
            .iter()
            .any(|file| file.relative_path == "bonus.flac"),
        "the release lists the folder's broken file among its own: {:?}",
        invalid.files
    );
}

/// A release that reads a folder's own files takes them from the sidecar:
/// the sidecar goes, and the grouping that read it is rebuilt without them.
/// A sidecar written again takes them back from a stale release there.
#[tokio::test]
async fn a_release_reading_the_folders_files_replaces_its_sidecar() {
    let (db, _temp, members) = scanned(&["Album/Disc 1", "Album/Disc 2"]).await;
    scan_in(&db, ScanItem::Sidecar(sidecar_of("Album", &["cover.jpg"]))).await;
    db.combine_releases(
        crate::import::CandidateKey::grouping("discs"),
        members.clone(),
    )
    .await
    .unwrap()
    .unwrap();

    let album = super::candidate_with(
        &host_root("/music"),
        "Album",
        sidecar_owner_files(),
        crate::import::folder_scanner::ReleaseFileScope::Direct,
    );
    let write = scan_in(&db, ScanItem::Valid(album.clone())).await;
    assert_eq!(write.regrouped().unwrap().written.len(), 1);
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("discs")).await,
        ["Disc 1/01.flac", "Disc 2/01.flac"]
    );

    let write = scan_in(&db, ScanItem::Sidecar(sidecar_of("Album", &["cover.jpg"]))).await;
    assert_eq!(write.superseded_keys(), [album.key()]);
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("discs")).await,
        ["cover.jpg", "Disc 1/01.flac", "Disc 2/01.flac"]
    );
}

/// The files of a release at the album folder itself: one track and the cover.
fn sidecar_owner_files() -> crate::import::folder_scanner::CategorizedFiles {
    let folder = host_root("/music").join("Album");
    crate::import::folder_scanner::CategorizedFiles {
        files: vec![
            CandidateFile {
                file: ScannedFile::new(folder.join("00.flac"), "00.flac".into(), 1_000, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
            },
            CandidateFile {
                file: ScannedFile::new(folder.join("cover.jpg"), "cover.jpg".into(), 10, 1),
                role: FileRole::Artwork,
            },
        ],
        parts: Vec::new(),
    }
}

/// A grouping whose releases come to sit in a folder whose files another
/// grouping already reads is blocked, saying so, rather than reading them a
/// second time; separating the other lets it read them.
#[tokio::test]
async fn a_grouping_that_comes_to_read_taken_files_waits_for_them() {
    let (db, _temp, members) = scanned(&[
        "Album/Disc 1",
        "Album/Disc 2",
        "Album/Disc 3/Audio",
        "Album/Disc 4/Audio",
    ])
    .await;
    scan_in(&db, ScanItem::Sidecar(sidecar_of("Album", &["cover.jpg"]))).await;
    db.combine_releases(
        crate::import::CandidateKey::grouping("first"),
        members[..2].to_vec(),
    )
    .await
    .unwrap()
    .unwrap();
    // Each of these sits in a disc folder of its own, so none is shared.
    db.combine_releases(
        crate::import::CandidateKey::grouping("second"),
        members[2..].to_vec(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        !release_files(&db, &crate::import::CandidateKey::grouping("second"))
            .await
            .contains(&"cover.jpg".to_string())
    );

    // The disc folders gain files of their own, which the releases below them
    // take in: now both sit directly in the album folder.
    for member in &members[2..] {
        let mut lent = member.clone();
        lent.file_root = lent.path.parent().unwrap().to_path_buf();
        scan_in(&db, ScanItem::Valid(lent)).await;
    }
    let error = db
        .load_release_candidate(&crate::import::CandidateKey::grouping("second"))
        .await
        .unwrap()
        .expect_err("the album's files already go with the first release");
    assert!(
        matches!(&error, GroupingBlock::FolderFilesTaken { folder, .. } if folder == "Album"),
        "{error}"
    );
    assert!(
        release_files(&db, &crate::import::CandidateKey::grouping("first"))
            .await
            .contains(&"cover.jpg".to_string())
    );

    let (_, regrouped) = db
        .separate_picked_grouping(&crate::import::CandidateKey::grouping("first"))
        .await
        .unwrap();
    assert_eq!(regrouped.written.len(), 1);
    assert!(db
        .load_release_candidate(&crate::import::CandidateKey::grouping("second"))
        .await
        .unwrap()
        .unwrap()
        .is_some());
    assert!(
        release_files(&db, &crate::import::CandidateKey::grouping("second"))
            .await
            .contains(&"cover.jpg".to_string())
    );
}

/// Groupings sitting in a folder with no files of its own contend for
/// nothing, so any number may. Once the folder has files, none of them reads
/// them while several sit there — each is blocked, saying so — and separating
/// one lets the other read them.
#[tokio::test]
async fn groupings_contend_only_for_files_that_are_there() {
    let (db, _temp, members) = scanned(&["Box/CD1", "Box/CD2", "Box/CD3", "Box/CD4"]).await;
    db.combine_releases(
        crate::import::CandidateKey::grouping("first"),
        members[..2].to_vec(),
    )
    .await
    .unwrap()
    .unwrap();
    db.combine_releases(
        crate::import::CandidateKey::grouping("second"),
        members[2..].to_vec(),
    )
    .await
    .unwrap()
    .unwrap();

    let write = scan_in(&db, ScanItem::Sidecar(sidecar_of("Box", &["cover.jpg"]))).await;
    assert!(write.regrouped().unwrap().written.is_empty());
    for key in [
        crate::import::CandidateKey::grouping("first"),
        crate::import::CandidateKey::grouping("second"),
    ] {
        let error = db
            .load_release_candidate(&key)
            .await
            .unwrap()
            .expect_err("the box's files would go with two releases");
        assert_eq!(
            error,
            GroupingBlock::FolderFilesContested {
                folder: "Box".into()
            }
        );
    }

    let (_, regrouped) = db
        .separate_picked_grouping(&crate::import::CandidateKey::grouping("first"))
        .await
        .unwrap();
    assert_eq!(regrouped.written.len(), 1);
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("second")).await,
        ["cover.jpg", "CD3/01.flac", "CD4/01.flac"]
    );
}

/// Files still downloading into the folder hold the grouping back, as a
/// download holds back a release, until the download ends.
#[tokio::test]
async fn a_grouping_waits_for_its_folders_files_to_finish_downloading() {
    let (db, _temp, members) = scanned(&["Album/Disc 1", "Album/Disc 2"]).await;
    db.combine_releases(
        crate::import::CandidateKey::grouping("discs"),
        members.clone(),
    )
    .await
    .unwrap()
    .unwrap();
    scan_in(
        &db,
        ScanItem::Sidecar(FolderSidecar {
            files: SidecarFiles::Downloading,
            ..sidecar_of("Album", &[])
        }),
    )
    .await;
    let error = db
        .load_release_candidate(&crate::import::CandidateKey::grouping("discs"))
        .await
        .unwrap()
        .expect_err("the album's files are not known yet");
    assert_eq!(
        error,
        GroupingBlock::FolderFilesDownloading {
            folder: "Album".into()
        }
    );

    scan_in(&db, ScanItem::Sidecar(sidecar_of("Album", &["cover.jpg"]))).await;
    assert_eq!(
        release_files(&db, &crate::import::CandidateKey::grouping("discs")).await,
        ["cover.jpg", "Disc 1/01.flac", "Disc 2/01.flac"]
    );
}

/// The release reading a folder's files is found by the index of each
/// folder's one reader, not by reading every grouping in the folder.
#[tokio::test]
async fn the_folder_files_reader_is_found_by_its_index() {
    let (db, _tmp) = super::temp_db().await;
    let plan = db
        .read(|sql| {
            sql.query(
                &format!(
                    "EXPLAIN QUERY PLAN {}",
                    super::super::release_groupings::PARENT_FILES_READER
                ),
                ["/music/Album", "grouping"],
                |row| row.get::<_, String>(3),
            )
            .map_err(crate::db::DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter()
            .any(|step| step.contains("USING INDEX release_grouping_parent_reader")),
        "the reader is found by its index: {plan:?}"
    );
}

/// Reading one folder again prunes the sidecars it no longer yields at and
/// below that folder, and leaves those of the folders beside it, whatever
/// their names sort next to.
#[tokio::test]
async fn reading_a_folder_again_prunes_only_its_own_sidecars() {
    let (db, _temp, _) = scanned(&[]).await;
    let root = host_root("/music");
    for folder in ["Album", "Album/Disc 1", "Album 2", "Album0"] {
        scan_in(&db, ScanItem::Sidecar(sidecar_of(folder, &["cover.jpg"]))).await;
    }

    let stamp = db.begin_folder_reading(&root).await.unwrap();
    db.commit_folder_reading(crate::db::FolderReadingCommit {
        watched_folder_path: root.clone(),
        folder: "Album".into(),
        stamp,
        decision: None,
        scanned_decisions: Vec::new(),
        items: Vec::new(),
        directories: Some(Vec::new()),
    })
    .await
    .unwrap();

    let folder = |name: &str| coven::path_text(&root.join(name));
    assert_eq!(
        db.query_texts_for_test("SELECT coven_path_text(folder) FROM scan_sidecar ORDER BY folder")
            .await
            .unwrap(),
        [folder("Album 2"), folder("Album0")]
    );
}
