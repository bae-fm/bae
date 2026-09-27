// Stopping watching folders: what a removal forgets, and what a folder taking
// over the watched folders inside it keeps.

#[tokio::test]
async fn removed_and_readded_root_rejects_items_from_its_old_registration() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    let old_generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();

    db.remove_watched_import_folders(vec![root.to_string()], None).await.unwrap();
    db.add_watched_import_folder(root).await.unwrap();
    let new_generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    assert!(new_generation > old_generation);

    assert!(db.save_folder_scan_item(root, old_generation, &scanned_candidate(root, "Old")).await.unwrap().is_none());
    assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .is_empty());
}

async fn rows_in(db: &Database, table: &'static str) -> i64 {
    db.read(move |sql| {
        Ok(sql.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?)
    })
    .await
    .unwrap()
}

/// The tables holding what is known about folders, and candidate state.
const FOLDER_TABLES: [&str; 6] = [
    "skipped_import_candidates",
    "folder_discovery",
    "release_grouping",
    "release_grouping_member",
    "import_candidate_folder",
    "import_candidate_state",
];

/// Two releases under `root`, read together as the grouping `key`.
async fn picked_grouping(db: &Database, root: &str, names: [&str; 2], key: &str) {
    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    let members: Vec<_> = names
        .iter()
        .map(|name| super::candidate(root, name))
        .collect();
    for member in &members {
        db.save_folder_scan_item(
            root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(member.clone()),
        )
        .await
        .unwrap();
    }
    db.finish_folder_scan(root, generation, None).await.unwrap();
    db.combine_releases(key.to_string(), members)
        .await
        .unwrap()
        .unwrap();
}

/// Removing a watched folder removes everything decided under it: skips,
/// folder decisions, groupings, and candidate state.
#[tokio::test]
async fn removing_a_watched_folder_forgets_what_was_decided_under_it() {
    let (db, _tmp) = empty_db().await;
    let root = &host_root("/mounted/library");
    db.add_watched_import_folder(root).await.unwrap();
    picked_grouping(&db, root, ["Volume A", "Volume B"], "grouping:picked").await;
    db.set_import_candidate_skipped(
        &crate::import::watched_folder::folder_below(root, "Collection/Release").unwrap(),
        true,
    )
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
    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
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
    let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
    db.finish_folder_scan(root, generation, None)
        .await
        .unwrap();
    // The scan rows are gone; the grouping's release stays as last built.
    let left: Vec<String> = db.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .iter()
        .filter_map(crate::import::folder_scanner::ScanItem::persisted_key)
        .collect();
    assert_eq!(left, vec!["grouping:picked".to_string()]);

    let removed = db
        .remove_watched_import_folders(vec![root.to_string()], None)
        .await
        .unwrap()
        .expect("the folder was watched");
    assert!(
        removed.contains(&"grouping:picked".to_string()),
        "the grouping's release leaves the queue with it: {removed:?}"
    );
    assert!(db
        .load_watched_import_folders()
        .await
        .unwrap()
        .is_empty());
    for table in FOLDER_TABLES {
        assert_eq!(rows_in(&db, table).await, 0, "{table} still holds rows");
    }
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

/// A folder taking over the watched folders inside it keeps everything decided
/// about them; their scan rows go, and the parent's own scan builds the
/// grouping again under it.
#[tokio::test]
async fn a_folder_taking_over_the_watched_folders_inside_it_keeps_what_was_decided() {
    let (db, _tmp) = empty_db().await;
    let music = host_root("/music");
    let one = host_root("/music/one");
    let two = host_root("/music/two");
    for root in [&one, &two] {
        db.add_watched_import_folder(root).await.unwrap();
    }
    let volume_a = super::candidate(&one, "Volume A");
    let volume_b = super::candidate(&two, "Volume B");
    for (root, member) in [(&one, &volume_a), (&two, &volume_b)] {
        let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
        db.save_folder_scan_item(
            root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(member.clone()),
        )
        .await
        .unwrap();
        db.finish_folder_scan(root, generation, None).await.unwrap();
    }
    db.combine_releases(
        "grouping:picked".to_string(),
        vec![volume_a.clone(), volume_b.clone()],
    )
    .await
    .unwrap()
    .unwrap();
    let skipped = crate::import::watched_folder::folder_below(&one, "Album").unwrap();
    db.set_import_candidate_skipped(&skipped, true).await.unwrap();
    store_user_folder_decision(
        &db,
        &crate::import::folder_scanner::FolderReleaseDecisionKey {
            watched_folder_path: one.clone(),
            relative_folder_path: "Box".to_string(),
        },
        crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();
    let edited = volume_b.files.content_hash();
    crate::import::CandidatePreparations::new(db.clone())
        .set_field(
            &edited,
            crate::import::CandidateEditField::AlbumTitle,
            "Edited Album Title",
        )
        .await
        .unwrap();
    let mut decided = Vec::new();
    for table in FOLDER_TABLES {
        decided.push(rows_in(&db, table).await);
    }

    let mut removed = db
        .remove_watched_import_folders(vec![one.clone(), two.clone()], Some(music.clone()))
        .await
        .unwrap()
        .expect("the inner folders were watched");

    removed.sort();
    let mut listed = vec![
        "grouping:picked".to_string(),
        volume_a.key(),
        volume_b.key(),
    ];
    listed.sort();
    assert_eq!(removed, listed, "what the inner scans found leaves the queue");
    assert_eq!(
        db.load_watched_import_folders().await.unwrap(),
        vec![crate::import::WatchedFolder::from_path(music.clone())]
    );
    assert!(db.load_folder_scan_snapshots().await.unwrap().is_empty());
    for (table, before) in FOLDER_TABLES.into_iter().zip(decided) {
        assert_eq!(rows_in(&db, table).await, before, "{table} lost rows");
    }
    assert_eq!(
        db.load_skipped_import_candidates(&music).await.unwrap(),
        std::collections::HashSet::from(["one/Album".to_string()])
    );
    let box_reading = db.load_folder_release_decisions(&music).await.unwrap();
    let box_reading = box_reading.get("one/Box").expect("the box still reads its way");
    assert_eq!(
        box_reading.decision,
        crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases
    );
    assert_eq!(
        box_reading.author,
        crate::import::folder_scanner::FolderReleaseDecisionAuthor::User
    );
    assert_eq!(
        db.load_grouping(&box_reading.grouping).await.unwrap(),
        Some(crate::db::GroupingFacts::Anchored {
            folder: crate::import::folder_scanner::FolderReleaseDecisionKey {
                watched_folder_path: music.clone(),
                relative_folder_path: "one/Box".to_string(),
            },
            decision: crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
        })
    );
    assert!(db
        .load_import_candidate_state(&edited)
        .await
        .unwrap()
        .is_some());

    let generation = db.begin_folder_scan(&music, crate::import::VolumeKind::Local).await.unwrap();
    for name in ["one/Volume A", "two/Volume B"] {
        db.save_folder_scan_item(
            &music,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(super::candidate(&music, name)),
        )
        .await
        .unwrap();
    }
    db.finish_folder_scan(&music, generation, None).await.unwrap();
    match db.load_folder_scan_item("grouping:picked").await.unwrap() {
        Some(crate::import::folder_scanner::ScanItem::Valid(release)) => {
            assert_eq!(release.watched_folder_path, music);
        }
        other => panic!("the grouping is built again under the parent: {other:?}"),
    }
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
        let generation = db.begin_folder_scan(root, crate::import::VolumeKind::Local).await.unwrap();
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

    db.remove_watched_import_folders(vec![first.clone()], None).await.unwrap();
    assert!(db
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .is_some());

    let generation = db.begin_folder_scan(&second, crate::import::VolumeKind::Local).await.unwrap();
    db.finish_folder_scan(&second, generation, None)
        .await
        .unwrap();
    db.remove_watched_import_folders(vec![second.clone()], None).await.unwrap();
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
    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    db.save_folder_scan_item(&root, generation, &candidate)
        .await
        .unwrap();
    db.remove_watched_import_folders(vec![root.clone()], None).await.unwrap();

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
