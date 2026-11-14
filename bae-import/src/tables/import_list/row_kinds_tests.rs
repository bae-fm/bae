//! What a window of the import list reads for each kind of row: the
//! statements it runs and the time it takes, as the window's rows grow.

use crate::folder_scanner::{CandidateFile, FileRole, ScanItem, ScannedFile};
use crate::tables::import_list::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_store::test_fixtures::window_cost::{report, report_cost, LIMIT, RUNS};
use std::time::{Duration, Instant};

/// A kind of row a window holds, by what its read has to look up.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A release this device imported, its file waiting to upload.
    Imported,
    /// A release linked to a pressing.
    Linked,
    /// A folder that failed validation.
    Invalid,
    /// A current candidate holding no draft row, its folder's image its cover.
    FolderCover,
    /// A candidate whose cover is a picture inside its audio file.
    EmbeddedCover,
}

impl Kind {
    const ALL: [Kind; 5] =
        [Kind::Imported, Kind::Linked, Kind::Invalid, Kind::FolderCover, Kind::EmbeddedCover];

    /// The tab that lists this kind.
    fn tab(self) -> crate::TriageTab {
        match self {
            Kind::Imported => crate::TriageTab::Done,
            Kind::Invalid => crate::TriageTab::Skipped,
            Kind::Linked | Kind::FolderCover | Kind::EmbeddedCover => crate::TriageTab::Pending,
        }
    }
}

const PICKED: &str = "mb-picked";

/// `rows` rows of `kind` in a library of their own, written through the
/// import's own writes.
async fn library_of(kind: Kind, rows: u64) -> Library {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let paths: Vec<String> = (0..rows).map(|row| format!("Album {row:04}")).collect();
    match kind {
        Kind::Imported => {
            let pressing = bae_catalogs::records::ReleaseRecord::new(
                &bae_catalogs::records::MetadataRef::new(
                    bae_catalogs::catalog::Catalog::MusicBrainz,
                    "mb-release",
                ),
                Some("mb-group".to_string()),
            );
            for path in &paths {
                lib.run(lib.scan(path)).await;
                lib.run(lib.import_uploading(
                    lib.candidate(path),
                    path,
                    "2026-01-01T00:00:00Z",
                    "Album Title",
                    vec![pressing.clone()],
                ))
                .await;
            }
        }
        Kind::Linked => {
            for (row, path) in paths.iter().enumerate() {
                let record = linked_release(&tables, row).await;
                lib.run(
                    lib.scan(path)
                        + lib.link(
                            path,
                            Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                                record,
                                partners: vec![],
                            })),
                        ),
                )
                .await;
            }
        }
        Kind::Invalid => {
            for path in &paths {
                lib.run(lib.scan_invalid(path, None)).await;
            }
        }
        Kind::FolderCover => {
            for path in &paths {
                let mut candidate = lib.candidate(path);
                candidate.files.files.push(CandidateFile {
                    file: ScannedFile::new(
                        candidate.path.join("cover.jpg"),
                        "cover.jpg".to_string(),
                        500,
                        1,
                    ),
                    role: FileRole::Artwork,
                });
                // Imported and then deleted from the library, the folder is
                // back in Found holding no draft row.
                lib.run(
                    lib.store(ScanItem::Valid(candidate.clone()), None)
                        + lib.import_of(
                            candidate,
                            path,
                            "2026-01-02T00:00:00Z",
                            "Album Title",
                            Vec::new(),
                        )
                        + lib.release_deleted(path),
                )
                .await;
            }
        }
        Kind::EmbeddedCover => {
            for path in &paths {
                lib.run(lib.scan(path)).await;
                let candidate = lib.candidate(path);
                let hash = candidate.files.content_hash();
                let draft = tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft;
                let audio = candidate.files.audio().next().unwrap();
                let relative_path = audio.relative_path.clone();
                let snapshot = crate::file_tag_snapshot::FileTagSnapshot {
                    scan_generation: 1,
                    file_edit_revision: 0,
                    files: vec![crate::file_tag_snapshot::FileTagFact {
                        observation: crate::file_tag_snapshot::FileObservation {
                            relative_path: relative_path.clone(),
                            size: audio.version.size,
                            modified_at_ns: audio.version.modified_at_ns,
                        },
                        tags: crate::file_tag_snapshot::TagReading::Stated(
                            crate::file_tag_snapshot::FileTags::untagged(),
                        ),
                    }],
                    embedded_cover: Some(crate::file_tag_snapshot::EmbeddedCoverFact {
                        source_relative_path: relative_path.clone(),
                        content_type: bae_util::content_type::ContentType::Jpeg,
                        data: vec![1, 2, 3, 4],
                    }),
                };
                tables
                    .apply_file_metadata(
                        &lib.root,
                        &candidate.key(),
                        &as_read(&tables, &hash).await,
                        &snapshot,
                        &draft,
                        Some(&crate::cover_art::PreparedCover::Embedded(relative_path)),
                    )
                    .await
                    .unwrap();
            }
        }
    }
    lib
}

/// The `row`th row's own linked release, stored: a MusicBrainz release in a
/// release group of its own that links a Discogs release, the group read to
/// be an AllMusic album none of its records reaches, so the release reads
/// its own records and the album that statement joins to them.
async fn linked_release(
    tables: &crate::tables::ImportTables,
    row: usize,
) -> bae_catalogs::records::LookupRef {
    let key = format!("{PICKED}-{row:04}");
    let group = format!("mb-group-{row:04}");
    let mut document = musicbrainz_release(&key, "Picked Album");
    document["release-group"] = serde_json::json!({ "id": group });
    document["relations"] = serde_json::json!([{
        "target-type": "url",
        "type": "discogs",
        "url": { "resource": format!("https://www.discogs.com/release/{}", 4000 + row) }
    }]);
    let record = bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, key);
    tables
        .save_source_release(
            &bae_catalogs::payloads::ReleasePayloads::for_test(
                record.clone(),
                document.to_string(),
                Vec::new(),
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    tables
        .replace_group_album_links(vec![(
            group,
            vec![bae_catalogs::records::MetadataRef::new(
                bae_catalogs::catalog::Catalog::AllMusic,
                format!("mw{row:010}"),
            )],
        )])
        .await
        .unwrap();
    record
}

/// One read of the first `rows` items of `kind`'s tab, and the statements it
/// ran.
async fn read_window(lib: &Library, kind: Kind, rows: u64) -> (crate::ImportWindow, usize) {
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let list = open_in_path_order(&lib.db, kind.tab()).await;
    let window = bae_store::ListWindow::Forward { from: None, until: None, limit: rows };
    let counter = lib.db.track_work_for_test().await.unwrap();
    let read = tables.read_import_window(list, window).await.unwrap();
    (read, counter.take().app_read_statements)
}

/// What one window of `LIMIT` rows of each kind costs to read: the median of
/// [`RUNS`] reads, reported under `import_list_rows` with the kind named as
/// its order, and the statements one read runs beside it. A measurement, not
/// a check: run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     import_list_row_kinds_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn import_list_row_kinds_cost() {
    for kind in Kind::ALL {
        let lib = library_of(kind, LIMIT).await;
        let (_, statements) = read_window(&lib, kind, LIMIT).await;
        let mut runs: Vec<Duration> = Vec::with_capacity(RUNS);
        let mut rows = 0;
        for _ in 0..RUNS {
            let started = Instant::now();
            let (read, _) = read_window(&lib, kind, LIMIT).await;
            runs.push(started.elapsed());
            rows = read.read.rows.len();
        }
        runs.sort();
        report(
            "import_list_rows",
            &format!("{kind:?}"),
            "window",
            "top",
            0,
            LIMIT,
            (runs[RUNS / 2], rows),
        );
        report_cost(&format!("import_list_rows/{kind:?}/statements"), statements as f64, "count");
    }
}

/// The statements a window of `rows` rows of `kind` reads, whose rows are
/// all of that kind.
async fn statements_of(kind: Kind, rows: u64) -> usize {
    let lib = library_of(kind, rows).await;
    let (read, statements) = read_window(&lib, kind, rows).await;
    assert_eq!(read.read.rows.len() as u64, rows, "{kind:?}: the window holds every row");
    statements
}

#[tokio::test]
async fn a_window_reads_imported_rows_in_statements_its_size_does_not_change() {
    assert_eq!(statements_of(Kind::Imported, 2).await, statements_of(Kind::Imported, 10).await);
}

/// An imported row whose file waits in this device's upload queue shows it
/// waiting, read with the window's other imported rows'.
#[tokio::test]
async fn an_imported_row_shows_its_upload_waiting_whatever_the_window_holds() {
    let lib = library_of(Kind::Imported, 3).await;
    let (read, _) = read_window(&lib, Kind::Imported, 3).await;
    for row in window_rows(&read) {
        assert_eq!(
            row.upload,
            Some(bae_records::UploadProgress { queued: 1, uploading: 0 }),
            "{}",
            row.display_path
        );
    }
}

#[tokio::test]
async fn an_imported_row_shows_its_records_whatever_the_window_holds() {
    let lib = library_of(Kind::Imported, 3).await;
    let (read, _) = read_window(&lib, Kind::Imported, 3).await;
    for row in window_rows(&read) {
        assert!(
            matches!(row.reading, crate::triage::TriageReading::Identified { .. }),
            "{} reads its release's records",
            row.display_path
        );
    }
}

#[tokio::test]
async fn a_window_reads_linked_rows_in_statements_its_size_does_not_change() {
    assert_eq!(statements_of(Kind::Linked, 2).await, statements_of(Kind::Linked, 10).await);
}

/// A linked row names what its stored release, read whole, says it is: its
/// own records and the album a kept statement joins to them, whatever else
/// the window holds.
#[tokio::test]
async fn a_linked_row_names_the_records_its_whole_release_does() {
    let lib = library_of(Kind::Linked, 3).await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let (read, _) = read_window(&lib, Kind::Linked, 3).await;
    for row in window_rows(&read) {
        let Some(crate::ReleaseLink::Pressing(link)) = &row.release_link else {
            panic!("{} is linked to a pressing", row.display_path);
        };
        let whole = tables.load_source_release(&link.record).await.unwrap().unwrap();
        let crate::triage::TriageReading::Identified { records } = &row.reading else {
            panic!("{} reads as identified, got {:?}", row.display_path, row.reading);
        };
        assert_eq!(
            records,
            &bae_catalogs::source_release::claimed_records(&[whole.records()]),
            "{}",
            row.display_path
        );
        assert!(
            records
                .iter()
                .any(|record| record.catalog() == bae_catalogs::catalog::Catalog::AllMusic),
            "{} names the album its group was read to be: {records:?}",
            row.display_path
        );
    }
}

#[tokio::test]
async fn a_window_reads_invalid_rows_in_statements_its_size_does_not_change() {
    assert_eq!(statements_of(Kind::Invalid, 2).await, statements_of(Kind::Invalid, 10).await);
}

#[tokio::test]
async fn a_window_reads_no_draft_rows_in_statements_its_size_does_not_change() {
    assert_eq!(
        statements_of(Kind::FolderCover, 2).await,
        statements_of(Kind::FolderCover, 10).await
    );
}

#[tokio::test]
async fn a_window_reads_embedded_cover_rows_in_statements_its_size_does_not_change() {
    assert_eq!(
        statements_of(Kind::EmbeddedCover, 2).await,
        statements_of(Kind::EmbeddedCover, 10).await
    );
}
