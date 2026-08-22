fn scanned_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
    use crate::import::folder_scanner::{FolderCandidate, ReleaseFileScope, ScanItem};

    let path = PathBuf::from(root).join(name);
    ScanItem::Valid(FolderCandidate {
        path: path.clone(),
        file_root: path,
        name: name.to_string(),
        files: track_files_candidate(&[("01.flac", 123)]),
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        display_path: name.to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    })
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

#[tokio::test]
async fn folder_decisions_remove_contradictory_scan_rows_before_failed_rescan() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for name in ["Box/CD1", "Box/CD2"] {
        let item = scanned_candidate(root, name);
        db.save_folder_scan_item(root, generation, &item)
            .await
            .unwrap();
    }
    let key = FolderReleaseDecisionKey {
        watched_folder_path: root.to_string(),
        relative_folder_path: "Box".to_string(),
    };
    let (combine_generation, combine_removals) = db
        .set_folder_release_decisions(&[(key.clone(), FolderReleaseDecision::CombineAsOneRelease)])
        .await
        .unwrap();
    assert_eq!(
        combine_removals,
        vec![
            scanned_candidate(root, "Box/CD1").persisted_key(),
            scanned_candidate(root, "Box/CD2").persisted_key(),
        ]
    );
    db.finish_folder_scan(root, combine_generation, Some("share disconnected"))
        .await
        .unwrap();
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Box"),
        Some(FolderReleaseDecision::CombineAsOneRelease)
    );

    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Box"))
        .await
        .unwrap();
    let (separate_generation, separate_removals) = db
        .set_folder_release_decisions(&[(key, FolderReleaseDecision::KeepAsSeparateReleases)])
        .await
        .unwrap();
    assert_eq!(
        separate_removals,
        vec![scanned_candidate(root, "Box").persisted_key()]
    );
    db.finish_folder_scan(root, separate_generation, Some("share disconnected"))
        .await
        .unwrap();
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Box"),
        Some(FolderReleaseDecision::KeepAsSeparateReleases)
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
async fn folder_decision_failure_rolls_back_decision_entries_and_generation() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Box/CD1"))
        .await
        .unwrap();
    db.call(|conn| {
        conn.execute(
            "UPDATE folder_scan_generation_sequence SET last_generation = ?",
            [i64::MAX],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let result = db
        .set_folder_release_decisions(&[(
            FolderReleaseDecisionKey {
                watched_folder_path: root.to_string(),
                relative_folder_path: "Box".to_string(),
            },
            FolderReleaseDecision::CombineAsOneRelease,
        )])
        .await;
    assert!(result.is_err());

    assert!(db
        .load_folder_release_decisions(root)
        .await
        .unwrap()
        .get("Box")
        .is_none());
    let snapshots = db.load_folder_scan_snapshots().await.unwrap();
    assert_eq!(snapshots[0].generation, generation);
    assert_eq!(snapshots[0].items.len(), 1);
    assert_eq!(
        snapshots[0].items[0].persisted_key(),
        scanned_candidate(root, "Box/CD1").persisted_key()
    );
}

#[tokio::test]
async fn removing_watched_root_cascades_all_local_folder_state() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    db.set_import_candidate_skipped(root, "Collection/Release", true)
        .await
        .unwrap();
    db.set_folder_release_decision(
        &crate::import::folder_scanner::FolderReleaseDecisionKey {
            watched_folder_path: root.to_string(),
            relative_folder_path: "Collection".to_string(),
        },
        crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Release"))
        .await
        .unwrap();

    assert!(db.remove_watched_import_folder(root).await.unwrap().is_some());
    assert!(db
        .load_import_folder_registry()
        .await
        .unwrap()
        .watched_folders()
        .is_empty());
    assert_eq!(
        db.load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Collection"),
        None
    );
    assert!(db.load_folder_scan_snapshots().await.unwrap().is_empty());
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
        .load_import_folder_registry()
        .await
        .unwrap()
        .watched_folders()
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
        .load_import_folder_registry()
        .await
        .unwrap()
        .watched_folders()
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
    assert!(db
        .set_folder_release_decision(
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
            "INSERT INTO folder_release_decisions VALUES (?, 'a/./b', 'combine_as_one_release')",
            params![stored_root],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_import_folder_registry().await.is_err());
    assert!(db.load_folder_release_decisions(&root).await.is_err());
}

#[tokio::test]
async fn corrupt_scan_entry_identity_and_generation_fail_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    let item = scanned_candidate(root, "Release");
    db.save_folder_scan_item(root, generation, &item)
        .await
        .unwrap();
    // A key naming a folder the stored item does not: the entry no longer
    // identifies its own item.
    let other_key = scanned_candidate(root, "Other").persisted_key();
    db.call(move |conn| {
        conn.execute(
            "UPDATE folder_scan_entries SET entry_key = ?",
            params![other_key],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(db.load_folder_scan_snapshots().await.is_err());

    // Key restored, so only the generation is now wrong.
    let item_key = item.persisted_key();
    db.call(move |conn| {
        conn.execute(
            "UPDATE folder_scan_entries SET entry_key = ?, generation = ?",
            params![item_key, i64::try_from(generation + 1).unwrap()],
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
    db.save_import_candidate_file_edits(
        &scanned.content_hash(),
        &folder.path().to_string_lossy(),
        0,
        &candidate_edits,
        &[(folder.path().to_string_lossy().into_owned(), settled)],
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
        Some(SheetDisc::Disc { number: 2 })
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
        fixtures.join("tests/fixtures/test_album.log"),
        tmp.path().join("rip.log"),
    )
    .unwrap();
    let mut cue =
        String::from("PERFORMER \"Test Artist\"\nTITLE \"Album\"\nFILE \"cd.wav\" WAVE\n");
    for track in 1..=12 {
        cue.push_str(&format!(
            "  TRACK {track:02} AUDIO\n    TITLE \"Track {track:02}\"\n    INDEX 01 {:02}:00:00\n",
            (track - 1) * 5,
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
