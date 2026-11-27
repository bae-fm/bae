//! A window's rows as the list reads them, from the tables alone: what each
//! release's row shows (its draft, its link's records, its cover, a Done row's
//! library release), a folder that failed validation, and whether a group's
//! header offers to combine.

use super::*;
use crate::folder_scanner::FolderReleaseDecision;
use crate::folder_scanner::{
    CandidateFile, FileRole, FileVersion, FolderCandidate, ScanItem, ScannedFile,
};
use crate::folder_scanner::{CategorizedFiles, ReleaseFileScope};
use crate::list::ImportListItem;
use crate::list::ImportListRead;
use crate::tables::import_list::tests::*;
use crate::tables::tests::*;
use crate::{MetadataProvenance, TriageTab};
use bae_catalogs::catalog::LookupCatalog;
use bae_identify::TerminalVerdict;
use bae_util::test_channels::HANG_GUARD;

#[path = "window_tests/done_rows.rs"]
mod done_rows;

/// Each header's group name and whether it offers to combine, in order.
fn combinable(read: &ImportListRead) -> Vec<(String, bool)> {
    read.windows[0]
        .rows
        .iter()
        .filter_map(|(item, _key)| match item {
            ImportListItem::GroupHeader(crate::ImportGroupHeader { group, .. }) => {
                Some((group.name.clone(), group.combinable))
            }
            _ => None,
        })
        .collect()
}

impl Library {
    /// The scan's reading of the folder at `display_path` that keeps its
    /// releases apart.
    fn kept_apart(&self, display_path: &str) -> Write {
        self.reading(display_path, FolderReleaseDecision::KeepAsSeparateReleases)
    }
}

/// A group header is where a folder read as several releases offers to be read
/// as one. A header that is only a path component the rows share offers
/// nothing: the folder that holds them — `Singles/Label`, the nearest one
/// nothing is stored for — is where that choice belongs, not every folder
/// above it.
#[tokio::test]
async fn only_a_group_over_a_folder_read_as_several_offers_to_combine() {
    let lib = Library::new().await;
    lib.run(
        lib.scan("Box/Disc 1")
            + lib.scan("Box/Disc 2")
            + lib.scan("Singles/Label/Series/One")
            + lib.scan("Singles/Label/Series/Two")
            + lib.kept_apart("Box")
            + lib.kept_apart("Singles/Label/Series"),
    )
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(combinable(&read), [("Box".to_string(), true), ("Singles".to_string(), false)]);
}

/// A folder nothing has settled either way, holding several releases below a
/// folder that keeps them apart, is combinable too — it is the nearest folder
/// above them to hold them all, and the header for it is where the choice
/// belongs.
#[tokio::test]
async fn a_folder_nothing_settled_over_several_releases_offers_to_combine() {
    let lib = Library::new().await;
    lib.run(
        lib.scan("Wrapper/Box/Disc 1")
            + lib.scan("Wrapper/Box/Disc 2")
            + lib.kept_apart("Wrapper/Box"),
    )
    .await;
    let read = lib.shown(view(TriageTab::Pending)).await;
    assert_eq!(combinable(&read), [("Wrapper".to_string(), true)]);
}

/// Whether a header offers to combine follows every write that decides it: a
/// reading stored or changed at its folder or at one inside it, and a release
/// of any tab placed below it or gone.
#[tokio::test]
async fn a_header_offers_to_combine_as_its_readings_and_rows_change() {
    let lib = Library::new().await;
    let offers = || async { combinable(&lib.shown(view(TriageTab::Pending)).await) };
    let singles = |offered: bool| [("Singles".to_string(), offered)];
    lib.run(lib.scan("Singles/Label/Series/One") + lib.scan("Singles/Label/Series/Two")).await;
    assert_eq!(offers().await, singles(false), "Series is nearer to both");
    lib.run(lib.kept_apart("Singles/Label/Series")).await;
    assert_eq!(offers().await, singles(false), "Label is nearer to both");
    lib.run(lib.scan("Singles/Loose") + lib.imported("Singles/Loose", "2026-01-02T00:00:00Z"))
        .await;
    assert_eq!(
        offers().await,
        singles(true),
        "a release on Done right below Singles is nearest to it"
    );
    lib.run(lib.unscan("Singles/Loose")).await;
    assert_eq!(offers().await, singles(false), "and gone, it is not");
    lib.run(lib.kept_apart("Singles/Label")).await;
    assert_eq!(offers().await, singles(true), "every folder between is settled");
    lib.run(lib.kept_apart("Singles")).await;
    assert_eq!(offers().await, singles(true), "Singles keeps them apart");
    lib.run(lib.reading("Singles", FolderReleaseDecision::CombineAsOneRelease)).await;
    assert_eq!(offers().await, singles(false), "Singles is read as one");
}

/// A candidate the user picked a release for reads as identified in the
/// records its stored release describes — not in whatever the verdict named.
#[tokio::test]
async fn a_picked_row_reads_in_the_stored_releases_records() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    save_verdict(&db, &candidate, "mb-verdict").await;

    tables
        .save_source_release(
            &bae_catalogs::payloads::ReleasePayloads::for_test(
                bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-picked"),
                musicbrainz_release("mb-picked", "Picked Album").to_string(),
                Vec::new(),
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    let draft = tables
        .load_import_candidate_pane_rows(&candidate.files.content_hash())
        .await
        .unwrap()
        .draft
        .release_edit();
    crate::tables::ImportTables::new(db.clone())
        .replace_metadata(
            &candidate.files.content_hash(),
            &crate::CandidateKey::folder(&candidate.path),
            &draft,
            Some(&MetadataProvenance::ExternalRelease {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    "mb-picked".to_string(),
                ),
            }),
            &[],
        )
        .await
        .unwrap();
    crate::tables::ImportTables::new(db.clone())
        .replace_link(
            &candidate.files.content_hash(),
            Some(&crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    LookupCatalog::MusicBrainz,
                    "mb-picked",
                ),
                partners: vec![],
            })),
            &[],
        )
        .await
        .unwrap();

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    let reading = &rows(&projection)[0].reading;
    let crate::triage::TriageReading::Identified { records, .. } = reading else {
        panic!("a linked row reads as identified, got {reading:?}");
    };
    assert_eq!(
        records,
        &[bae_catalogs::records::ReleaseRecord::new(
            &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-picked"),
            None,
        )]
    );
}

/// The list row carries the applied draft's title and the chosen cover,
/// which a rescan keeps.
#[tokio::test]
async fn the_list_projects_the_applied_draft_and_cover() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let mut candidate = candidate(&root, "Album");
    candidate.files.files.push(CandidateFile {
        file: ScannedFile::new(
            root.join("Album").join("cover.jpg"),
            "cover.jpg".to_string(),
            500,
            1,
        ),
        role: FileRole::Artwork,
    });
    candidate.files.files.push(CandidateFile {
        file: ScannedFile::new(
            root.join("Album").join("folder.jpg"),
            "folder.jpg".to_string(),
            600,
            1,
        ),
        role: FileRole::Artwork,
    });
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    save_verdict(&db, &candidate, "mb-verdict").await;
    let hash = candidate.files.content_hash();

    let pane = crate::tables::import_state::tests::PaneControls::of(&db, &candidate);
    pane.choose_cover(&hash, &crate::cover_art::PreparedCover::Local("cover.jpg".to_string()))
        .await
        .unwrap();
    pane.type_field(&hash, crate::CandidateEditField::AlbumTitle, "Edited Album").await.unwrap();
    pane.type_field(&hash, crate::CandidateEditField::PressingYear, "1991").await.unwrap();

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    let row = rows(&projection).remove(0);
    let summary = row.reading.album().cloned().expect("the row carries its applied draft");
    assert_eq!(summary, crate::AlbumLine::TitleAlone { title: "Edited Album".to_string() });
    assert_eq!(
        row.cover,
        Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
            file: FileVersion {
                path: root.join("Album").join("cover.jpg"),
                size: 500,
                modified_at_ns: 1,
            },
        }))
    );

    pane.type_field(&hash, crate::CandidateEditField::AlbumTitle, "").await.unwrap();
    let untitled = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert!(matches!(
        rows(&untitled).remove(0).reading.album(),
        Some(crate::AlbumLine::Untitled { .. })
    ));

    let content_hash = candidate.files.content_hash();
    pane.choose_cover(
        &content_hash,
        &crate::cover_art::PreparedCover::Local("folder.jpg".to_string()),
    )
    .await
    .unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(
        rows(&projection).remove(0).cover,
        Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
            file: FileVersion {
                path: root.join("Album").join("folder.jpg"),
                size: 600,
                modified_at_ns: 1,
            },
        })),
        "a rescan must retain the explicit cover"
    );

    crate::tables::ImportTables::new(db.clone())
        .replace_metadata(
            &content_hash,
            &crate::CandidateKey::folder(&candidate.path),
            &crate::pane::blank_candidate_draft(&candidate.files).release_edit(),
            None,
            &[],
        )
        .await
        .unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate)).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(
        rows(&projection).remove(0).cover,
        Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
            file: FileVersion {
                path: root.join("Album").join("folder.jpg"),
                size: 600,
                modified_at_ns: 1,
            },
        })),
        "a draft that brings no image of its own leaves the selection standing"
    );
}

/// A scan stores the folder's own cover, and the row shows it over a cover a
/// verdict found.
#[tokio::test]
async fn the_scan_stores_the_folders_own_cover() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let mut candidate = candidate(&root, "Album");
    candidate.files.files.push(CandidateFile {
        file: ScannedFile::new(
            root.join("Album").join("cover.jpg"),
            "cover.jpg".to_string(),
            500,
            1,
        ),
        role: FileRole::Artwork,
    });
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    assert_eq!(
        tables
            .load_import_candidate_pane_rows(&candidate.files.content_hash())
            .await
            .unwrap()
            .cover,
        Some(crate::CoverSelection::Local("cover.jpg".to_string()))
    );
    let detail = tables
        .load_import_candidate(&crate::CandidateKey::folder(&candidate.path))
        .await
        .unwrap()
        .expect("the candidate reads back");
    assert_eq!(
        detail.draft().unwrap().cover.clone().map(|cover| cover.selection()),
        Some(crate::CoverSelection::Local("cover.jpg".to_string()))
    );
    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    let row = rows(&projection).remove(0);
    assert!(row.reading.album().is_none());
    assert_eq!(
        row.cover,
        Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
            file: FileVersion {
                path: root.join("Album").join("cover.jpg"),
                size: 500,
                modified_at_ns: 1,
            },
        }))
    );

    let mut with_cover = verdict("release-with-cover", None);
    let TerminalVerdict::Found { findings, .. } = &mut with_cover else {
        unreachable!("verdict builds a found verdict");
    };
    findings.matches[0].cover_art = Some(bae_catalogs::remote_image::RemoteCover {
        image: bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/full.jpg".to_string(),
        ),
        label: bae_catalogs::remote_image::CoverLabel::Lead { release_group: false },
        source: Catalog::MusicBrainz,
        standing: bae_catalogs::remote_image::CoverStanding::Stated,
    });
    store_verdict(&db, &candidate, with_cover).await;
    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(
        rows(&projection).remove(0).cover,
        Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
            file: FileVersion {
                path: root.join("Album").join("cover.jpg"),
                size: 500,
                modified_at_ns: 1,
            },
        })),
        "a row shows the cover the candidate would commit with, and nothing a \
         verdict merely found"
    );
}

#[tokio::test]
async fn the_list_projects_the_persisted_embedded_file_metadata_cover() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    let hash = candidate.files.content_hash();
    let draft = tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft;
    let bytes = vec![1, 2, 3, 4];
    let snapshot = crate::file_tag_snapshot::FileTagSnapshot {
        scan_generation: 1,
        file_edit_revision: 0,
        files: vec![crate::file_tag_snapshot::FileTagFact {
            // Read after the scan: the file was touched, its size unchanged.
            observation: crate::file_tag_snapshot::FileObservation {
                relative_path: "01.flac".to_string(),
                size: 1_000,
                modified_at_ns: 5,
            },
            tags: crate::file_tag_snapshot::TagReading::Stated(
                crate::file_tag_snapshot::FileTags::untagged(),
            ),
        }],
        embedded_cover: Some(crate::file_tag_snapshot::EmbeddedCoverFact {
            source_relative_path: "01.flac".to_string(),
            content_type: bae_util::content_type::ContentType::Jpeg,
            data: bytes.clone(),
        }),
    };
    crate::tables::ImportTables::new(db.clone())
        .apply_file_metadata(
            &candidate.key(),
            &crate::CandidateAsRead {
                content_hash: hash.clone(),
                file_edit_revision: 0,
                metadata_revision: 0,
            },
            &snapshot,
            &draft,
            Some(&crate::cover_art::PreparedCover::Embedded("01.flac".to_string())),
        )
        .await
        .unwrap();

    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    rows(&projection)
        .remove(0)
        .reading
        .album()
        .cloned()
        .expect("the row carries its file-metadata draft");
    // The row names the artwork by the version of the file the snapshot read
    // it from and holds no bytes; the bytes are read apart, by the same name.
    assert_pane_and_row_draw(
        &tables,
        &candidate,
        crate::CoverChoice::Embedded {
            candidate: candidate.key(),
            source_file_id: "01.flac".to_string(),
            source: FileVersion {
                path: root.join("Album").join("01.flac"),
                size: 1_000,
                modified_at_ns: 5,
            },
        },
    )
    .await;
    assert_eq!(
        tables.load_embedded_cover_bytes(&candidate.key(), "01.flac").await.unwrap(),
        Some(bytes),
        "the artwork's bytes are read when a slot draws them"
    );
    assert_eq!(
        tables.load_embedded_cover_bytes(&candidate.key(), "02.flac").await.unwrap(),
        None,
        "a file the snapshot stored no artwork from has none"
    );
}

/// The pane reads the candidate's cover as `expected`, and its list row draws
/// the same image under the same identity.
async fn assert_pane_and_row_draw(
    tables: &crate::tables::ImportTables,
    candidate: &FolderCandidate,
    expected: crate::CoverChoice,
) {
    let detail = tables
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the candidate reads back");
    assert_eq!(detail.draft().unwrap().cover, Some(expected.clone()), "the pane's cover");
    let projection = tables.load_import_list_read(request(TriageTab::Pending).await).await.unwrap();
    assert_eq!(rows(&projection).remove(0).cover, Some(expected.into_image()), "the row's cover");
}

/// A folder image chosen as the cover reads back as that image, at the
/// version the scan read.
#[tokio::test]
async fn a_stored_folder_image_cover_reads_back_as_its_choice() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let mut candidate = candidate(&root, "Album");
    for (name, size) in [("cover.jpg", 500), ("back.jpg", 600)] {
        candidate.files.files.push(CandidateFile {
            file: ScannedFile::new(root.join("Album").join(name), name.to_string(), size, 1),
            role: FileRole::Artwork,
        });
    }
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    crate::tables::import_state::tests::PaneControls::of(&db, &candidate)
        .choose_cover(
            &candidate.files.content_hash(),
            &crate::cover_art::PreparedCover::Local("back.jpg".to_string()),
        )
        .await
        .unwrap();

    assert_pane_and_row_draw(
        &tables,
        &candidate,
        crate::CoverChoice::Local {
            file_id: "back.jpg".to_string(),
            file: FileVersion {
                path: root.join("Album").join("back.jpg"),
                size: 600,
                modified_at_ns: 1,
            },
        },
    )
    .await;
}

/// A catalog's image chosen as the cover reads back with its catalog and
/// every downscaled copy the catalog serves of it.
#[tokio::test]
async fn a_stored_catalog_cover_reads_back_as_its_choice() {
    use bae_catalogs::remote_image::{DownscaledCopy, RemoteImage, RemoteImageSet};
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    let image = RemoteImageSet::with_copies(
        "https://example.invalid/front".to_string(),
        vec![DownscaledCopy {
            url: "https://example.invalid/front-250".to_string(),
            max_edge: 250,
        }],
    );

    crate::tables::import_state::tests::PaneControls::of(&db, &candidate)
        .choose_cover(
            &candidate.files.content_hash(),
            &crate::cover_art::PreparedCover::Remote {
                image: image.clone(),
                source: Catalog::Discogs,
                fetched: RemoteImage {
                    bytes: vec![1, 2, 3],
                    content_type: bae_util::content_type::ContentType::Jpeg,
                },
            },
        )
        .await
        .unwrap();

    assert_pane_and_row_draw(
        &tables,
        &candidate,
        crate::CoverChoice::Remote { image, source: Catalog::Discogs },
    )
    .await;
}

/// No write stores a cover naming a file the scan did not store, and the
/// cover is stored under the candidate's content hash, which fixes its files,
/// so such a cover is a damaged row, planted here as damage: the pane and the
/// row both fail reading it, and neither draws nothing in its place.
#[tokio::test]
async fn a_stored_cover_naming_a_file_the_scan_did_not_store_fails_both_reads() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(&db, &root, "Album").await;
    let hash = candidate.files.content_hash();
    // A typed field stores the draft row the pane reads the cover beside.
    crate::tables::import_state::tests::PaneControls::of(&db, &candidate)
        .type_field(&hash, crate::CandidateEditField::AlbumTitle, "Album")
        .await
        .unwrap();
    tables.damage_cover_file_for_test(&hash, "gone.jpg").await.unwrap();

    let pane = tables
        .load_import_candidate(&candidate.key())
        .await
        .map(|_| ())
        .map_err(|error| error.to_string());
    let row = tables
        .load_import_list_read(request(TriageTab::Pending).await)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string());
    for (read, error) in [("pane", pane), ("row", row)] {
        let error = error.expect_err(read);
        assert!(error.contains("\"gone.jpg\", which nothing here writes"), "{read}: {error}");
    }
}

/// A row linked to a pressing is read from the tables: the list opens none of
/// the folder's files. Its rip log here is a pipe nobody writes, which holds
/// whoever opens it the way a volume that stops answering does, so a list
/// that read the log would never answer.
#[cfg(unix)]
#[tokio::test]
async fn a_linked_row_is_read_without_opening_its_folders_files() {
    use std::os::unix::fs::OpenOptionsExt;

    let (db, tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = folder_below(&root, "Album");
    tmp.create_folder(&folder);
    let log = folder.join("rip.log");
    let path = std::ffi::CString::new(log.to_str().unwrap()).unwrap();
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let candidate = candidate_with(
        &root,
        "Album",
        CategorizedFiles {
            files: vec![
                CandidateFile {
                    file: ScannedFile::new(folder.join("01.flac"), "01.flac".into(), 1_000, 1)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                },
                CandidateFile {
                    file: ScannedFile::new(log.clone(), "rip.log".into(), 1_000, 1),
                    role: FileRole::Document { kind: bae_records::DocumentKind::RipLog },
                },
            ],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    );
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let record = bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-picked");
    tables
        .save_source_release(
            &bae_catalogs::payloads::ReleasePayloads::for_test(
                record.clone(),
                musicbrainz_release("mb-picked", "Picked Album").to_string(),
                Vec::new(),
            )
            .extract()
            .unwrap(),
        )
        .await
        .unwrap();
    crate::tables::ImportTables::new(db.clone())
        .replace_link(
            &candidate.files.content_hash(),
            Some(&crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                record,
                partners: vec![],
            })),
            &[],
        )
        .await
        .unwrap();

    let read = tokio::time::timeout(
        HANG_GUARD,
        tables.load_import_list_read(request(TriageTab::Pending).await),
    )
    .await;
    if read.is_err() {
        // Let the read held on the pipe go: a writer opening and closing it
        // ends it.
        drop(tmp.open_with(
            &log,
            std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK),
        ));
    }
    let projection = read.expect("the list read waited on the folder's rip log").unwrap();
    assert_eq!(
        rows(&projection)[0].release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-picked"),
            partners: vec![],
        },)),
        "the row reads its link from the tables"
    );
}

/// A folder two of whose files show one name — one spelled `Caf\xE9.flac`, one
/// whose name holds the Latin-1 byte — is stored with both, each by the bytes
/// of its own path, and listed under Skipped naming the name they share. A
/// Unix path holds any bytes; Windows can't spell the second name.
#[cfg(unix)]
#[tokio::test]
async fn a_skipped_folder_whose_files_show_one_name_is_listed_with_that_name() {
    use crate::folder_scanner::{InvalidCandidate, InvalidReason, ProposedRole, SkippedFile};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(&root).await.unwrap();
    let folder = PathBuf::from(&root).join("Album");
    let file = |name: &[u8]| SkippedFile {
        relative_path: r"Caf\xE9.flac".to_string(),
        file: FileVersion {
            path: folder.join(std::ffi::OsStr::from_bytes(name)),
            size: 1_000,
            modified_at_ns: 1,
        },
        role: ProposedRole::Audio,
    };
    let invalid = InvalidCandidate {
        path: folder.clone(),
        name: "Album".to_string(),
        watched_folder_path: root.clone(),
        relative_folder: crate::RelativeFolder::test("Album"),
        grouping: None,
        files: vec![file(br"Caf\xE9.flac"), file(b"Caf\xE9.flac")],
        tags: None,
    };
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&root, generation, &ScanItem::Invalid(invalid)).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let list = tables.load_import_list_read(request(TriageTab::Skipped).await).await.unwrap();
    let reasons: Vec<&InvalidReason> = list
        .windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .filter_map(|item| match item {
            ImportListItem::Invalid { row, .. } => Some(&row.reason),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, [&InvalidReason::SameName { name: r"Caf\xE9.flac".to_string() }]);
}

/// A folder of one bound FLAC and a front cover image.
fn with_cover(root: &std::path::Path, name: &str) -> FolderCandidate {
    let folder = folder_below(root, name);
    candidate_with(
        root,
        name,
        CategorizedFiles {
            files: vec![
                CandidateFile {
                    file: ScannedFile::new(folder.join("01.flac"), "01.flac".to_string(), 1_000, 1)
                        .with_test_flac_audio(),
                    role: FileRole::Audio,
                },
                CandidateFile {
                    file: ScannedFile::new(
                        folder.join("cover.jpg"),
                        "cover.jpg".to_string(),
                        500,
                        1,
                    ),
                    role: FileRole::Artwork,
                },
            ],
            parts: Vec::new(),
        },
        ReleaseFileScope::Recursive,
    )
}

/// An import deletes a candidate's draft and cover. When the library deletes
/// the release the import wrote, the folder returns to Found with its files'
/// blank draft and its own cover, on its row and in its pane.
#[tokio::test]
async fn a_folder_whose_release_is_deleted_lists_its_blank_draft_and_folder_cover() {
    let lib = Library::new().await;
    let tables = crate::tables::ImportTables::new(lib.db.clone());
    let candidate = with_cover(&lib.root, "Album");
    let key = candidate.key();
    let hash = candidate.files.content_hash();
    let folder_cover = crate::cover_art::PreparedCover::Local("cover.jpg".to_string());
    assert_eq!(
        crate::cover_art::blank_cover_file(candidate.files.artwork()),
        Some("cover.jpg".to_string())
    );
    lib.run(
        lib.store(ScanItem::Valid(candidate.clone()), None)
            + lib.import_of(
                candidate.clone(),
                "Album",
                "2026-01-02T00:00:00Z",
                "Album Title",
                Vec::new(),
            )
            + lib.release_deleted("Album"),
    )
    .await;

    let list = in_path_order(crate::TriageTab::Pending);
    let listed = tables.read_import_list_for_test(list.clone(), Vec::new()).await.unwrap();
    let row = listed
        .windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .find_map(|item| match item {
            crate::ImportListItem::Candidate { row, .. } => Some(row.clone()),
            _ => None,
        })
        .expect("the folder is listed");
    assert_eq!(row.reading.album(), None, "it shows its folder's name");
    assert_eq!(
        row.cover,
        Some(crate::CoverImageSource::Import(crate::ImportPicture::Folder {
            file: candidate.files.files[1].file.version.clone()
        }))
    );

    let pane = tables.load_import_candidate(&key).await.unwrap().expect("the folder has a pane");
    let draft = pane.draft().expect("a folder nobody imported holds a draft");
    assert_eq!(
        draft.metadata_draft,
        crate::RawReleaseEdit {
            album_title: String::new(),
            album_artist_assignments: Vec::new(),
            album_year: String::new(),
            pressing: crate::RawPressingEdit::from_pressing(&bae_types::pressing::Pressing::blank()),
            tracks: vec![crate::RawTrackEdit {
                id: "candidate-track-0".to_string(),
                title: String::new(),
                artist_assignments: crate::TrackArtistAssignments::AlbumArtists,
                file: Some(crate::AudioFile::Standalone { file_id: "01.flac".to_string() }),
            }],
        }
    );
    let stored = tables.load_import_candidate_states().await.unwrap();
    let state = stored.get(&candidate.files.content_hash()).expect("the folder stores its state");
    assert_eq!(state.metadata_author, crate::MetadataAuthor::Nobody);
    assert_eq!(
        draft.cover.as_ref().map(|cover| cover.selection().clone()),
        Some(folder_cover.selection())
    );

    crate::tables::import_state::tests::PaneControls::of(&lib.db, &candidate)
        .type_field(&hash, crate::CandidateEditField::AlbumTitle, "Typed album")
        .await
        .unwrap();
    let stored = tables.load_candidate_preparation(&hash).await.unwrap().unwrap().stored().unwrap();
    assert_eq!(stored.metadata.draft.album_title, "Typed album");
    assert_eq!(stored.metadata.cover, Some(folder_cover));
    assert_eq!(
        lib.db.count_rows_for_test("import_candidate_cover", "content_hash", &hash).await.unwrap(),
        1,
        "the table holds only the cover row that the deletion wrote"
    );
}
