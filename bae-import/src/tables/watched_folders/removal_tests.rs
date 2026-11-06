//! Stopping watching folders: what a removal forgets, and what a folder taking
//! over the watched folders inside it keeps.

use crate::tables::release_groupings::folder_readings::tests::store_user_folder_decision;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;
use bae_store::Store;

#[tokio::test]
async fn removed_and_readded_root_rejects_items_from_its_old_registration() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    let old_generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();

    tables.remove_watched_import_folders(vec![root.clone()], None).await.unwrap();
    tables.add_watched_import_folder(root).await.unwrap();
    let new_generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    assert!(new_generation > old_generation);

    assert!(tables
        .save_folder_scan_item(root, old_generation, &scanned_candidate(root, "Old"))
        .await
        .unwrap()
        .is_none());
    assert!(tables.load_folder_scan_snapshots().await.unwrap()[0].items.is_empty());
}

async fn rows_in(db: &Store, table: &'static str) -> i64 {
    db.read(move |sql| {
        Ok(sql.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))?)
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
async fn picked_grouping(
    db: &Store,
    root: &std::path::Path,
    names: [&str; 2],
    key: &crate::CandidateKey,
) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let members: Vec<_> = names.iter().map(|name| candidate(root, name)).collect();
    for member in &members {
        tables
            .save_folder_scan_item(
                root,
                generation,
                &crate::folder_scanner::ScanItem::Valid(member.clone()),
            )
            .await
            .unwrap();
    }
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    tables.combine_releases(key.clone(), members).await.unwrap();
}

/// Removing a watched folder removes everything decided under it: skips,
/// folder decisions, groupings, and candidate state.
#[tokio::test]
async fn removing_a_watched_folder_forgets_what_was_decided_under_it() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = &host_root("/mounted/library");
    tables.add_watched_import_folder(root).await.unwrap();
    picked_grouping(&db, root, ["Volume A", "Volume B"], &crate::CandidateKey::grouping("picked"))
        .await;
    tables
        .set_import_candidate_skipped(&root.join("Collection").join("Release"), true)
        .await
        .unwrap();
    store_user_folder_decision(
        &db,
        &crate::folder_scanner::FolderReleaseDecisionKey {
            watched_folder_path: root.clone(),
            relative_folder: crate::RelativeFolder::test("Collection"),
        },
        crate::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let candidate = scanned_candidate(root, "Release");
    let crate::folder_scanner::ScanItem::Valid(candidate_files) = &candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = candidate_files.files.content_hash();
    tables.save_folder_scan_item(root, generation, &candidate).await.unwrap();
    crate::tables::import_state::tests::PaneControls::of(&db, candidate_files)
        .type_field(&content_hash, crate::CandidateEditField::AlbumTitle, "Edited Album Title")
        .await
        .unwrap();
    assert!(tables.load_import_candidate_state(&content_hash).await.unwrap().is_some());

    tables.finish_folder_scan(root, generation, None).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    // The scan rows are gone; the grouping's release stays as last built.
    let left: Vec<crate::CandidateKey> = tables.load_folder_scan_snapshots().await.unwrap()[0]
        .items
        .iter()
        .map(crate::folder_scanner::ScanItem::key)
        .collect();
    assert_eq!(left, vec![crate::CandidateKey::grouping("picked")]);

    let removed = tables
        .remove_watched_import_folders(vec![root.clone()], None)
        .await
        .unwrap()
        .expect("the folder was watched");
    assert!(
        removed.removed_keys.contains(&crate::CandidateKey::grouping("picked")),
        "the grouping's release leaves the queue with it: {removed:?}"
    );
    assert!(removed.watched.is_empty(), "the write leaves nothing watched");
    assert!(tables.load_watched_import_folders().await.unwrap().is_empty());
    for table in FOLDER_TABLES {
        assert_eq!(rows_in(&db, table).await, 0, "{table} still holds rows");
    }
    assert_eq!(
        tables
            .load_folder_release_decisions(root)
            .await
            .unwrap()
            .get(&crate::RelativeFolder::test("Collection")),
        None
    );
    assert!(tables.load_folder_scan_snapshots().await.unwrap().is_empty());
    assert!(tables.load_import_candidate_state(&content_hash).await.unwrap().is_none());
}

/// A folder taking over the watched folders inside it keeps everything decided
/// about them; their scan rows go, and the parent's own scan builds the
/// grouping again under it.
#[tokio::test]
async fn a_folder_taking_over_the_watched_folders_inside_it_keeps_what_was_decided() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let music = host_root("/music");
    let one = host_root("/music/one");
    let two = host_root("/music/two");
    for root in [&one, &two] {
        tables.add_watched_import_folder(root).await.unwrap();
    }
    let volume_a = candidate(&one, "Volume A");
    let volume_b = candidate(&two, "Volume B");
    for (root, member) in [(&one, &volume_a), (&two, &volume_b)] {
        let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
        tables
            .save_folder_scan_item(
                root,
                generation,
                &crate::folder_scanner::ScanItem::Valid(member.clone()),
            )
            .await
            .unwrap();
        tables.finish_folder_scan(root, generation, None).await.unwrap();
    }
    // Edited while its folder is still a release of its own, as the pane
    // edits one.
    let edited = volume_b.files.content_hash();
    crate::tables::import_state::tests::PaneControls::of(&db, &volume_b)
        .type_field(&edited, crate::CandidateEditField::AlbumTitle, "Edited Album Title")
        .await
        .unwrap();
    tables
        .combine_releases(
            crate::CandidateKey::grouping("picked"),
            vec![volume_a.clone(), volume_b.clone()],
        )
        .await
        .unwrap();
    let skipped = one.join("Album");
    tables.set_import_candidate_skipped(&skipped, true).await.unwrap();
    store_user_folder_decision(
        &db,
        &crate::folder_scanner::FolderReleaseDecisionKey {
            watched_folder_path: one.clone(),
            relative_folder: crate::RelativeFolder::test("Box"),
        },
        crate::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();
    let mut decided = Vec::new();
    for table in FOLDER_TABLES {
        decided.push(rows_in(&db, table).await);
    }

    let removal = tables
        .remove_watched_import_folders(vec![one.clone(), two.clone()], Some(music.clone()))
        .await
        .unwrap()
        .expect("the inner folders were watched");

    assert_eq!(
        removal.watched.iter().map(|folder| &folder.path).collect::<Vec<_>>(),
        [&music],
        "the write leaves the parent watched in their place"
    );
    let mut removed = removal.removed_keys;
    removed.sort();
    let mut listed = vec![crate::CandidateKey::grouping("picked"), volume_a.key(), volume_b.key()];
    listed.sort();
    assert_eq!(removed, listed, "what the inner scans found leaves the queue");
    assert_eq!(
        tables.load_watched_import_folders().await.unwrap(),
        vec![crate::WatchedFolder::from_path(music.clone())]
    );
    assert!(tables.load_folder_scan_snapshots().await.unwrap().is_empty());
    for (table, before) in FOLDER_TABLES.into_iter().zip(decided) {
        assert_eq!(rows_in(&db, table).await, before, "{table} lost rows");
    }
    assert_eq!(
        tables.load_skipped_import_candidates(&music).await.unwrap(),
        std::collections::HashSet::from([crate::RelativeFolder::test("one/Album")])
    );
    let box_reading = tables.load_folder_release_decisions(&music).await.unwrap();
    let box_reading = box_reading
        .get(&crate::RelativeFolder::test("one/Box"))
        .expect("the box still reads its way");
    assert_eq!(
        box_reading.decision,
        crate::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases
    );
    assert_eq!(box_reading.author, crate::folder_scanner::FolderReleaseDecisionAuthor::User);
    assert_eq!(
        tables.load_grouping(&box_reading.grouping).await.unwrap(),
        Some(crate::tables::GroupingFacts::Anchored {
            folder: crate::folder_scanner::FolderReleaseDecisionKey {
                watched_folder_path: music.clone(),
                relative_folder: crate::RelativeFolder::test("one/Box"),
            },
            decision: crate::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
        })
    );
    assert!(tables.load_import_candidate_state(&edited).await.unwrap().is_some());

    let generation = tables.begin_folder_scan(&music, crate::VolumeKind::Local).await.unwrap();
    for name in ["one/Volume A", "two/Volume B"] {
        tables
            .save_folder_scan_item(
                &music,
                generation,
                &crate::folder_scanner::ScanItem::Valid(candidate(&music, name)),
            )
            .await
            .unwrap();
    }
    tables.finish_folder_scan(&music, generation, None).await.unwrap();
    match tables.load_folder_scan_item(&crate::CandidateKey::grouping("picked")).await.unwrap() {
        Some(crate::folder_scanner::ScanItem::Valid(release)) => {
            assert_eq!(release.watched_folder_path, music);
        }
        other => panic!("the grouping is built again under the parent: {other:?}"),
    }
}

#[tokio::test]
async fn shared_candidate_state_leaves_with_its_last_watched_root() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let first = host_root("/mounted/first");
    let second = host_root("/mounted/second");
    for root in [&first, &second] {
        tables.add_watched_import_folder(root).await.unwrap();
    }

    let second_candidate = scanned_candidate(&second, "Release");
    let first_candidate = scanned_candidate(&first, "Release");
    for (root, candidate) in [(&second, &second_candidate), (&first, &first_candidate)] {
        let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
        tables.save_folder_scan_item(root, generation, candidate).await.unwrap();
        tables.finish_folder_scan(root, generation, None).await.unwrap();
    }
    let crate::folder_scanner::ScanItem::Valid(candidate) = &first_candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = candidate.files.content_hash();

    tables.remove_watched_import_folders(vec![first.clone()], None).await.unwrap();
    assert!(tables.load_import_candidate_state(&content_hash).await.unwrap().is_some());

    let generation = tables.begin_folder_scan(&second, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&second, generation, None).await.unwrap();
    tables.remove_watched_import_folders(vec![second.clone()], None).await.unwrap();
    assert!(tables.load_import_candidate_state(&content_hash).await.unwrap().is_none());
}

/// Removing a watched folder forgets what was decided under it alone: a
/// watched folder beside it whose name only begins with its own keeps its
/// skips.
#[tokio::test]
async fn removing_a_watched_folder_leaves_the_folder_beside_it() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    let beside = host_root("/mounted/library 2");
    for folder in [&root, &beside] {
        tables.add_watched_import_folder(folder).await.unwrap();
        tables.set_import_candidate_skipped(&folder.join("Release"), true).await.unwrap();
    }

    tables
        .remove_watched_import_folders(vec![root.clone()], None)
        .await
        .unwrap()
        .expect("the folder was watched");

    assert!(tables.load_skipped_import_candidates(&root).await.unwrap().is_empty());
    assert_eq!(
        tables.load_skipped_import_candidates(&beside).await.unwrap(),
        [crate::RelativeFolder::test("Release")].into_iter().collect()
    );
}
