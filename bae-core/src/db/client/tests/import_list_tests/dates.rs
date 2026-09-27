use super::*;
use crate::import::{folder_scanner::FolderDate, ImportListOrder};
use coven::FixedClock;

async fn dates(db: &Database) -> Vec<(String, i64, Option<i64>, Option<String>)> {
    db.read(|sql| {
        Ok(sql.query(
            "SELECT c.name, d.first_seen_at, d.source_date, d.source_date_kind \
         FROM scan_candidate AS c JOIN folder_discovery AS d ON d.folder = c.folder \
         ORDER BY c.name",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn stored_dates_order_the_list_and_survive_candidate_replacement() {
    let (db, _tmp, root) = watched_root().await;
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for (name, date) in [
        ("A", Some(FolderDate::Created(100))),
        ("B", Some(FolderDate::AddedToDirectory(200))),
        ("C", None),
    ] {
        db.save_folder_scan_item_with_seed(
            &root,
            generation,
            &ScanItem::Valid(candidate(&root, name)),
            None,
            date,
        )
        .await
        .unwrap();
    }
    let stored = dates(&db).await;
    assert_eq!(
        stored,
        vec![
            (
                "A".into(),
                fixed_now().timestamp_millis(),
                Some(100),
                Some("created".into())
            ),
            (
                "B".into(),
                fixed_now().timestamp_millis(),
                Some(200),
                Some("added_to_directory".into())
            ),
            ("C".into(), fixed_now().timestamp_millis(), None, None),
        ]
    );
    let names = |projection: crate::import::ImportListProjection| {
        rows(&projection)
            .into_iter()
            .map(|row| row.folder_name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(
            db.load_import_list(request(TriageTab::Pending).await)
                .await
                .unwrap()
        ),
        ["C", "B", "A"]
    );
    let later = Database::from_handle(
        db.inner.handle.clone(),
        Arc::new(FixedClock(fixed_now() + chrono::Duration::days(1))),
    );
    let generation = later
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for name in ["A", "B", "C"] {
        let original = candidate(&root, name);
        // A tentative write and a rewrite with new files both keep the
        // dates, even with no filesystem date this time.
        later
            .save_folder_scan_item(&root, generation, &ScanItem::Discovered(original.clone()))
            .await
            .unwrap();
        let mut changed = original;
        changed.files.files[0].file.size += 1;
        later
            .save_folder_scan_item(&root, generation, &ScanItem::Valid(changed))
            .await
            .unwrap();
    }
    later
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    assert_eq!(dates(&later).await, stored);
    assert_eq!(
        names(
            later
                .load_import_list(request(TriageTab::Pending).await)
                .await
                .unwrap()
        ),
        ["C", "B", "A"]
    );
    let mut ascending = request(TriageTab::Pending).await;
    ascending.view.order = ImportListOrder::OldestFirst;
    assert_eq!(
        names(later.load_import_list(ascending).await.unwrap()),
        ["A", "B", "C"]
    );
}

#[tokio::test]
async fn a_rescan_captures_dates_even_when_the_candidate_files_are_unchanged() {
    let (db, _tmp, root) = watched_root().await;
    let item = scanned(&db, &root, "Album").await;
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item_with_seed(
        &root,
        generation,
        &ScanItem::Valid(item),
        None,
        Some(FolderDate::Created(123)),
    )
    .await
    .unwrap();
    assert_eq!(
        dates(&db).await,
        vec![(
            "Album".into(),
            fixed_now().timestamp_millis(),
            Some(123),
            Some("created".into())
        )]
    );
}
