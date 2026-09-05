use super::*;
use crate::import::{folder_scanner::FolderDate, ImportListOrder};

async fn dates(db: &Database) -> Vec<(String, i64, Option<i64>, Option<String>)> {
    db.read(|sql| Ok(sql.query(
        "SELECT name, first_seen_at, source_date, source_date_kind FROM scan_candidate ORDER BY name",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?)).await.unwrap()
}

#[tokio::test]
async fn stored_dates_order_the_list_and_survive_candidate_replacement() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    db.add_watched_import_folder(root).await.unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    for (name, date) in [
        ("A", Some(FolderDate::Created(100))),
        ("B", Some(FolderDate::AddedToDirectory(200))),
        ("C", None),
    ] {
        db.save_folder_scan_item_with_initial_source(
            root,
            generation,
            &ScanItem::Valid(candidate(root, name)),
            crate::config::DefaultImportMetadataSource::FindOnline,
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
                now().timestamp_millis(),
                Some(100),
                Some("created".into())
            ),
            (
                "B".into(),
                now().timestamp_millis(),
                Some(200),
                Some("added_to_directory".into())
            ),
            ("C".into(), now().timestamp_millis(), None, None),
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
        Arc::new(FixedClock(now() + chrono::Duration::days(1))),
        db.inner.ids.clone(),
    );
    let generation = later.begin_folder_scan(root).await.unwrap();
    for name in ["A", "B", "C"] {
        let original = candidate(root, name);
        // Both the no-op/discovered path and a file-shape replacement retain
        // discovery, even when this observation supplies no filesystem date.
        later
            .save_folder_scan_item(root, generation, &ScanItem::Discovered(original.clone()))
            .await
            .unwrap();
        let mut changed = original;
        changed.files.files[0].file.size += 1;
        later
            .save_folder_scan_item(root, generation, &ScanItem::Valid(changed))
            .await
            .unwrap();
    }
    later
        .finish_folder_scan(root, generation, None)
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
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();
    let item = scanned(&db, root, "Album").await;
    // The explicit pre-date-tracking shape retained by the migration.
    db.call(|sql| {
        sql.execute("UPDATE scan_candidate SET first_seen_at = NULL", [])?;
        Ok(())
    })
    .await
    .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    db.save_folder_scan_item_with_initial_source(
        root,
        generation,
        &ScanItem::Valid(item),
        crate::config::DefaultImportMetadataSource::FindOnline,
        Some(FolderDate::Created(123)),
    )
    .await
    .unwrap();
    assert_eq!(
        dates(&db).await,
        vec![(
            "Album".into(),
            now().timestamp_millis(),
            Some(123),
            Some("created".into())
        )]
    );
}
