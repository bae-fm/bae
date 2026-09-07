use super::*;
use crate::import::selection::ImportSelection;
use crate::import::triage::CandidateAction;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn eligible(value: &ImportSelection, action: CandidateAction) -> Vec<&str> {
    value
        .offers
        .iter()
        .find(|offer| offer.action == action)
        .map(|offer| {
            offer
                .candidates
                .iter()
                .map(|candidate| candidate.key.as_str())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn bulk_selection_reads_only_selected_facts_and_never_opens_editors() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let selected = scanned(&db, root.to_str().unwrap(), "Selected Album").await;
    save_verdict(&db, &selected, "selected-release").await;
    let other_root = tmp.path().join("other");
    std::fs::create_dir_all(&other_root).unwrap();
    let other_root = other_root.to_str().unwrap();
    db.add_watched_import_folder(other_root).await.unwrap();
    let generation = db.begin_folder_scan(other_root).await.unwrap();
    let mut other = candidate(other_root, "Unselected Album");
    other.files.files[0].file =
        ScannedFile::new(other.path.join("02.flac"), "02.flac".to_owned(), 2_000, 1)
            .with_test_flac_audio();
    db.save_folder_scan_item_with_initial_source(
        other_root,
        generation,
        &ScanItem::Valid(other.clone()),
        crate::config::DefaultImportMetadataSource::FindOnline,
        None,
    )
    .await
    .unwrap();
    db.finish_folder_scan(other_root, generation, None)
        .await
        .unwrap();
    assert_ne!(selected.files.content_hash(), other.files.content_hash());
    let other_hash = other.files.content_hash();
    // An unrelated invalid draft would fail a whole-list read, so the selection
    // must reach its key directly, even when no list page has been loaded.
    db.call(move |sql| {
        sql.execute(
            "DELETE FROM import_candidate_edit WHERE content_hash = ?",
            [other_hash],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let key = selected.path.to_string_lossy().into_owned();
    let keys = [key.clone()].into_iter().collect();
    let reads = Arc::new(AtomicUsize::new(0));
    let read_count = reads.clone();
    let mut query = db.inner.handle.subscribe(move |sql| {
        read_count.fetch_add(1, Ordering::SeqCst);
        super::super::super::import_selection::load_import_selection_on(&sql, &keys)
            .map_err(CovenError::from)
    });
    let first = query.next().await.unwrap().resolve(&BTreeMap::new());
    assert_eq!(first.candidate_keys, vec![key.clone()]);
    assert_eq!(
        eligible(&first, CandidateAction::ImportReady),
        vec![key.as_str()]
    );

    db.call(|sql| {
        sql.execute("DELETE FROM scan_candidate_file", [])?;
        sql.execute("DELETE FROM import_candidate_cover", [])?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(80), query.next())
            .await
            .is_err()
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "file and artwork changes do not execute the bulk read"
    );

    let root_owned = root.to_string_lossy().into_owned();
    db.remove_watched_import_folder(&root_owned).await.unwrap();
    let removed = tokio::time::timeout(Duration::from_secs(2), query.next())
        .await
        .unwrap()
        .unwrap()
        .resolve(&BTreeMap::new());
    assert!(removed.candidate_keys.is_empty());
}

#[tokio::test]
async fn bulk_readiness_tracks_canonical_artists_and_raw_years() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let candidate = scanned(&db, root.to_str().unwrap(), "Album").await;
    let hash = candidate.files.content_hash();
    let key = candidate.path.to_string_lossy().into_owned();
    let edited_hash = hash.clone();
    db.call(move |sql| {
        sql.execute("INSERT INTO artists (id, name, created_at, _updated_at) VALUES ('019b2c44-98c0-7000-8000-000000000001', 'Artist', ?, ?)", params![now().to_rfc3339(), now().to_rfc3339()])?;
        sql.execute("UPDATE import_candidate_edit SET album_title = 'Album' WHERE content_hash = ?", [&edited_hash])?;
        sql.execute("INSERT INTO import_candidate_album_artist_assignment (content_hash, position, assignment_kind, artist_id) VALUES (?, 0, 'existing', '019b2c44-98c0-7000-8000-000000000001')", [&edited_hash])?;
        Ok(())
    }).await.unwrap();
    let mut query = db.subscribe_import_selection([key.clone()].into_iter().collect());
    let next = query.next().await.unwrap().resolve(&BTreeMap::new());
    assert_eq!(
        eligible(&next, CandidateAction::ImportReady),
        vec![key.as_str()]
    );
    db.call(|sql| {
        sql.execute(
            "UPDATE artists SET name = ' ' WHERE id = '019b2c44-98c0-7000-8000-000000000001'",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let next = query.next().await.unwrap().resolve(&BTreeMap::new());
    assert!(eligible(&next, CandidateAction::ImportReady).is_empty());
    let edited_hash = hash.clone();
    db.call(move |sql| {
        sql.execute(
            "UPDATE artists SET name = 'Renamed Artist' WHERE id = '019b2c44-98c0-7000-8000-000000000001'",
            [],
        )?;
        sql.execute(
            "UPDATE import_candidate_edit SET album_year = 'not-a-year' WHERE content_hash = ?",
            [edited_hash],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    // The invalid year leaves the same unavailable action set, so query equality
    // intentionally suppresses it. Read the actual query directly to inspect it.
    let current = db
        .subscribe_import_selection([key.clone()].into_iter().collect())
        .next()
        .await
        .unwrap()
        .resolve(&BTreeMap::new());
    assert!(eligible(&current, CandidateAction::ImportReady).is_empty());
    db.call(move |sql| {
        sql.execute(
            "UPDATE import_candidate_edit SET album_year = '2001' WHERE content_hash = ?",
            [hash],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let next = query.next().await.unwrap().resolve(&BTreeMap::new());
    assert_eq!(
        eligible(&next, CandidateAction::ImportReady),
        vec![key.as_str()]
    );
    let runtime = [(
        key.clone(),
        crate::import::TriageRuntimeFacts {
            identification: Some(crate::import::IdentificationStatus::Running),
            importing: false,
        },
    )]
    .into_iter()
    .collect();
    let current = db
        .subscribe_import_selection([key].into_iter().collect())
        .next()
        .await
        .unwrap()
        .resolve(&runtime);
    assert!(eligible(&current, CandidateAction::ImportReady).is_empty());
}

#[tokio::test]
async fn visible_list_extraction_does_not_parse_provider_documents() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let candidate = scanned(&db, root.to_str().unwrap(), "Album").await;
    save_verdict(&db, &candidate, "verdict").await;
    db.save_source_release_payloads(&[DbSourceReleasePayload {
        source: PayloadSource::MusicBrainz,
        source_release_id: "picked".into(),
        json: serde_json::json!({"id":"picked", "artist-credit":"invalid editor metadata"})
            .to_string(),
        fetched_at: now(),
    }])
    .await
    .unwrap();
    let draft = db
        .load_import_candidate_pane_rows(&candidate.files.content_hash())
        .await
        .unwrap()
        .draft
        .release_edit();
    crate::import::CandidatePreparations::new(db.clone())
        .replace_metadata(
            &candidate.files.content_hash(),
            &candidate.path.to_string_lossy(),
            &draft,
            Some(&MetadataProvenance::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: "picked".into(),
                partners: vec![],
            }),
        )
        .await
        .unwrap();
    let input = request(TriageTab::Pending).await;
    db.read(move |sql| {
        super::super::super::import_list::ImportListQuery::new(crate::import::volume::volume_kind)
            .read(&sql, &input)
    })
    .await
    .expect("the SQLite extraction must only return owned provider bytes");
    assert!(
        db.load_import_list(request(TriageTab::Pending).await)
            .await
            .is_err(),
        "the list processor must surface malformed metadata"
    );
}

#[tokio::test]
async fn editor_and_bulk_ignore_artist_errors_on_dropped_tracks() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let candidate = scanned(&db, root.to_str().unwrap(), "Album").await;
    let key = candidate.path.to_string_lossy().into_owned();
    let hash = candidate.files.content_hash();
    db.call(move |sql| {
        sql.execute("UPDATE import_candidate_edit SET album_title = 'Album' WHERE content_hash = ?", [&hash])?;
        sql.execute("INSERT INTO import_candidate_album_artist_assignment (content_hash, position, assignment_kind, name) VALUES (?, 0, 'new', 'Artist')", [&hash])?;
        sql.execute("UPDATE import_candidate_track SET dropped = 1, file_kind = NULL, file_id = NULL, sheet_id = NULL, slice_index = NULL, artist_assignment_kind = 'explicit' WHERE content_hash = ?", [&hash])?;
        sql.execute("INSERT INTO import_candidate_track_artist_assignment (content_hash, track_id, position, assignment_kind, name) SELECT content_hash, track_id, 0, 'new', ' ' FROM import_candidate_track WHERE content_hash = ?", [&hash])?;
        Ok(())
    }).await.unwrap();
    let bulk = db
        .subscribe_import_selection([key.clone()].into_iter().collect())
        .next()
        .await
        .unwrap()
        .resolve(&BTreeMap::new());
    assert_eq!(
        eligible(&bulk, CandidateAction::ImportReady),
        vec![key.as_str()]
    );
    let editor = db
        .load_import_candidate(&key)
        .await
        .unwrap()
        .unwrap()
        .resolve(&crate::import::TriageRuntimeFacts::default());
    assert!(
        editor.row.actions.contains(&CandidateAction::ImportReady),
        "opening the editor must preserve the committed draft's readiness"
    );
}

#[tokio::test]
async fn list_extraction_does_not_access_the_watched_volume() {
    let (db, tmp) = empty_db().await;
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    scanned(&db, root.to_str().unwrap(), "Album").await;
    let input = request(TriageTab::Pending).await;
    let query = super::super::super::import_list::ImportListQuery::new(|_| {
        panic!("SQL extraction must not access the watched volume")
    });
    let rows = db
        .read(move |sql| query.read(&sql, &input))
        .await
        .expect("volume access happens only after SQL extraction returns");
    let processor = super::super::super::import_list::ImportListQuery::new(|_| {
        crate::import::volume::VolumeKind::Network
    });
    let value = processor
        .process(rows)
        .expect("resolve the owned volume facts");
    assert_eq!(value.summary.folder_scan_statuses.len(), 1);
    assert!(value.summary.folder_scan_statuses[0].on_network_volume);
}
