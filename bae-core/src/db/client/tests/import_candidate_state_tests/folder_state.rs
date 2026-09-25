fn scanned_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
    scanned_candidate_with_scope(
        root,
        name,
        crate::import::folder_scanner::ReleaseFileScope::Direct,
    )
}

/// The shape a folder read as one release stores as: one candidate at the
/// folder's own key over everything below it.
fn combined_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
    scanned_candidate_with_scope(
        root,
        name,
        crate::import::folder_scanner::ReleaseFileScope::Recursive,
    )
}

fn scanned_candidate_with_scope(
    root: &str,
    name: &str,
    scope: crate::import::folder_scanner::ReleaseFileScope,
) -> crate::import::folder_scanner::ScanItem {
    crate::import::folder_scanner::ScanItem::Valid(super::candidate_with(
        root,
        name,
        track_files_candidate(&[("01.flac", 123)]),
        scope,
    ))
}

#[tokio::test]
async fn folder_scan_cache_writes_progressively_and_prunes_only_on_success() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    let first = scanned_candidate(root, "First");
    let second = scanned_candidate(root, "Second");
    db.add_watched_import_folder(root).await.unwrap();

    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db.save_folder_scan_item(root, generation, &first).await.unwrap().is_some());
    assert!(db.finish_folder_scan(root, generation, Some("share disconnected")).await.unwrap().is_some());

    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db.save_folder_scan_item(root, generation, &second).await.unwrap().is_some());
    assert!(db.finish_folder_scan(root, generation, Some("directory unreadable")).await.unwrap().is_some());
    let failed = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].items.len(), 2);
    assert!(matches!(
        &failed[0].status,
        crate::import::FolderScanStatus::Failed { error }
            if error == "directory unreadable"
    ));

    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db.save_folder_scan_item(root, generation, &second).await.unwrap().is_some());
    assert!(db.finish_folder_scan(root, generation, None).await.unwrap().is_some());
    let complete = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(complete[0].items.len(), 1);
    assert_eq!(complete[0].items[0].persisted_key(), second.persisted_key());
    assert_eq!(
        complete[0].status,
        crate::import::FolderScanStatus::Complete
    );

    assert!(db.save_folder_scan_item(root, generation - 1, &first).await.unwrap().is_none(),
        "a superseded generation cannot overwrite the stored snapshot"
    );
}

#[tokio::test]
async fn folder_scan_item_rejects_a_mismatched_embedded_root_without_changing_the_snapshot() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let existing = scanned_candidate(root, "Existing");
    db.save_folder_scan_item(root, generation, &existing)
        .await
        .unwrap();

    let mismatched = scanned_candidate(&host_root("/other/library"), "Injected");
    let error = db
        .save_folder_scan_item(root, generation, &mismatched)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("does not belong"));
    let snapshot = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].items.len(), 1);
    assert_eq!(
        snapshot[0].items[0].persisted_key(),
        existing.persisted_key()
    );
}

#[tokio::test]
async fn imported_content_hash_lookup_uses_its_partial_index() {
    let (db, _tmp) = empty_db().await;
    let plan = db.content_hash_query_plan_for_test().await.unwrap();

    assert!(
        plan.iter()
            .any(|detail| detail.contains("idx_releases_content_hash")),
        "query plan did not use the content-hash index: {plan:?}"
    );
}

/// One stored folder reading: `folder` under `root` read as `decision`,
/// yielding `items`, under a reading begun now.
async fn commit_reading(
    db: &Database,
    root: &str,
    folder: &str,
    decision: crate::import::folder_scanner::FolderReleaseDecision,
    items: Vec<crate::import::folder_scanner::ScanItem>,
) -> Result<crate::db::FolderReadingWrite, coven::DbError> {
    let stamp = db.begin_folder_reading(root).await?;
    commit_reading_under(db, root, folder, decision, items, stamp).await
}

async fn commit_reading_under(
    db: &Database,
    root: &str,
    folder: &str,
    decision: crate::import::folder_scanner::FolderReleaseDecision,
    items: Vec<crate::import::folder_scanner::ScanItem>,
    stamp: crate::db::FolderReadingStamp,
) -> Result<crate::db::FolderReadingWrite, coven::DbError> {
    db.commit_folder_reading(crate::db::FolderReadingCommit {
        watched_folder_path: root.to_string(),
        folder: folder.to_string(),
        stamp,
        decision: Some((
            crate::import::folder_scanner::FolderReleaseDecisionKey {
                watched_folder_path: root.to_string(),
                relative_folder_path: folder.to_string(),
            },
            decision,
        )),
        scanned_decisions: Vec::new(),
        items: items
            .into_iter()
            .map(|item| crate::db::ScanItemToWrite {
                item,
                file_metadata: None,
                folder_date: None,
            })
            .collect(),
        directories: Some(Vec::new()),
    })
    .await
}

/// `root` scanned to completion with `Box/CD1`, `Box/CD2` and a sibling
/// `Other`, returning the generation that scan stamped.
async fn scanned_box_and_sibling(db: &Database, root: &str) -> u64 {
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["Box/CD1", "Box/CD2", "Other"] {
        db.save_folder_scan_item(root, generation, &scanned_candidate(root, name))
            .await
            .unwrap()
            .expect("the scan generation is current");
    }
    db.finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the scan generation is current");
    generation
}

async fn stored_keys(db: &Database, root: &str) -> Vec<String> {
    let mut keys: Vec<String> = db
        .load_folder_scan_items(root)
        .await
        .unwrap()
        .iter()
        .filter_map(crate::import::folder_scanner::ScanItem::persisted_key)
        .collect();
    keys.sort();
    keys
}

async fn row_generation(db: &Database, root: &str, name: &str) -> i64 {
    let path = std::path::Path::new(root)
        .join(name)
        .to_string_lossy()
        .into_owned();
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

fn key_of(root: &str, name: &str) -> String {
    std::path::Path::new(root)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Combining a folder and keeping it separate each trade the folder's entries
/// for the other reading's in the write that stores the decision; the sibling
/// folder is neither rewritten nor restamped.
#[tokio::test]
async fn a_folder_reading_trades_its_entries_in_the_write_that_stores_the_decision() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionAuthor};

    let (db, _tmp) = empty_db().await;
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
    assert_eq!(
        combined.pruned,
        vec![key_of(root, "Box/CD1"), key_of(root, "Box/CD2")]
    );
    assert_eq!(
        stored_keys(&db, root).await,
        vec![key_of(root, "Box"), key_of(root, "Other")]
    );
    assert_eq!(
        db.load_folder_release_decisions(root).await.unwrap().get("Box"),
        Some((
            FolderReleaseDecision::CombineAsOneRelease,
            FolderReleaseDecisionAuthor::User
        ))
    );
    assert_eq!(row_generation(&db, root, "Other").await, sibling_generation);
    let snapshot = &db.load_folder_scan_snapshots().await.unwrap()[0];
    assert!(snapshot.generation > scanned);
    assert_eq!(snapshot.status, crate::import::FolderScanStatus::Complete);

    let separated = commit_reading(
        &db,
        root,
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
        vec![
            scanned_candidate(root, "Box/CD1"),
            scanned_candidate(root, "Box/CD2"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(separated.pruned, vec![key_of(root, "Box")]);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Box/CD1"),
            key_of(root, "Box/CD2"),
            key_of(root, "Other")
        ]
    );
    assert_eq!(
        db.load_folder_release_decisions(root).await.unwrap().get("Box"),
        Some((
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User
        ))
    );
    assert_eq!(row_generation(&db, root, "Other").await, sibling_generation);
}

/// A reading taken before something else wrote the root's entries describes
/// a store that is gone: storing it fails, and writes nothing.
#[tokio::test]
async fn a_folder_reading_taken_before_the_root_moved_stores_nothing() {
    use crate::import::folder_scanner::FolderReleaseDecision;

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    scanned_box_and_sibling(&db, root).await;
    let stamp = db.begin_folder_reading(root).await.unwrap();
    let moved = db.begin_folder_scan(root).await.unwrap();

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
    assert!(db.load_folder_release_decisions(root).await.unwrap().get("Box").is_none());
    assert_eq!(db.load_folder_scan_snapshots().await.unwrap()[0].generation, moved);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Box/CD1"),
            key_of(root, "Box/CD2"),
            key_of(root, "Other")
        ]
    );
}

/// A reading that fails partway through its write leaves the decision, the
/// entries and the root's generation as they were.
#[tokio::test]
async fn a_folder_reading_that_fails_partway_rolls_back_whole() {
    use crate::import::folder_scanner::{FolderReleaseDecision, ScanItem};

    let (db, _tmp) = empty_db().await;
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
    assert!(db.load_folder_release_decisions(root).await.unwrap().get("Box").is_none());
    assert_eq!(db.load_folder_scan_snapshots().await.unwrap()[0].generation, generation);
    assert_eq!(
        stored_keys(&db, root).await,
        vec![
            key_of(root, "Box/CD1"),
            key_of(root, "Box/CD2"),
            key_of(root, "Other")
        ]
    );
}

#[tokio::test]
async fn removed_and_readded_root_rejects_items_from_its_old_registration() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let old_generation = db.begin_folder_scan(root).await.unwrap();

    db.remove_watched_import_folder(root).await.unwrap();
    db.add_watched_import_folder(root).await.unwrap();
    let new_generation = db.begin_folder_scan(root).await.unwrap();
    assert!(new_generation > old_generation);

    assert!(db.save_folder_scan_item(root, old_generation, &scanned_candidate(root, "Old")).await.unwrap().is_none());
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
}

#[tokio::test]
async fn removing_watched_root_cascades_all_local_folder_state() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    db.set_import_candidate_skipped(root, "Collection/Release", true)
        .await
        .unwrap();
    store_user_folder_decision(
    &db,
        &crate::import::folder_scanner::FolderReleaseDecisionKey {
            watched_folder_path: root.to_string(),
            relative_folder_path: "Collection".to_string(),
        },
        crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let candidate = scanned_candidate(root, "Release");
    let crate::import::folder_scanner::ScanItem::Valid(candidate_files) = &candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = candidate_files.files.content_hash();
    db.save_folder_scan_item(root, generation, &candidate)
        .await
        .unwrap();
    crate::import::CandidatePreparations::new(db.clone()).set_field(
        &content_hash,
        crate::import::CandidateEditField::AlbumTitle,
        "Edited Album Title",
    )
    .await
    .unwrap();
    assert!(db
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .is_some());

    db.finish_folder_scan(root, generation, None)
        .await
        .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.finish_folder_scan(root, generation, None)
        .await
        .unwrap();
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());

    assert!(db.remove_watched_import_folder(root).await.unwrap().is_some());
    assert!(db
        .load_watched_import_folders()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Collection"),
        None
    );
    assert!(db.load_folder_scan_snapshots().await.unwrap().is_empty());
    assert!(db
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn shared_candidate_state_leaves_with_its_last_watched_root() {
    let (db, _tmp) = empty_db().await;
    let first = host_root("/mounted/first");
    let second = host_root("/mounted/second");
    for root in [&first, &second] {
        db.add_watched_import_folder(root).await.unwrap();
    }

    let second_candidate = scanned_candidate(&second, "Release");
    let first_candidate = scanned_candidate(&first, "Release");
    for (root, candidate) in [(&second, &second_candidate), (&first, &first_candidate)] {
        let generation = db.begin_folder_scan(root).await.unwrap();
        db.save_folder_scan_item(root, generation, candidate)
            .await
            .unwrap();
        db.finish_folder_scan(root, generation, None)
            .await
            .unwrap();
    }
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &first_candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = candidate.files.content_hash();

    db.remove_watched_import_folder(&first).await.unwrap();
    assert!(db
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .is_some());

    let generation = db.begin_folder_scan(&second).await.unwrap();
    db.finish_folder_scan(&second, generation, None)
        .await
        .unwrap();
    db.remove_watched_import_folder(&second).await.unwrap();
    assert!(db
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_late_import_failure_cannot_recreate_state_after_root_removal() {
    let (db, _tmp) = empty_db().await;
    let root = host_root("/mounted/library");
    let candidate = scanned_candidate(&root, "Release");
    let crate::import::folder_scanner::ScanItem::Valid(folder) = &candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = folder.files.content_hash();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root).await.unwrap();
    db.save_folder_scan_item(&root, generation, &candidate)
        .await
        .unwrap();
    db.remove_watched_import_folder(&root).await.unwrap();

    db.save_import_candidate_failure(
        &content_hash,
        0,
        &crate::import::ImportFailure::error_only(
            "the source disappeared",
            fixed_now(),
        ),
    )
    .await
    .expect_err("a removed candidate cannot receive an import failure");
    assert!(db
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn watched_root_overlap_uses_paths_not_sql_patterns() {
    let (db, _tmp) = empty_db().await;
    for root in ["/music/100%", "/music/name_value"] {
        assert!(db
            .add_watched_import_folder(&host_root(root))
            .await
            .unwrap());
    }
    for child in ["/music/100%/child", "/music/name_value/child"] {
        let error = db
            .add_watched_import_folder(&host_root(child))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("cannot overlap"), "{child}");
    }
}

#[tokio::test]
async fn watched_root_order_survives_middle_removal_and_later_add() {
    let (db, _tmp) = empty_db().await;
    for root in ["/one", "/two", "/three"] {
        db.add_watched_import_folder(&host_root(root))
            .await
            .unwrap();
    }
    db.remove_watched_import_folder(&host_root("/two"))
        .await
        .unwrap();
    db.add_watched_import_folder(&host_root("/four"))
        .await
        .unwrap();
    let paths: Vec<_> = db
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(
        paths,
        vec![host_root("/one"), host_root("/three"), host_root("/four")]
    );
}

/// However the folder was spelled on the way in, one row exists and it is
/// keyed by the canonical spelling — so a second spelling of a folder
/// already watched is recognized as the same folder rather than added
/// beside it.
#[tokio::test]
async fn watched_root_spellings_settle_on_one_row() {
    let (db, _tmp) = empty_db().await;
    let canonical = host_root("/music/rips");
    assert!(db.add_watched_import_folder(&canonical).await.unwrap());

    // The last of these is the drive-lettered, forward-slashed form a
    // `bae://import` link and a `file://` folder drop hand over on Windows.
    #[cfg(windows)]
    const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
    #[cfg(not(windows))]
    const URL_SPELLINGS: &[&str] = &[];

    let spellings = [
        host_root("/music/rips/"),
        host_root("/music//rips"),
        host_root("/music/./rips"),
    ];

    for spelling in spellings
        .iter()
        .map(String::as_str)
        .chain(URL_SPELLINGS.iter().copied())
    {
        assert!(
            !db.add_watched_import_folder(spelling).await.unwrap(),
            "{spelling} is the folder already watched, not a new one"
        );
    }
    let paths: Vec<_> = db
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(paths, vec![canonical]);
}

/// `..` never becomes a key: rewriting it without reading the filesystem
/// is wrong across a symlink, so it is refused instead.
#[tokio::test]
async fn watched_root_rejects_a_path_climbing_out_of_itself() {
    let (db, _tmp) = empty_db().await;
    let path = host_root("/music/../rips");
    assert!(db.add_watched_import_folder(&path).await.is_err(), "{path}");
}

#[tokio::test]
async fn corrupt_relative_folder_keys_fail_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let root = host_root("/mounted/library");
    db.add_watched_import_folder(&root).await.unwrap();
    assert!(db
        .set_import_candidate_skipped(&root, "a//b", true)
        .await
        .is_err());
    assert!(store_user_folder_decision(
        &db,
            &crate::import::folder_scanner::FolderReleaseDecisionKey {
                watched_folder_path: root.clone(),
                relative_folder_path: "a/./b".to_string(),
            },
            crate::import::folder_scanner::FolderReleaseDecision::CombineAsOneRelease,
        )
        .await
        .is_err());
    let stored_root = root.clone();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO skipped_import_candidates VALUES (?, 'a//b')",
            params![stored_root],
        )?;
        conn.execute(
            "INSERT INTO folder_release_decisions \
                 VALUES (?, 'a/./b', 'combine_as_one_release', 'user')",
            params![stored_root],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_skipped_import_candidates(&root).await.is_err());
    assert!(db.load_folder_release_decisions(&root).await.is_err());
}

/// No two stored roots overlap: the add refuses one under another, on any
/// spelling. Rows that overlap anyway are corrupt durable state and are read
/// loudly rather than served as two folders.
#[tokio::test]
async fn overlapping_stored_roots_fail_to_load() {
    let (db, _tmp) = empty_db().await;
    let outer = host_root("/music");
    let inner = host_root("/music/artist");
    db.add_watched_import_folder(&outer).await.unwrap();
    let error = db.add_watched_import_folder(&inner).await.unwrap_err();
    assert!(error.to_string().contains("cannot overlap"), "{error}");
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO watched_import_folders (path, position) VALUES (?, 1)",
            params![inner],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let error = db.load_watched_import_folders().await.unwrap_err();
    assert!(error.to_string().contains("cannot overlap"), "{error}");
}

/// A candidate's key is its own `path` column, so an entry can no longer
/// name a folder it does not describe. Its generation is still a separate
/// column, and one ahead of its root's is a store nothing here wrote.
#[tokio::test]
async fn a_scan_entry_from_a_generation_the_root_never_reached_fails_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Release"))
        .await
        .unwrap();
    assert!(db.load_folder_scan_snapshots().await.is_ok());

    db.call(move |conn| {
        conn.execute(
            "UPDATE scan_candidate SET generation = ?",
            params![i64::try_from(generation + 1).unwrap()],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_folder_scan_snapshots().await.is_err());
}

/// A disc assignment the user set survives a relaunch: it is stored under
/// the candidate's content hash, read back from a cold database, and the
/// scan that follows lays the discs down as they settled them rather than
/// in the order the cue filenames read.
#[tokio::test]
async fn a_disc_assignment_survives_a_relaunch() {
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetDisc, SheetDiscEdits,
        StoredCandidateEdits,
    };

    let (db, _tmp) = empty_db().await;
    let folder = two_sheet_folder();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let root = folder.path().to_string_lossy().into_owned();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root).await.unwrap();
    let candidate = crate::import::folder_scanner::FolderCandidate {
        path: folder.path().to_path_buf(),
        file_root: folder.path().to_path_buf(),
        name: "Release".to_string(),
        files: scanned.clone(),
        watched_folder_path: root.clone(),
        scope: crate::import::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: String::new(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    };
    db.save_folder_scan_item(
        &root,
        generation,
        &crate::import::folder_scanner::ScanItem::Valid(candidate),
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    // The rip named its sheets the other way round: `alpha.cue` is disc two.
    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("alpha.cue".to_string(), SheetDisc::Disc { number: 2 });
    sheet_discs.set("beta.cue".to_string(), SheetDisc::Disc { number: 1 });
    let candidate_edits = CandidateFileEdits {
        sheet_discs,
        ..Default::default()
    };
    let mut settled = scanned.clone();
    settled
        .apply_candidate_file_edits(&candidate_edits)
        .unwrap();
    let hash = scanned.content_hash();
    let (metadata_revision, mapping_preparation) = current_mapping_preparation(&db, &hash).await;
    crate::import::CandidatePreparations::new(db.clone()).store_file_decisions(
        &as_read(&hash, metadata_revision),
        &folder.path().to_string_lossy(),
        &candidate_edits,
        &[(folder.path().to_string_lossy().into_owned(), settled)],
        &mapping_preparation,
    )
    .await
    .unwrap();

    let current = db
        .load_candidate_file_edits(&scanned.content_hash())
        .await
        .unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(
        current.sheet_discs.get("alpha.cue"),
        Some(&SheetDisc::Disc { number: 2 })
    );

    // A subsequent scan reads the same decisions, so the folder's audio
    // comes out in the order the user settled rather than in path order.
    let stored = db.load_stored_candidate_edits().await.unwrap();
    let reopened = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .unwrap();
    assert_eq!(
        reopened
            .carving_sheets()
            .iter()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![
            ("alpha.cue", SheetDisc::Disc { number: 2 }),
            ("beta.cue", SheetDisc::Disc { number: 1 }),
        ],
    );
}

/// Two bound single-track sheets, each naming the audio beside it.
fn two_sheet_folder() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for stem in ["alpha", "beta"] {
        std::fs::copy(
            fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
            tmp.path().join(format!("{stem}.flac")),
        )
        .unwrap();
        std::fs::write(
            tmp.path().join(format!("{stem}.cue")),
            format!(
                "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
                 FILE \"{stem}.flac\" WAVE\n  TRACK 01 AUDIO\n    \
                 TITLE \"Track Title\"\n    INDEX 01 00:00:00\n",
            ),
        )
        .unwrap();
    }
    tmp
}

/// The walkthrough folder on disk: a twelve-track sheet written against a
/// WAV, the FLAC it was actually encoded to, and the rip log.
fn walkthrough_folder() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(
        fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
        tmp.path().join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("tests/fixtures/logs/test_album.log"),
        tmp.path().join("rip.log"),
    )
    .unwrap();
    let mut cue =
        String::from("PERFORMER \"Test Artist\"\nTITLE \"Album\"\nFILE \"cd.wav\" WAVE\n");
    for track in 1..=12 {
        cue.push_str(&format!(
            "  TRACK {track:02} AUDIO\n    TITLE \"Track {track:02}\"\n    INDEX 01 00:{:02}:00\n",
            track - 1,
        ));
    }
    std::fs::write(tmp.path().join("cd.cue"), cue).unwrap();
    tmp
}

/// The generation counter is allocated by the scan write itself, not read
/// from a row a migration seeded: a store whose device-local tables were
/// rebuilt without the seed still scans.
#[tokio::test]
async fn a_scan_generation_is_allocated_without_a_seeded_counter_row() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    db.call(|sql| {
        sql.execute("DELETE FROM folder_scan_generation_sequence", [])?;
        Ok(())
    })
    .await
    .unwrap();

    let first = db.begin_folder_scan(root).await.unwrap();
    let second = db.begin_folder_scan(root).await.unwrap();
    assert_eq!(first, 1);
    assert_eq!(second, 2);
}

/// A tentative candidate is a release approximation the scan found before it
/// knew what enclosed it, and the list draws none of them. A re-walk sends
/// every candidate through that state on its way back to valid, so a row that
/// is already a settled release must not go back through it: it would leave
/// the list and the tab counts until the valid write landed a moment later,
/// which is the swing a viewer sees while a folder rescans.
#[tokio::test]
async fn a_rescan_never_takes_a_settled_row_back_to_tentative() {
    use crate::import::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let tentative = ScanItem::Discovered(candidate);
    db.add_watched_import_folder(root).await.unwrap();

    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &settled)
        .await
        .unwrap()
        .expect("the first scan stores the candidate");
    db.finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the first scan completes");

    // The re-walk reaches the same folder again and reports it tentative first.
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &tentative)
        .await
        .unwrap()
        .expect("the tentative write is accepted");
    let midway = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(midway[0].items.len(), 1);
    assert!(
        matches!(midway[0].items[0], ScanItem::Valid(_)),
        "the settled row stands through the re-walk, got {:?}",
        midway[0].items[0]
    );

    // And the valid write that follows still replaces it whole.
    db.save_folder_scan_item(root, generation, &settled)
        .await
        .unwrap()
        .expect("the valid write is accepted");
    db.finish_folder_scan(root, generation, None)
        .await
        .unwrap()
        .expect("the re-walk completes");
    let after = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(after[0].items.len(), 1);
    assert!(matches!(after[0].items[0], ScanItem::Valid(_)));
}

/// The stamp the kept row takes is this generation's, so completing the scan
/// does not prune the very row it just decided to keep.
#[tokio::test]
async fn a_row_kept_through_a_rescan_survives_the_completion_prune() {
    use crate::import::folder_scanner::ScanItem;

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    let settled = scanned_candidate(root, "Album");
    let ScanItem::Valid(candidate) = settled.clone() else {
        panic!("the fixture is a valid candidate")
    };
    let tentative = ScanItem::Discovered(candidate);
    db.add_watched_import_folder(root).await.unwrap();

    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &settled).await.unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();

    // A re-walk that only ever reports it tentative — the valid write never
    // arrives, because the scan ended first.
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &tentative).await.unwrap();
    db.finish_folder_scan(root, generation, None).await.unwrap();

    let after = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(
        after[0].items.len(),
        1,
        "the kept row carries this generation, so the prune leaves it"
    );
}
