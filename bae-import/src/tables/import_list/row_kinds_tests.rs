//! What a window of the import list reads for each kind of row: the
//! statements it runs and the time it takes, as the window's rows grow.

use crate::folder_scanner::{CandidateFile, FileRole, ScanItem, ScannedFile};
use crate::tables::import_list::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_records::test_fixtures::aggregate_ordering::insert_file;
use bae_store::test_fixtures::window_cost::{report, report_cost, LIMIT, RUNS};
use std::time::{Duration, Instant};

/// A kind of row a window holds, by what its read has to look up.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A release this device imported, with an upload queued.
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
                lib.run(lib.import_of(
                    lib.candidate(path),
                    path,
                    "2026-01-01T00:00:00Z",
                    "Album Title",
                    vec![pressing.clone()],
                ))
                .await;
                let dir = lib._tmp.as_ref().unwrap().path();
                insert_file(
                    &lib.db,
                    dir,
                    bae_records::NewReleaseFile {
                        id: bae_util::test_ids::test_uuid(&format!("file {path}")),
                        release_id: lib.release_id(path),
                        original_filename: "01.flac".to_string(),
                        content_type: bae_util::content_type::ContentType::from_mime("audio/flac"),
                        source_audio: None,
                        created_at: bae_store::test_fixtures::fixed_now(),
                    },
                    1_000,
                )
                .await;
            }
        }
        Kind::Linked => {
            let record = bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, PICKED);
            tables
                .save_source_release(
                    &bae_catalogs::payloads::ReleasePayloads::for_test(
                        record.clone(),
                        musicbrainz_release(PICKED, "Picked Album").to_string(),
                        Vec::new(),
                    )
                    .extract()
                    .unwrap(),
                )
                .await
                .unwrap();
            for path in &paths {
                lib.run(
                    lib.scan(path)
                        + lib.link(
                            path,
                            Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                                record: record.clone(),
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

/// One read of the first `rows` items of `kind`'s tab, and the statements it
/// ran.
async fn read_window(lib: &Library, kind: Kind, rows: u64) -> (crate::ImportListProjection, usize) {
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let request = list_request(&lib.db, kind.tab(), [(0, rows)]).await;
    let counter = lib.db.track_work_for_test().await.unwrap();
    let read = tables.load_import_list(request).await.unwrap();
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
            rows = read.windows[0].rows.len();
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
