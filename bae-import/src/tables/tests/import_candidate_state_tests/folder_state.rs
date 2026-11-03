/// A folder read as one release: one candidate at the folder's key over
/// everything below it.
fn combined_candidate(
    root: &std::path::Path,
    name: &str,
) -> crate::folder_scanner::ScanItem {
    scanned_candidate_with_scope(
        root,
        name,
        crate::folder_scanner::ReleaseFileScope::Recursive,
    )
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
    assert_eq!(complete[0].items[0].persisted_key(), second.persisted_key());
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
    assert_eq!(snapshot[0].items[0].persisted_key(), existing.persisted_key());
}

/// Whether this device imported a folder's files is a lookup by its hash,
/// which the hash's uniqueness indexes.
#[tokio::test]
async fn imported_content_hash_lookup_uses_its_unique_index() {
    let (db, _tmp) = empty_db().await;
    let plan = db.content_hash_query_plan_for_test().await.unwrap();

    assert!(
        plan.iter()
            .any(|detail| detail.contains("INDEX") && detail.contains("local_release_origin")),
        "query plan did not use the content-hash index: {plan:?}"
    );
}

/// Store `folder` under `root` read as `decision`, yielding `items`.
async fn commit_reading(
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

async fn commit_reading_under(
    db: &Database,
    root: &std::path::Path,
    folder: &str,
    decision: crate::folder_scanner::FolderReleaseDecision,
    items: Vec<crate::folder_scanner::ScanItem>,
    stamp: crate::tables::folder_scans::FolderReadingStamp,
) -> Result<crate::tables::FolderReadingWrite, bae_db::DbError> {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.commit_folder_reading(crate::tables::FolderReadingCommit {
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
            .map(|item| crate::tables::ScanItemToWrite {
                item,
                seed: crate::tables::ScanItemSeed::Tags(None),
                folder_date: None,
            })
            .collect(),
        directories: Some(Vec::new()),
    })
    .await
}

/// Scan `root` holding `Box/CD1`, `Box/CD2` and `Other`, returning the scan's
/// generation.
async fn scanned_box_and_sibling(db: &Database, root: &std::path::Path) -> u64 {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(root), crate::VolumeKind::Local)
        .await
        .unwrap();
    for name in ["Box/CD1", "Box/CD2", "Other"] {
        tables.save_folder_scan_item(
            std::path::Path::new(root),
            generation,
            &scanned_candidate(root, name),
        )
        .await
        .unwrap()
        .expect("the scan generation is current");
    }
    tables.finish_folder_scan(std::path::Path::new(root), generation, None)
        .await
        .unwrap()
        .expect("the scan generation is current");
    generation
}

async fn stored_keys(db: &Database, root: &std::path::Path) -> Vec<crate::CandidateKey> {
    let tables = crate::tables::ImportTables::new(db.clone());
    let mut keys: Vec<crate::CandidateKey> = tables
        .load_folder_scan_items(root)
        .await
        .unwrap()
        .iter()
        .filter_map(crate::folder_scanner::ScanItem::persisted_key)
        .collect();
    keys.sort();
    keys
}

async fn row_generation(db: &Database, root: &std::path::Path, name: &str) -> i64 {
    let path = key_of(root, name);
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT generation FROM scan_candidate WHERE path = ?",
            [path],
            |row| row.get::<_, i64>(0),
        )?)
    })
    .await
    .unwrap()
}

/// Every entry a folder reading took out, replaced or pruned.
fn removed_by(write: &crate::tables::FolderReadingWrite) -> Vec<crate::CandidateKey> {
    let mut removed: Vec<crate::CandidateKey> = write
        .writes
        .iter()
        .flat_map(|(_, write)| write.superseded_keys().to_vec())
        .chain(write.pruned.iter().cloned())
        .collect();
    removed.sort();
    removed.dedup();
    removed
}

fn key_of(root: &std::path::Path, name: &str) -> crate::CandidateKey {
    crate::CandidateKey::folder(&super::folder_below(root, name))
}

/// Each decision replaces the folder's entries in the same write that stores
/// it, and leaves the sibling folder's row untouched.
#[tokio::test]
async fn a_folder_reading_trades_its_entries_in_the_write_that_stores_the_decision() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionAuthor};

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let scanned = scanned_box_and_sibling(&db, root).await;
    let sibling_generation = row_generation(&db, root, "Other").await;

    let combined = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![combined_candidate(root, "Box")],
    )
    .await
    .unwrap();
    assert_eq!(removed_by(&combined), vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box"), key_of(root, "Other")]);
    assert_eq!(
        tables.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get(&bae_db::RelativeFolder::test("Box"))
            .map(|reading| (reading.decision, reading.author)),
        Some((FolderReleaseDecision::CombineAsOneRelease, FolderReleaseDecisionAuthor::User))
    );
    assert_eq!(row_generation(&db, root, "Other").await, sibling_generation);
    let snapshot = &tables.load_folder_scan_snapshots().await.unwrap()[0];
    assert!(snapshot.generation > scanned);
    assert_eq!(snapshot.status, crate::FolderScanStatus::Complete);

    let separated = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
        vec![scanned_candidate(root, "Box/CD1"), scanned_candidate(root, "Box/CD2")],
    )
    .await
    .unwrap();
    assert_eq!(removed_by(&separated), vec![key_of(root, "Box")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2"), key_of(root, "Other")]
    );
    assert_eq!(
        tables.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get(&bae_db::RelativeFolder::test("Box"))
            .map(|reading| (reading.decision, reading.author)),
        Some((FolderReleaseDecision::KeepAsSeparateReleases, FolderReleaseDecisionAuthor::User))
    );
    assert_eq!(row_generation(&db, root, "Other").await, sibling_generation);
}

/// A folder reading takes out the folder's own entries and those below it,
/// and leaves a sibling whose name only begins with the folder's.
#[tokio::test]
async fn a_folder_reading_prunes_only_the_folder_and_below() {
    use crate::folder_scanner::FolderReleaseDecision;

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    for name in ["Box/CD2", "Box 2", "Box/CD1"] {
        tables.save_folder_scan_item(root, generation, &scanned_candidate(root, name))
            .await
            .unwrap()
            .expect("the scan generation is current");
    }
    tables.finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the scan generation is current");

    let combined = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![combined_candidate(root, "Box")],
    )
    .await
    .unwrap();
    assert_eq!(removed_by(&combined), vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box"), key_of(root, "Box 2")]);

    // Read with nothing found, the folder's own entry is pruned and the
    // sibling stays.
    let emptied =
        commit_reading(&db, root, "Box", FolderReleaseDecision::KeepAsSeparateReleases, Vec::new())
            .await
            .unwrap();
    assert_eq!(emptied.pruned, vec![key_of(root, "Box")]);
    assert_eq!(stored_keys(&db, root).await, vec![key_of(root, "Box 2")]);
}

/// A reading begun before another write to the root fails and writes nothing.
#[tokio::test]
async fn a_folder_reading_taken_before_the_root_moved_stores_nothing() {
    use crate::folder_scanner::FolderReleaseDecision;

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    scanned_box_and_sibling(&db, root).await;
    let stamp = tables.begin_folder_reading(root).await.unwrap();
    let moved = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();

    let error = commit_reading_under(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![combined_candidate(root, "Box")],
        stamp,
    )
    .await
    .err()
    .expect("a reading of a root that moved is refused");
    assert!(error.to_string().contains("changed while it was being read again"), "{error}");
    assert!(tables
        .load_folder_release_decisions(root)
        .await
        .unwrap()
        .get(&bae_db::RelativeFolder::test("Box"))
        .is_none());
    assert_eq!(tables.load_folder_scan_snapshots().await.unwrap()[0].generation, moved);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2"), key_of(root, "Other")]
    );
}

/// A reading that fails partway through its write leaves the decision, the
/// entries and the root's generation as they were.
#[tokio::test]
async fn a_folder_reading_that_fails_partway_rolls_back_whole() {
    use crate::folder_scanner::{FolderReleaseDecision, ScanItem};

    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    let generation = scanned_box_and_sibling(&db, root).await;
    let ScanItem::Valid(mut foreign) = combined_candidate(root, "Box") else {
        panic!("the fixture is a valid candidate");
    };
    foreign.watched_folder_path = host_root("/other/library");

    assert!(commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
        vec![ScanItem::Valid(foreign)],
    )
    .await
    .is_err());
    assert!(tables
        .load_folder_release_decisions(root)
        .await
        .unwrap()
        .get(&bae_db::RelativeFolder::test("Box"))
        .is_none());
    assert_eq!(tables.load_folder_scan_snapshots().await.unwrap()[0].generation, generation);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2"), key_of(root, "Other")]
    );
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
    tables.save_folder_scan_item(root, generation, &scanned_candidate(root, "Release")).await.unwrap();
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
    tables.save_folder_scan_item(root, generation, &settled)
        .await
        .unwrap()
        .expect("the first scan stores the candidate");
    tables.finish_folder_scan(root, generation, None).await.unwrap().expect("the first scan completes");

    // The re-walk reaches the same folder again and reports it tentative first.
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(root, generation, &tentative)
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
    tables.save_folder_scan_item(root, generation, &settled)
        .await
        .unwrap()
        .expect("the valid write is accepted");
    tables.finish_folder_scan(root, generation, None).await.unwrap().expect("the re-walk completes");
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

/// A folder is offered combined while two releases or more are read from it
/// or below it, and offered separate while the release read from it is its
/// grouping.
#[tokio::test]
async fn a_folder_reading_is_offered_as_the_stored_scan_reads_the_folder() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    scanned_box_and_sibling(&db, root).await;
    let key = |folder: &str| FolderReleaseDecisionKey {
        watched_folder_path: root.clone(),
        relative_folder: bae_db::RelativeFolder::test(folder),
    };
    let offered = |folder: &'static str, decision| {
        let db = db.clone();
    let tables = crate::tables::ImportTables::new(db.clone());
        let key = key(folder);
        async move { tables.folder_reading_offered(&key, decision).await.unwrap() }
    };
    assert!(offered("Box", FolderReleaseDecision::CombineAsOneRelease).await);
    assert!(!offered("Other", FolderReleaseDecision::CombineAsOneRelease).await);
    assert!(!offered("Box", FolderReleaseDecision::KeepAsSeparateReleases).await);

    // Read as one release, the folder's release is its grouping.
    let mut combined = combined_candidate(root, "Box");
    if let crate::folder_scanner::ScanItem::Valid(candidate) = &mut combined {
        candidate.grouping = Some(crate::CandidateKey::grouping("Box"));
    }
    commit_reading(&db, root, "Box", FolderReleaseDecision::CombineAsOneRelease, vec![combined])
        .await
        .unwrap();
    assert!(offered("Box", FolderReleaseDecision::KeepAsSeparateReleases).await);
    assert!(!offered("Box", FolderReleaseDecision::CombineAsOneRelease).await);
}
