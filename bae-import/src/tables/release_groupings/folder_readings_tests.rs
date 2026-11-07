//! Folder readings as the store keeps them: a person's reading kept over the
//! scan's, each root's own, offered as the stored scan reads the folder, and
//! a corrupt key failing the read.

use crate::tables::folder_scans::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;
use bae_store::Store;

/// Store the person's answer for how one folder reads, alone: the row a
/// fixture needs beside the entries it writes itself. The service never stores
/// a decision without the candidates it gives — that is
/// `commit_folder_reading`.
pub(crate) async fn store_user_folder_decision(
    db: &Store,
    key: &crate::folder_scanner::FolderReleaseDecisionKey,
    decision: crate::folder_scanner::FolderReleaseDecision,
) -> Result<(), crate::ImportError> {
    let key = key.clone();
    let grouping = crate::CandidateKey::grouping(&format!(
        "{}/{}",
        key.watched_folder_path.display(),
        key.relative_folder.text()
    ));
    db.write(move |sql| {
        super::store_folder_reading(
            &sql,
            &key,
            decision,
            crate::folder_scanner::FolderReleaseDecisionAuthor::User,
            &grouping,
        )
    })
    .await
}

/// A folder decision's key below the root that is not canonical — a `.` —
/// fails the read of the root's decisions. It is spelled with the host's
/// separator, which the read takes the range below the root by.
#[tokio::test]
async fn corrupt_folder_decision_keys_fail_when_loaded() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    tables.add_watched_import_folder(&root).await.unwrap();
    let sep = std::path::MAIN_SEPARATOR;
    let dotted = format!("{}{sep}a{sep}.{sep}b", root.display());
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO release_grouping (key, anchor_folder, combined, author) \
                 VALUES (CAST('grouping:corrupt' AS BLOB), ?, 1, 'user')",
            [dotted.into_bytes()],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(tables.load_folder_release_decisions(&root).await.is_err());
}

/// A folder is offered combined while two releases or more are read from it
/// or below it, and offered separate while the release read from it is its
/// grouping.
#[tokio::test]
async fn a_folder_reading_is_offered_as_the_stored_scan_reads_the_folder() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let root = &host_root("/mounted/library");
    scanned_box_and_sibling(&db, root).await;
    let key = |folder: &str| FolderReleaseDecisionKey {
        watched_folder_path: root.clone(),
        relative_folder: crate::RelativeFolder::test(folder),
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

#[tokio::test]
async fn folder_release_decision_is_idempotent_and_root_scoped() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let other = host_root("/other/library");
    let key = FolderReleaseDecisionKey {
        watched_folder_path: host_root("/mounted/library"),
        relative_folder: crate::RelativeFolder::test("Collection/Release Wrapper"),
    };
    tables.add_watched_import_folder(&key.watched_folder_path).await.unwrap();
    tables.add_watched_import_folder(&other).await.unwrap();

    store_user_folder_decision(&db, &key, FolderReleaseDecision::CombineAsOneRelease)
        .await
        .unwrap();
    store_user_folder_decision(&db, &key, FolderReleaseDecision::CombineAsOneRelease)
        .await
        .unwrap();
    store_user_folder_decision(
        &db,
        &FolderReleaseDecisionKey {
            watched_folder_path: other,
            relative_folder: key.relative_folder.clone(),
        },
        FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();

    let decisions = tables.load_folder_release_decisions(&key.watched_folder_path).await.unwrap();
    assert_eq!(
        decisions.get(&key.relative_folder).map(|reading| (reading.decision, reading.author)),
        Some((
            FolderReleaseDecision::CombineAsOneRelease,
            crate::folder_scanner::FolderReleaseDecisionAuthor::User
        ))
    );
}

/// A scan's reading of a folder the store reads as another grouping, as
/// one stored after the reading was taken, is refused as that, and the
/// stored grouping stays.
#[tokio::test]
async fn a_reading_under_another_grouping_is_refused() {
    use crate::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    tables.add_watched_import_folder(&root).await.unwrap();
    let key = FolderReleaseDecisionKey {
        watched_folder_path: root.clone(),
        relative_folder: crate::RelativeFolder::test("Box"),
    };
    let stored = crate::CandidateKey::grouping("stored");
    let taken = crate::CandidateKey::grouping("taken");
    tables
        .record_scanned_folder_release_decision(
            &key,
            FolderReleaseDecision::KeepAsSeparateReleases,
            &stored,
        )
        .await
        .unwrap();

    let refused = tables
        .record_scanned_folder_release_decision(
            &key,
            FolderReleaseDecision::CombineAsOneRelease,
            &taken,
        )
        .await;

    assert!(
        matches!(
            &refused,
            Err(crate::ImportError::FolderReading(
                crate::tables::FolderReadingRefusal::GroupedElsewhere { stored: held, grouping, .. }
            )) if *held == stored && *grouping == taken
        ),
        "{refused:?}"
    );
    assert_eq!(tables.folder_reading_grouping(&key).await.unwrap(), Some(stored));
}
