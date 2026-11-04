//! The folder-scan tables as a scan writes and reads them: generations, what
//! an item's write replaces and finds, and each root's snapshot.

use super::read::RowSources;
use super::*;
use crate::tables::tests::*;
use crate::tables::ImportTables;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;
use bae_db::Database;
use rusqlite::params;

impl ImportTables {
    /// Each watched folder's scan as a test reads it whole: its generation,
    /// status and every stored entry.
    pub async fn load_folder_scan_snapshots(&self) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
        self.database
            .read(move |sql| load_folder_scan_snapshots_on(&sql))
            .process(|process| process())
            .await
    }
}

fn load_folder_scan_snapshots_on(
    sql: &SqlReadContext<'_>,
) -> Result<impl FnOnce() -> Result<Vec<DbFolderScanSnapshot>, DbError> + Send + 'static, DbError> {
    let roots = sql.query(
        "SELECT roots.watched_folder_path, roots.generation, roots.status, roots.error, \
                COUNT(candidate.path) \
         FROM folder_scan_roots AS roots \
         LEFT JOIN scan_candidate AS candidate \
           ON candidate.watched_folder_path = roots.watched_folder_path \
          AND candidate.generation = roots.generation \
         GROUP BY roots.watched_folder_path, roots.generation, roots.status, roots.error \
         ORDER BY roots.watched_folder_path",
        [],
        |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;

    let mut snapshots = Vec::with_capacity(roots.len());
    for (watched_folder_path, generation, status, error, found_count) in roots {
        let generation = u64::try_from(generation).map_err(|error| {
            DbError::Message(format!(
                "folder scan root {} has a negative generation: {error}",
                coven::path_text(&watched_folder_path)
            ))
        })?;
        let status = match (status.as_str(), error) {
            ("scanning", None) => crate::FolderScanStatus::Scanning {
                found_count: to_u64(found_count, "current folder-scan candidate count")?,
            },
            ("complete", None) => crate::FolderScanStatus::Complete,
            ("failed", Some(error)) => crate::FolderScanStatus::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {} has invalid status {status:?} and error {error:?}",
                    coven::path_text(&watched_folder_path)
                )))
            }
        };
        let items = load_folder_scan_items_on(sql, &watched_folder_path, RowSources::Any)?;
        snapshots.push((watched_folder_path, generation, status, items));
    }
    Ok(move || {
        snapshots
            .into_iter()
            .map(|(watched_folder_path, generation, status, items)| {
                Ok(DbFolderScanSnapshot {
                    watched_folder_path,
                    generation,
                    status,
                    items: items()?,
                })
            })
            .collect()
    })
}

/// A folder read as one release: one candidate at the folder's key over
/// everything below it.
pub(crate) fn combined_candidate(
    root: &std::path::Path,
    name: &str,
) -> crate::folder_scanner::ScanItem {
    scanned_candidate_with_scope(root, name, crate::folder_scanner::ReleaseFileScope::Recursive)
}

/// Store `folder` under `root` read as `decision`, yielding `items`.
pub(crate) async fn commit_reading(
    db: &Database,
    root: &std::path::Path,
    folder: &str,
    decision: crate::folder_scanner::FolderReleaseDecision,
    items: Vec<crate::folder_scanner::ScanItem>,
) -> Result<crate::tables::FolderReadingWrite, bae_db::DbError> {
    let tables = crate::tables::ImportTables::new(db.clone());
    let stamp = tables.begin_folder_reading(std::path::Path::new(root)).await?;
    commit_reading_under(db, root, folder, decision, items, stamp).await
}

pub(crate) async fn commit_reading_under(
    db: &Database,
    root: &std::path::Path,
    folder: &str,
    decision: crate::folder_scanner::FolderReleaseDecision,
    items: Vec<crate::folder_scanner::ScanItem>,
    stamp: crate::tables::folder_scans::FolderReadingStamp,
) -> Result<crate::tables::FolderReadingWrite, bae_db::DbError> {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .commit_folder_reading(crate::tables::FolderReadingCommit {
            watched_folder_path: root.to_path_buf(),
            folder: folder.into(),
            stamp,
            decision: Some((
                crate::folder_scanner::FolderReleaseDecisionKey {
                    watched_folder_path: root.to_path_buf(),
                    relative_folder: bae_db::RelativeFolder::test(folder),
                },
                crate::folder_scanner::FolderReading {
                    decision,
                    author: crate::folder_scanner::FolderReleaseDecisionAuthor::User,
                    grouping: crate::CandidateKey::grouping(folder),
                },
            )),
            scanned_decisions: Vec::new(),
            items: items
                .into_iter()
                .map(|item| {
                    crate::tables::FolderEntryToWrite::Release(crate::tables::ScanItemToWrite {
                        item,
                        seed: crate::tables::ScanItemSeed::Tags(None),
                        folder_date: None,
                    })
                })
                .collect(),
            directories: Some(Vec::new()),
        })
        .await
}

/// Scan `root` holding `Box/CD1`, `Box/CD2` and `Other`, returning the scan's
/// generation.
pub(crate) async fn scanned_box_and_sibling(db: &Database, root: &std::path::Path) -> u64 {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(root), crate::VolumeKind::Local)
        .await
        .unwrap();
    for name in ["Box/CD1", "Box/CD2", "Other"] {
        tables
            .save_folder_scan_item(
                std::path::Path::new(root),
                generation,
                &scanned_candidate(root, name),
            )
            .await
            .unwrap()
            .expect("the scan generation is current");
    }
    tables
        .finish_folder_scan(std::path::Path::new(root), generation, None)
        .await
        .unwrap()
        .expect("the scan generation is current");
    generation
}

#[tokio::test]
async fn folder_scan_cache_writes_progressively_and_prunes_only_on_success() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let first = scanned_candidate(root, "First");
    let second = scanned_candidate(root, "Second");
    tables.add_watched_import_folder(root).await.unwrap();

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(tables.save_folder_scan_item(root, generation, &first).await.unwrap().is_some());
    assert!(tables
        .finish_folder_scan(root, generation, Some("share disconnected"))
        .await
        .unwrap()
        .is_some());

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(tables.save_folder_scan_item(root, generation, &second).await.unwrap().is_some());
    assert!(tables
        .finish_folder_scan(root, generation, Some("directory unreadable"))
        .await
        .unwrap()
        .is_some());
    let failed = tables.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].items.len(), 2);
    assert!(matches!(
        &failed[0].status,
        crate::FolderScanStatus::Failed { error }
            if error == "directory unreadable"
    ));

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(tables.save_folder_scan_item(root, generation, &second).await.unwrap().is_some());
    assert!(tables.finish_folder_scan(root, generation, None).await.unwrap().is_some());
    let complete = tables.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(complete[0].items.len(), 1);
    assert_eq!(complete[0].items[0].key(), second.key());
    assert_eq!(complete[0].status, crate::FolderScanStatus::Complete);

    assert!(
        tables.save_folder_scan_item(root, generation - 1, &first).await.unwrap().is_none(),
        "a superseded generation cannot overwrite the stored snapshot"
    );
}

#[tokio::test]
async fn folder_scan_item_rejects_a_mismatched_embedded_root_without_changing_the_snapshot() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let existing = scanned_candidate(root, "Existing");
    tables.save_folder_scan_item(root, generation, &existing).await.unwrap();

    let mismatched = scanned_candidate(&host_root("/other/library"), "Injected");
    let error = tables.save_folder_scan_item(root, generation, &mismatched).await.unwrap_err();

    assert!(error.to_string().contains("does not belong"));
    let snapshot = tables.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].items.len(), 1);
    assert_eq!(snapshot[0].items[0].key(), existing.key());
}

/// An entry whose generation is ahead of its root's was written by nothing
/// here, and fails to load.
#[tokio::test]
async fn a_scan_entry_from_a_generation_the_root_never_reached_fails_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(root, generation, &scanned_candidate(root, "Release"))
        .await
        .unwrap();
    assert!(tables.load_folder_scan_snapshots().await.is_ok());

    db.call(move |conn| {
        conn.execute(
            "UPDATE scan_candidate SET generation = ?",
            params![i64::try_from(generation + 1).unwrap()],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(tables.load_folder_scan_snapshots().await.is_err());
}

/// A scan generation is allocated even when the counter table holds no row.
#[tokio::test]
async fn a_scan_generation_is_allocated_without_a_seeded_counter_row() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    db.call(|sql| {
        sql.execute("DELETE FROM folder_scan_generation_sequence", [])?;
        Ok(())
    })
    .await
    .unwrap();

    let first = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let second = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert_eq!(first, 1);
    assert_eq!(second, 2);
}

/// A rescan reports each candidate tentative before valid, and the list hides
/// tentative rows, so a settled row stays valid or it would vanish mid-rescan.
#[tokio::test]
async fn a_rescan_never_takes_a_settled_row_back_to_tentative() {
    use crate::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let tentative = ScanItem::Discovered(candidate);
    tables.add_watched_import_folder(root).await.unwrap();

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(root, generation, &settled)
        .await
        .unwrap()
        .expect("the first scan stores the candidate");
    tables
        .finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the first scan completes");

    // The re-walk reaches the same folder again and reports it tentative first.
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(root, generation, &tentative)
        .await
        .unwrap()
        .expect("the tentative write is accepted");
    let midway = tables.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(midway[0].items.len(), 1);
    assert!(
        matches!(midway[0].items[0], ScanItem::Valid(_)),
        "the settled row stands through the re-walk, got {:?}",
        midway[0].items[0]
    );

    // And the valid write that follows still replaces it whole.
    tables
        .save_folder_scan_item(root, generation, &settled)
        .await
        .unwrap()
        .expect("the valid write is accepted");
    tables
        .finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the re-walk completes");
    let after = tables.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(after[0].items.len(), 1);
    assert!(matches!(after[0].items[0], ScanItem::Valid(_)));
}

/// The kept row takes this generation, so completing the scan does not prune
/// it.
#[tokio::test]
async fn a_row_kept_through_a_rescan_survives_the_completion_prune() {
    use crate::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let tentative = ScanItem::Discovered(candidate);
    tables.add_watched_import_folder(root).await.unwrap();

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &settled).await.unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    // A re-walk that ends before the valid write arrives.
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &tentative).await.unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let after = tables.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(
        after[0].items.len(),
        1,
        "the kept row carries this generation, so the prune leaves it"
    );
}

// A release is found once: the first write that settles its folder.

async fn write(
    db: &Database,
    root: &std::path::Path,
    generation: u64,
    item: &crate::folder_scanner::ScanItem,
) -> crate::tables::ScanItemWrite {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .save_folder_scan_item(std::path::Path::new(root), generation, item)
        .await
        .unwrap()
        .expect("the scan's generation is current")
}

/// The valid write after a tentative one finds the release; a rescan, even of
/// other files in the folder, does not.
#[tokio::test]
async fn a_release_is_found_by_its_first_settled_write_only() {
    use crate::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let tentative = ScanItem::Discovered(candidate.clone());
    tables.add_watched_import_folder(root).await.unwrap();

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(!write(&db, root, generation, &tentative).await.found());
    assert!(write(&db, root, generation, &settled).await.found());
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(!write(&db, root, generation, &tentative).await.found());
    assert!(!write(&db, root, generation, &settled).await.found());
    let mut changed = candidate;
    changed.files = track_files_candidate(&[("01.flac", 456)]);
    let changed = write(&db, root, generation, &ScanItem::Valid(changed)).await;
    assert!(changed.changed(), "the new files are stored");
    assert!(!changed.found(), "under a key that already held the release");
}

/// A folder that failed validation is already found, so fixing it finds
/// nothing.
#[tokio::test]
async fn a_fixed_folder_is_not_found_again() {
    use crate::folder_scanner::{InvalidCandidate, ScanItem};

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let invalid = ScanItem::Invalid(InvalidCandidate {
        path: candidate.path.clone(),
        name: candidate.name.clone(),
        watched_folder_path: root.clone(),
        relative_folder: candidate.relative_folder.clone(),
        grouping: None,
        files: vec![crate::folder_scanner::SkippedFile::empty_track(&candidate.path, "02.flac")],
        tags: None,
    });
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    write(&db, root, generation, &invalid).await;
    tables.finish_folder_scan(root, generation, None).await.unwrap();

    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(!write(&db, root, generation, &ScanItem::Discovered(candidate)).await.changed());
    let fixed = write(&db, root, generation, &settled).await;
    assert!(fixed.changed());
    assert!(!fixed.found());
}

/// A takeover finds only folders new to the parent; removing the watched
/// folder forgets what it found, so adding it again finds them again.
#[tokio::test]
async fn a_takeover_finds_only_folders_new_to_the_parent() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let music = host_root("/music");
    let inner = host_root("/music/inner");
    tables.add_watched_import_folder(&inner).await.unwrap();
    let generation = tables.begin_folder_scan(&inner, crate::VolumeKind::Local).await.unwrap();
    assert!(write(&db, &inner, generation, &scanned_candidate(&inner, "Album")).await.found());
    tables.finish_folder_scan(&inner, generation, None).await.unwrap();

    tables
        .remove_watched_import_folders(vec![inner.clone()], Some(music.clone()))
        .await
        .unwrap()
        .expect("the inner folder was watched");
    let generation = tables.begin_folder_scan(&music, crate::VolumeKind::Local).await.unwrap();
    assert!(
        !write(&db, &music, generation, &scanned_candidate(&music, "inner/Album")).await.found(),
        "the parent reads a folder its inner watched folder had found"
    );
    assert!(
        write(&db, &music, generation, &scanned_candidate(&music, "New Album")).await.found(),
        "a folder new to the parent is found"
    );
    tables.finish_folder_scan(&music, generation, None).await.unwrap();

    tables
        .remove_watched_import_folders(vec![music.clone()], None)
        .await
        .unwrap()
        .expect("the parent was watched");
    tables.add_watched_import_folder(&music).await.unwrap();
    let generation = tables.begin_folder_scan(&music, crate::VolumeKind::Local).await.unwrap();
    assert!(
        write(&db, &music, generation, &scanned_candidate(&music, "inner/Album")).await.found(),
        "a folder watched again after its removal is found again"
    );
}

/// Releases read together are a new release when combined, and only then: a
/// rebuild of the grouping under the parent that took over its folder finds
/// nothing.
#[tokio::test]
async fn a_grouping_is_found_when_combined_only() {
    use crate::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let music = host_root("/music");
    let inner = host_root("/music/inner");
    tables.add_watched_import_folder(&inner).await.unwrap();
    let generation = tables.begin_folder_scan(&inner, crate::VolumeKind::Local).await.unwrap();
    let members: Vec<_> =
        ["Volume A", "Volume B"].into_iter().map(|name| candidate(&inner, name)).collect();
    for member in &members {
        write(&db, &inner, generation, &ScanItem::Valid(member.clone())).await;
    }
    tables.finish_folder_scan(&inner, generation, None).await.unwrap();
    let combined = tables
        .combine_releases(crate::CandidateKey::grouping("picked"), members)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(combined.found, vec![crate::CandidateKey::grouping("picked")]);

    tables
        .remove_watched_import_folders(vec![inner.clone()], Some(music.clone()))
        .await
        .unwrap()
        .expect("the inner folder was watched");
    let generation = tables.begin_folder_scan(&music, crate::VolumeKind::Local).await.unwrap();
    let mut rebuilt = Vec::new();
    for name in ["inner/Volume A", "inner/Volume B"] {
        let stored =
            write(&db, &music, generation, &ScanItem::Valid(candidate(&music, name))).await;
        assert!(!stored.found(), "{name} was found under the inner folder");
        rebuilt.extend(stored.regrouped().cloned());
    }
    assert!(
        rebuilt.iter().any(|changes| !changes.written.is_empty()),
        "the grouping is built again under the parent"
    );
    assert!(rebuilt.iter().all(|changes| changes.found.is_empty()));
}

/// A tentative candidate at `name` below `root`.
fn tentative_candidate(root: &std::path::Path, name: &str) -> crate::folder_scanner::ScanItem {
    match scanned_candidate(root, name) {
        crate::folder_scanner::ScanItem::Valid(candidate) => {
            crate::folder_scanner::ScanItem::Discovered(candidate)
        }
        other => unreachable!("scanned_candidate is valid: {other:?}"),
    }
}

/// Save `items` in turn under one scan generation of `root`, returning each
/// write.
async fn save_in_turn(
    db: &Database,
    root: &std::path::Path,
    items: &[crate::folder_scanner::ScanItem],
) -> Vec<crate::tables::ScanItemWrite> {
    let tables = crate::tables::ImportTables::new(db.clone());
    let generation = tables
        .begin_folder_scan(std::path::Path::new(root), crate::VolumeKind::Local)
        .await
        .unwrap();
    let mut writes = Vec::new();
    for item in items {
        writes.push(
            tables
                .save_folder_scan_item(std::path::Path::new(root), generation, item)
                .await
                .unwrap()
                .expect("the scan generation is current"),
        );
    }
    writes
}

/// A release read over a whole folder replaces every entry reading a folder
/// below it, tentative or settled, and none whose folder only starts with the
/// same letters.
#[tokio::test]
async fn a_release_read_over_a_folder_replaces_the_entries_below_it_and_no_sibling() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let writes = save_in_turn(
        &db,
        root,
        &[
            scanned_candidate(root, "Box/CD1"),
            tentative_candidate(root, "Box/CD2"),
            scanned_candidate(root, "Box 2"),
            scanned_candidate(root, "Boxes/CD1"),
            combined_candidate(root, "Box"),
        ],
    )
    .await;

    let mut replaced = writes[4].superseded_keys().to_vec();
    replaced.sort();
    assert_eq!(replaced, vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box"), key_of(root, "Box 2"), key_of(root, "Boxes/CD1")]
    );
}

/// A release read from one folder replaces a release read over a folder
/// above it, but not one read from that folder's own files.
#[tokio::test]
async fn a_release_read_from_one_folder_replaces_only_a_whole_folder_above_it() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let writes = save_in_turn(
        &db,
        root,
        &[
            combined_candidate(root, "Set"),
            scanned_candidate(root, "Single"),
            combined_candidate(root, "Set 2"),
            scanned_candidate(root, "Set/CD1"),
            scanned_candidate(root, "Single/Bonus"),
            scanned_candidate(root, "Set 2/CD1/Extra"),
        ],
    )
    .await;

    assert_eq!(writes[3].superseded_keys(), [key_of(root, "Set")]);
    assert!(writes[4].superseded_keys().is_empty(), "{:?}", writes[4]);
    assert_eq!(writes[5].superseded_keys(), [key_of(root, "Set 2")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Set 2/CD1/Extra"),
            key_of(root, "Set/CD1"),
            key_of(root, "Single"),
            key_of(root, "Single/Bonus"),
        ]
    );
}

/// A release written again over its own row replaces only the other entries
/// reading its files.
#[tokio::test]
async fn a_release_written_again_does_not_replace_itself() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let again = match combined_candidate(root, "Box") {
        crate::folder_scanner::ScanItem::Valid(mut candidate) => {
            candidate.name = "Box, read again".to_string();
            crate::folder_scanner::ScanItem::Valid(candidate)
        }
        other => unreachable!("combined_candidate is valid: {other:?}"),
    };
    let writes = save_in_turn(
        &db,
        root,
        &[
            combined_candidate(root, "Box"),
            scanned_candidate(root, "Box/CD1"),
            combined_candidate(root, "Box"),
            again,
        ],
    )
    .await;

    assert_eq!(writes[2].superseded_keys(), [key_of(root, "Box/CD1")]);
    assert!(writes[3].changed());
    assert!(writes[3].superseded_keys().is_empty(), "{:?}", writes[3]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box")]);
}

/// A tentative candidate whose files a settled release already reads stands
/// down, keeping that release; one in a folder whose name only starts with
/// the release's is stored.
#[tokio::test]
async fn a_tentative_candidate_stands_down_only_for_a_release_reading_its_files() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let writes = save_in_turn(
        &db,
        root,
        &[
            combined_candidate(root, "Box"),
            tentative_candidate(root, "Box/CD1"),
            tentative_candidate(root, "Box 2"),
            scanned_candidate(root, "Single"),
            tentative_candidate(root, "Single/Bonus"),
        ],
    )
    .await;

    assert!(!writes[1].changed(), "{:?}", writes[1]);
    assert!(writes[2].changed(), "{:?}", writes[2]);
    assert!(writes[4].changed(), "{:?}", writes[4]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Box"),
            key_of(root, "Box 2"),
            key_of(root, "Single"),
            key_of(root, "Single/Bonus"),
        ]
    );
}

pub(crate) async fn stored_keys(db: &Database, root: &std::path::Path) -> Vec<crate::CandidateKey> {
    let tables = crate::tables::ImportTables::new(db.clone());
    let mut keys: Vec<crate::CandidateKey> = tables
        .load_folder_scan_items(root)
        .await
        .unwrap()
        .iter()
        .map(crate::folder_scanner::ScanItem::key)
        .collect();
    keys.sort();
    keys
}

pub(crate) fn key_of(root: &std::path::Path, name: &str) -> crate::CandidateKey {
    crate::CandidateKey::folder(&folder_below(root, name))
}
