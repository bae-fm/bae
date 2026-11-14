//! The cover an import picks from its folder's images and its files'
//! embedded artwork.

use super::*;
use crate::cover_art::PreparedCover;
use crate::folder_scanner::ScannedFile;
use crate::import_cancel::ImportStop;
use crate::service::tests::*;
use crate::ImportDestination;
use crate::ImportProgress;
use std::path::Path;
use std::sync::Arc;

#[tokio::test]
async fn explicit_bmp_cover_is_rejected() {
    let test = setup_import_service().await;
    let bmp = test.temp.path().join("cover.bmp");
    let jpg = test.temp.path().join("front.jpg");
    std::fs::write(&bmp, b"bmp bytes").unwrap();
    std::fs::write(&jpg, b"jpg bytes").unwrap();
    let discovered = vec![
        artwork(ScannedFile::new(bmp.clone(), "cover.bmp".to_string(), 9, 1)),
        artwork(ScannedFile::new(jpg.clone(), "front.jpg".to_string(), 9, 1)),
    ];

    let error = folder_cover(
        &discovered,
        "cover.bmp",
        &bae_diagnostics::Diagnostics::noop(),
        bae_util::test_files::lease(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, crate::ImportError::LocalCover { .. }));
}

fn artwork(file: ScannedFile) -> crate::folder_scanner::CandidateFile {
    crate::folder_scanner::CandidateFile { file, role: crate::folder_scanner::FileRole::Artwork }
}

/// A damaged image is never a cover, even under the name a selection gives:
/// the pick names the artwork the folder offers, and this file is not any.
#[tokio::test]
async fn a_damaged_image_is_never_picked_as_the_cover() {
    let test = setup_import_service().await;
    let front = test.temp.path().join("front.png");
    std::fs::write(&front, [0u8; 64]).unwrap();
    let discovered = vec![crate::folder_scanner::CandidateFile {
        file: ScannedFile::new(front, "front.png".to_string(), 64, 1),
        role: crate::folder_scanner::FileRole::DamagedImage,
    }];

    let error = folder_cover(
        &discovered,
        "front.png",
        &bae_diagnostics::Diagnostics::noop(),
        bae_util::test_files::lease(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, crate::ImportError::LocalCover { .. }));
}

/// A retained reading from a tag reader that accepted BMP artwork.
struct UnsupportedEmbeddedArtworkReader;

impl crate::file_tag_reader::FileTagReader for UnsupportedEmbeddedArtworkReader {
    fn read(
        &self,
        path: &Path,
        place: coven::LeasePlace,
    ) -> Result<crate::file_tag_reader::FileTagRead, crate::file_tag_reader::TagReadFailure> {
        let mut read = crate::file_tag_reader::LoftyFileTagReader.read(path, place)?;
        read.embedded_cover = Some((b"BM".to_vec(), bae_util::content_type::ContentType::Bmp));
        Ok(read)
    }
}

#[tokio::test]
async fn retained_unsupported_embedded_cover_is_only_used_when_explicitly_selected() {
    for explicit in [false, true] {
        let test = setup_import_service().await;
        let tables = test.service.tables.clone();
        let root = test.temp.path().join("watched");
        let folder = root.join("Album");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("01.flac"), flac()).unwrap();
        write_test_jpeg(&folder.join("front.jpg"));
        let root_text = root.to_string_lossy().into_owned();
        let key = crate::CandidateKey::folder(&folder);
        tables.add_watched_import_folder(std::path::Path::new(&root_text)).await.unwrap();
        let (scan, _) = test.scan_with(
            Arc::new(UnsupportedEmbeddedArtworkReader),
            Arc::new(crate::folder_scanner::OsDirectoryReader),
        );
        scan.rescan(&root).await.unwrap();
        let candidate = tables.load_release_candidate(&key).await.unwrap().unwrap().unwrap();
        let hash = candidate.files.content_hash();
        let revision = prepare_named_candidate(
            &test.service,
            &test.preparations,
            &hash,
            std::path::Path::new(&root_text),
            &key,
            "Cover Album",
        )
        .await;
        let preparation = crate::preparations::stored_preparation(&test.service.tables, &hash)
            .await
            .unwrap()
            .unwrap();
        let revision = test
            .preparations
            .apply_source(
                std::path::Path::new(&root_text),
                &crate::CandidateAsRead {
                    content_hash: hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: revision,
                },
                &key,
                &crate::CandidateMetadataDraft {
                    draft: preparation.metadata.draft,
                    provenance: preparation.metadata.provenance,
                    cover: explicit.then(|| PreparedCover::Embedded("01.flac".to_string())),
                },
            )
            .await
            .unwrap();
        let snapshot =
            tables.load_candidate_file_tag_snapshot(&key).await.unwrap().unwrap().snapshot.unwrap();
        assert_eq!(
            snapshot.embedded_cover.as_ref().unwrap().content_type,
            bae_util::content_type::ContentType::Bmp
        );
        let mut events = test.service.event_tx.every_event();
        test.service.queue_import_for_test(&key, "import-cover").await;
        let result = test
            .service
            .prepare_and_run_folder_import(
                "import-cover".to_string(),
                key,
                candidate.source(),
                crate::service::ImportExpectation {
                    candidate: crate::CandidateAsRead {
                        content_hash: hash,
                        file_edit_revision: candidate.file_edit_revision,
                        metadata_revision: revision,
                    },
                    file_tag_snapshot: Some(snapshot),
                },
                ImportDestination::ThisDevice,
            )
            .await;
        if explicit {
            let error = result.unwrap_err();
            assert!(
                matches!(&error, ImportStop::Failed(crate::ImportError::CoverArt { detail }) if detail.contains("Bmp") && detail.contains("not supported")),
                "{error:?}"
            );
        } else {
            result.expect("automatic artwork should skip unsupported snapshot data");
            let release_id = loop {
                match events.try_recv() {
                    Ok(crate::ImportEvent::ImportProgress {
                        progress: ImportProgress::Complete { id, .. },
                        ..
                    }) => break id,
                    Ok(_) => {}
                    Err(error) => panic!("import completed without its completion event: {error}"),
                }
            };
            // The embedded artwork is no image the store reads, so a stored
            // cover is the folder's front.jpg.
            bae_records::test_fixtures::test_records(&test.db)
                .find_library_image(&release_id, &bae_records::LibraryImageType::Cover)
                .await
                .unwrap()
                .expect("the folder's cover is stored");
        }
    }
}

#[tokio::test]
async fn explicit_local_cover_missing_from_discovered_images_is_an_error() {
    let test = setup_import_service().await;
    let fallback = test.temp.path().join("front.jpg");
    std::fs::write(&fallback, b"jpg bytes").unwrap();
    let discovered =
        vec![artwork(ScannedFile::new(fallback.clone(), "front.jpg".to_string(), 9, 1))];

    let err = folder_cover(
        &discovered,
        "cover.bmp",
        &bae_diagnostics::Diagnostics::noop(),
        bae_util::test_files::lease(),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(&err, crate::ImportError::LocalCover { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn explicit_local_cover_with_no_discovered_images_is_an_error() {
    let err = folder_cover(
        &[],
        "cover.bmp",
        &bae_diagnostics::Diagnostics::noop(),
        bae_util::test_files::lease(),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(&err, crate::ImportError::LocalCover { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_selected_cover_is_an_error() {
    use std::os::unix::fs::PermissionsExt;

    let test = setup_import_service().await;
    let cover = test.temp.path().join("cover.jpg");
    std::fs::write(&cover, b"jpg bytes").unwrap();
    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o000)).unwrap();
    let discovered = vec![artwork(ScannedFile::new(cover.clone(), "cover.jpg".to_string(), 9, 1))];

    let result = folder_cover(
        &discovered,
        "cover.jpg",
        &bae_diagnostics::Diagnostics::noop(),
        bae_util::test_files::lease(),
    )
    .await;

    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o600)).unwrap();
    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::ImportError::LocalCover { detail } if detail.contains("Failed to read cover art")),
        "got: {err}"
    );
}

fn snapshot_with_cover(
    source_relative_path: Option<&str>,
) -> crate::file_tag_snapshot::FileTagSnapshot {
    crate::file_tag_snapshot::FileTagSnapshot {
        scan_generation: 1,
        file_edit_revision: 0,
        files: Vec::new(),
        embedded_cover: source_relative_path.map(|path| {
            crate::file_tag_snapshot::EmbeddedCoverFact {
                source_relative_path: path.to_string(),
                content_type: bae_util::content_type::ContentType::Jpeg,
                data: vec![1, 2, 3],
            }
        }),
    }
}

#[test]
fn an_embedded_cover_is_the_snapshot_artwork_of_the_audio_it_names() {
    let key = crate::CandidateKey::test("album");
    let selection = PreparedCover::Embedded("01.flac".into());
    let snapshot = snapshot_with_cover(Some("01.flac"));
    let cover = ImportCover::selected(Some(selection.clone()), Some(&snapshot), &key).unwrap();
    assert!(matches!(cover, Some(ImportCover::Embedded(bytes)) if bytes == [1, 2, 3]));

    for snapshot in
        [Some(snapshot_with_cover(Some("02.flac"))), Some(snapshot_with_cover(None)), None]
    {
        let error = ImportCover::selected(Some(selection.clone()), snapshot.as_ref(), &key)
            .err()
            .expect("no other artwork stands in for the named audio's");
        assert!(matches!(error, crate::ImportError::Internal { .. }));
    }
}

/// The cover file of an import opens in a place of the budget: with the only
/// place of a budget of one held, the read waits, and stores the cover once
/// the place goes back.
#[tokio::test(flavor = "multi_thread")]
async fn the_import_cover_read_waits_for_a_place() {
    let temp = tempfile::tempdir().unwrap();
    let budget = coven::FileBudget::new(std::num::NonZeroUsize::MIN);
    let watch = budget.watch();
    let db = bae_store::Store::new_test_within(
        crate::TEST_SCHEMA,
        temp.path(),
        Arc::new(coven::SystemClock),
        coven::task::HeardBugs::shared(),
        budget.clone(),
    )
    .await
    .unwrap();
    let records = bae_records::test_fixtures::test_records(&db);
    let cover = temp.path().join("front.gif");
    std::fs::write(&cover, bae_util::test_cover_art::SOLID_GIF).unwrap();
    let discovered = vec![artwork(ScannedFile::new(
        cover,
        "front.gif".to_string(),
        bae_util::test_cover_art::SOLID_GIF.len() as u64,
        1,
    ))];
    let held = budget.lease(1).await.unwrap();
    let waited = watch.lease_waits_from_now(1);

    let diagnostics = bae_diagnostics::Diagnostics::noop();
    let storing = ImportCover::Local("front.gif".to_string()).stored(
        &discovered,
        "release-1",
        &diagnostics,
        &records,
    );
    tokio::pin!(storing);
    tokio::select! {
        _ = &mut storing => panic!("the cover was read without a place"),
        () = waited => {}
    }
    drop(held);

    let (image, bytes) = storing.await.expect("the cover is stored");
    assert_eq!(image.id, "release-1");
    assert!(!bytes.is_empty());
    assert_eq!(watch.holding(), 0);
    db.close().await.unwrap();
}
