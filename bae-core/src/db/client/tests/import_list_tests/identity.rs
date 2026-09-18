//! What a row states about its release's identity: the names its folder
//! carries, the catalogs its documents describe it in, and which name tied
//! its files to the record — each read once, so the pane's row and the
//! queue's agree.

use super::*;

/// A row states the names its folder carries: one line per value, folded here
/// so neither surface decides that two scans of one barcode are one line, and
/// tagged with every surface the value was read from.
///
/// A catalog number is one of them once somebody says this disc carries it —
/// until then it is one of extraction's guesses, which the row leaves alone.
#[tokio::test]
async fn a_row_states_the_names_its_folder_carries() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict_with_marks(&db, &candidate, "mb-verdict").await;

    let barcode = crate::import::ReleaseMarkLine {
        kind: crate::import::MarkKind::Barcode,
        value: "0075678164521".to_string(),
        origins: vec![
            crate::signals::SignalOrigin::Artwork,
            crate::signals::SignalOrigin::CueSheet,
        ],
    };
    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    assert_eq!(
        rows(&projection)[0].marks,
        vec![barcode.clone()],
        "nobody has chosen the folder's catalog number, so the row states none"
    );

    choose_catalogs(&db, &candidate, &["7559-60691-2"]).await;

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    assert_eq!(
        rows(&projection)[0].marks,
        vec![
            barcode,
            crate::import::ReleaseMarkLine {
                kind: crate::import::MarkKind::CatalogNumber,
                value: "7559-60691-2".to_string(),
                origins: vec![crate::signals::SignalOrigin::FolderName],
            },
        ],
    );

    let detail = db
        .load_import_candidate(&candidate.path.to_string_lossy())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .resolve(&crate::import::TriageRuntimeFacts::default());
    assert_eq!(
        detail.row.marks,
        rows(&projection)[0].marks,
        "the pane's row reads the same names the queue's does"
    );
}

/// Which catalogs describe a pick is read off its archived documents once:
/// the queue's row and the pane's detail name the same records, in the order
/// surfaces list catalogs — the pick's own, the release its document links,
/// and the two its release group links.
#[tokio::test]
async fn the_row_and_the_pane_name_the_same_records() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;
    db.save_source_release_payloads(&[
        DbSourceReleasePayload {
            source: PayloadSource::MusicBrainz,
            source_release_id: "mb-linked".to_string(),
            json: musicbrainz_release_linked_out("mb-linked", "mb-group").to_string(),
            fetched_at: fixed_now(),
        },
        DbSourceReleasePayload {
            source: PayloadSource::MusicBrainzReleaseGroup,
            source_release_id: "mb-group".to_string(),
            json: release_group_linked_out().to_string(),
            fetched_at: fixed_now(),
        },
    ])
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
                record: crate::import::MetadataRef::new(
                    Catalog::MusicBrainz,
                    "mb-linked".to_string(),
                ),
                partners: vec![],
            }),
        )
        .await
        .unwrap();

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    let row = rows(&projection).remove(0);
    let crate::import::triage::TriageReading::Identified { records } = &row.reading else {
        panic!("a picked row reads as identified, got {:?}", row.reading);
    };
    assert_eq!(
        records
            .iter()
            .map(|record| (record.catalog, record.key.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (Catalog::MusicBrainz, "mb-linked"),
            (Catalog::Discogs, "4242"),
            (Catalog::AllMusic, "mw0000424242"),
            (Catalog::Wikidata, "Q424242"),
        ]
    );

    let detail = db
        .load_import_candidate(&candidate.path.to_string_lossy())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .resolve(&crate::import::TriageRuntimeFacts::default());
    assert_eq!(
        detail.row.reading, row.reading,
        "the pane's row names the records the queue's does"
    );
}

/// A candidate nothing has read states no names, and the row says so rather
/// than drawing an empty line.
#[tokio::test]
async fn a_row_nothing_has_read_states_no_names() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    assert!(rows(&projection)[0].marks.is_empty());
}

/// The row says what tied the folder's files to the record its draft reads,
/// read off the verdict's own match rows.
#[tokio::test]
async fn a_row_states_what_tied_its_files_to_its_record() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    assert!(crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            candidate: crate::import::CandidateAsRead {
                content_hash: candidate.files.content_hash(),
                file_edit_revision: 0,
                metadata_revision: 0,
            },
            folder_path: candidate.path.to_string_lossy().into_owned(),
            verdict: verdict("mb-verdict", None),
            signals: crate::signals::Signals {
                disc_id: crate::signals::DiscIdSignal::Absent { track_count: 1 },
                verification: None,
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                durations: crate::import::probe::SourceDurations::totalling(1_000),
            },
            metadata: Some(crate::import::CandidateMetadataDraft {
                draft: crate::import::pane::blank_candidate_draft(&candidate.files),
                source_discogs_artist_ids: Default::default(),
                provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
                    record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-verdict"),
                    partners: Vec::new(),
                }),
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            }),
        })
        .await
        .unwrap());

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    assert_eq!(
        rows(&projection)[0].identified_by,
        Some(crate::import::MarkKind::DiscId),
    );
}

/// A folder nobody has settled a record for was tied to nothing, however many
/// releases its lookups named.
#[tokio::test]
async fn a_row_with_no_record_was_tied_by_nothing() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    assert_eq!(rows(&projection)[0].identified_by, None);
}

/// A run that settles on a record whose catalog number the folder prints
/// chooses that number as it lands — so the row and the import preparation
/// carry the catalog line with nobody touching the toolbar.
#[tokio::test]
async fn a_settled_pick_states_the_number_the_folder_prints() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    let mut edit = crate::import::pane::blank_candidate_draft(&candidate.files).release_edit();
    edit.pressing.catalog_number = "NJ 8255".to_string();
    assert!(crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            candidate: crate::import::CandidateAsRead {
                content_hash: candidate.files.content_hash(),
                file_edit_revision: 0,
                metadata_revision: 0,
            },
            folder_path: candidate.path.to_string_lossy().into_owned(),
            verdict: verdict("mb-verdict", None),
            signals: crate::signals::Signals {
                disc_id: crate::signals::DiscIdSignal::Absent { track_count: 1 },
                verification: None,
                barcode: crate::signals::BarcodeSignal::Absent,
                text: crate::signals::TextSignal::Settled {
                    catalogs: vec![crate::signals::SourcedValue::new(
                        "NJ-8255".to_string(),
                        crate::signals::SignalOrigin::FolderName,
                    )],
                    free_text: Vec::new(),
                },
                text_pool: vec![crate::signals::TextLine {
                    text: "NJ-8255".to_string(),
                    origin: crate::signals::SignalOrigin::FolderName,
                    file: None,
                    region: None,
                }],
                durations: crate::import::probe::SourceDurations::totalling(1_000),
            },
            metadata: Some(crate::import::CandidateMetadataDraft {
                draft: crate::import::pane::candidate_draft_from_edit(edit).draft,
                source_discogs_artist_ids: Default::default(),
                provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
                    record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-verdict"),
                    partners: Vec::new(),
                }),
                cover: None,
                assets: crate::import::CandidatePreparedAssets::default(),
            }),
        })
        .await
        .unwrap());

    let catalog = crate::import::ReleaseMarkLine {
        kind: crate::import::MarkKind::CatalogNumber,
        value: "NJ-8255".to_string(),
        origins: vec![crate::signals::SignalOrigin::FolderName],
    };
    let projection = db
        .load_import_list(request(TriageTab::Pending).await)
        .await
        .unwrap();
    assert_eq!(rows(&projection)[0].marks, vec![catalog]);
    let preparation = db
        .load_import_candidate_preparation(&candidate.files.content_hash())
        .await
        .unwrap()
        .expect("the settled candidate is prepared");
    assert_eq!(
        preparation.marks,
        vec![crate::import::ReleaseMark {
            kind: crate::import::MarkKind::CatalogNumber,
            sighting: crate::signals::SourcedValue::new(
                "NJ-8255".to_string(),
                crate::signals::SignalOrigin::FolderName,
            ),
        }]
    );
}
