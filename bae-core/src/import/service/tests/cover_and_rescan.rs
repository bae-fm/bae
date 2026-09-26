fn write_test_jpeg(path: &Path) {
    let image = ::image::RgbImage::from_pixel(1, 1, ::image::Rgb([0, 0, 0]));
    image.save(path).unwrap();
}

#[test]
fn affected_roots_maps_changed_paths_to_their_watched_roots() {
    let root_a = PathBuf::from("/music/new rips");
    let root_b = PathBuf::from("/downloads/bandcamp");
    let roots = vec![root_a.clone(), root_b.clone()];

    // A change inside one root flags only that root.
    let changed = [Path::new("/music/new rips/Album/01.flac")];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a.clone()]);

    // Changes under both roots flag both, in roots order, deduped.
    let changed = [
        Path::new("/downloads/bandcamp/X/cover.jpg"),
        Path::new("/music/new rips/Y"),
        Path::new("/music/new rips/Z"),
    ];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a, root_b]);

    // A change outside every watched root flags nothing.
    let changed = [Path::new("/elsewhere/file")];
    assert!(affected_roots(&changed, &roots).is_empty());
}

#[test]
fn watcher_error_without_a_mapped_path_rescans_every_root() {
    let roots = vec![PathBuf::from("/music/a"), PathBuf::from("/music/b")];
    assert_eq!(roots_for_watch_error(&[], &roots), roots);
    assert_eq!(
        roots_for_watch_error(&[PathBuf::from("/outside")], &roots),
        roots
    );
    assert_eq!(
        roots_for_watch_error(&[PathBuf::from("/music/b/release")], &roots),
        vec![PathBuf::from("/music/b")]
    );
}

#[tokio::test]
async fn explicit_bmp_cover_is_rejected() {
    let test = setup_import_service().await;
    let bmp = test.temp.path().join("cover.bmp");
    let jpg = test.temp.path().join("front.jpg");
    std::fs::write(&bmp, b"bmp bytes").unwrap();
    std::fs::write(&jpg, b"jpg bytes").unwrap();
    let discovered = vec![
        ScannedFile::new(bmp.clone(), "cover.bmp".to_string(), 9, 1),
        ScannedFile::new(jpg.clone(), "front.jpg".to_string(), 9, 1),
    ];

    let error = ImportService::pick_folder_cover(&discovered, "cover.bmp")
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        crate::import::ImportError::LocalCover { .. }
    ));
}

/// A retained reading from a tag reader that accepted BMP artwork.
struct UnsupportedEmbeddedArtworkReader;

impl crate::import::file_tag_snapshot::FileTagReader for UnsupportedEmbeddedArtworkReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        let mut read = crate::import::file_tag_snapshot::LoftyFileTagReader.read(path)?;
        read.embedded_cover = Some((b"BM".to_vec(), crate::util::content_type::ContentType::Bmp));
        Ok(read)
    }
}

#[tokio::test]
async fn retained_unsupported_embedded_cover_is_only_used_when_explicitly_selected() {
    for explicit in [false, true] {
        let test = setup_import_service().await;
        let root = test.temp.path().join("watched");
        let folder = root.join("Album");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("01.flac"), flac()).unwrap();
        write_test_jpeg(&folder.join("front.jpg"));
        let root_text = root.to_string_lossy().into_owned();
        let key = folder.to_string_lossy().into_owned();
        test.service
            .library_manager
            .add_watched_import_folder(&root_text)
            .await
            .unwrap();
        let (scan, _) = test.scan_with(
            Arc::new(UnsupportedEmbeddedArtworkReader),
            Arc::new(crate::import::folder_scanner::OsDirectoryReader),
        );
        scan.rescan(&root).await.unwrap();
        let candidate = test
            .service
            .library_manager
            .load_release_candidate(&key)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let hash = candidate.files.content_hash();
        let revision = prepare_named_candidate(
            &test.service,
            &test.preparations,
            &hash,
            &root_text,
            &key,
            "Cover Album",
        )
        .await;
        let preparation = test
            .service
            .library_manager
            .load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .unwrap();
        let revision = test
            .preparations
            .apply_source(
                &root_text,
                &crate::import::CandidateAsRead {
                    content_hash: hash.clone(),
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: revision,
                },
                &key,
                &crate::import::CandidateMetadataDraft {
                    draft: preparation.draft,
                    source_discogs_artist_ids: preparation.source_discogs_artist_ids,
                    provenance: preparation.metadata_provenance,
                    cover: explicit.then(|| CoverSelection::Embedded("01.flac".to_string())),
                    assets: preparation.assets,
                },
            )
            .await
            .unwrap();
        let snapshot = test
            .service
            .library_manager
            .load_candidate_file_tag_snapshot(&root_text, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot
            .unwrap();
        assert_eq!(
            snapshot.embedded_cover.as_ref().unwrap().content_type,
            crate::util::content_type::ContentType::Bmp
        );
        let mut events = test.service.event_tx.subscribe();
        let result = test
            .service
            .prepare_and_run_folder_import(
                test.service.ids.new_id(),
                key,
                candidate.source(),
                super::ImportExpectation {
                    candidate: crate::import::CandidateAsRead {
                        content_hash: hash,
                        file_edit_revision: candidate.file_edit_revision,
                        metadata_revision: revision,
                    },
                    file_tag_snapshot: Some(snapshot),
                },
                StorageMode::Local,
                false,
            )
            .await;
        if explicit {
            let error = result.unwrap_err();
            assert!(
                matches!(&error, crate::import::ImportError::CoverArt { detail } if detail.contains("Bmp") && detail.contains("not supported")),
                "{error}"
            );
        } else {
            result.expect("automatic artwork should skip unsupported snapshot data");
            let release_id = loop {
                match events.try_recv() {
                    Ok(crate::import::handle::ImportEvent::ImportProgress {
                        progress: ImportProgress::Complete { id, .. },
                        ..
                    }) => break id,
                    Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
                    Err(error) => panic!("import completed without its completion event: {error}"),
                }
            };
            let cover = test
                .service
                .library_manager
                .get_library_image(&release_id, &crate::db::LibraryImageType::Cover)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(cover.source, "local");
            assert_eq!(cover.source_url.as_deref(), Some("release://front.jpg"));
        }
    }
}

#[tokio::test]
async fn explicit_local_cover_missing_from_discovered_images_is_an_error() {
    let test = setup_import_service().await;
    let fallback = test.temp.path().join("front.jpg");
    std::fs::write(&fallback, b"jpg bytes").unwrap();
    let discovered = vec![ScannedFile::new(
        fallback.clone(),
        "front.jpg".to_string(),
        9,
        1,
    )];

    let err = ImportService::pick_folder_cover(&discovered, "cover.bmp")
        .await
        .unwrap_err();

    assert!(
        matches!(&err, crate::import::ImportError::LocalCover { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn explicit_local_cover_with_no_discovered_images_is_an_error() {
    let err = ImportService::pick_folder_cover(&[], "cover.bmp")
        .await
        .unwrap_err();

    assert!(
        matches!(&err, crate::import::ImportError::LocalCover { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn selected_local_cover_path_must_match_discovered_file() {
    let TestService {
        service,
        preparations,
        temp: tmp,
    } = setup_import_service().await;
    // The import under test commits a draft it was handed, not one the folder's
    // tags wrote: the pre-fill would give the candidate a file-metadata draft whose
    // stored reading this import is not carrying.
    service
        .library_manager
        .set_prefill_with_file_metadata(false).await
        .unwrap();
    let folder = tmp.path().join("release");
    std::fs::create_dir(&folder).unwrap();
    write_test_jpeg(&folder.join("front.jpg"));
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("01.flac"),
    )
    .unwrap();
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let expected_content_hash = files.content_hash();
    let watched_folder_path = folder.to_string_lossy().into_owned();
    service
        .library_manager
        .add_watched_import_folder(&watched_folder_path)
        .await
        .unwrap();
    let generation = service
        .library_manager
        .begin_folder_scan(&watched_folder_path)
        .await
        .unwrap();
    service
        .library_manager
        .save_folder_scan_item(
            &watched_folder_path,
            generation,
            &ScanItem::Valid(crate::import::folder_scanner::FolderCandidate {
                path: folder.clone(),
                file_root: folder.clone(),
                name: "Candidate".to_string(),
                files,
                watched_folder_path: watched_folder_path.clone(),
                scope: crate::import::ReleaseFileScope::Recursive,
                file_edit_revision: 0,
                display_path: "Candidate".to_string(),
                grouping: None,
            }),
        )
        .await
        .unwrap()
        .expect("the stored scan generation is current");
    service
        .library_manager
        .finish_folder_scan(&watched_folder_path, generation, None)
        .await
        .unwrap();
    let metadata_revision = prepare_named_candidate(
        &service,
        &preparations,
        &expected_content_hash,
        &watched_folder_path,
        &folder.to_string_lossy(),
        "Candidate",
    )
    .await;
    preparations
        .set_prepared_cover(
            &watched_folder_path,
            &folder.to_string_lossy(),
            &crate::import::CandidateAsRead {
                content_hash: expected_content_hash.clone(),
                file_edit_revision: 0,
                metadata_revision,
            },
            &CoverSelection::Local("cover.bmp".to_string()),
            None,
        )
        .await
        .unwrap();
    let audio_path = folder.join("01.flac");
    let opens_before = crate::audio_codec::probe_opens_for(&audio_path);

    let result = service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            folder.to_string_lossy().into_owned(),
            crate::import::release_candidate::CandidateSource {
                path: folder,
                scope: crate::import::folder_scanner::ReleaseFileScope::Recursive, parts: Vec::new(), 
            },
            super::ImportExpectation {
                candidate: crate::import::CandidateAsRead {
                    content_hash: expected_content_hash,
                    file_edit_revision: 0,
                    metadata_revision: metadata_revision + 1,
                },
                file_tag_snapshot: None,
            },
            StorageMode::Local,
            false,
        )
        .await;

    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::LocalCover { detail } if detail.contains("Selected cover cover.bmp not found")),
        "got: {err}"
    );
    assert_eq!(
        crate::audio_codec::probe_opens_for(&audio_path),
        opens_before,
        "import must reuse the stored scan facts without reopening the audio probe",
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
    let discovered = vec![ScannedFile::new(
        cover.clone(),
        "cover.jpg".to_string(),
        9,
        1,
    )];

    let result = ImportService::pick_folder_cover(&discovered, "cover.jpg").await;

    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o600)).unwrap();
    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::LocalCover { detail } if detail.contains("Failed to read cover art")),
        "got: {err}"
    );
}

async fn rescan_seeded_root(
    test: &TestService,
    root: &Path,
) -> (
    tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
    Result<(), crate::import::ImportError>,
) {
    let service = &test.service;
    service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    let generation = service
        .library_manager
        .begin_folder_scan(&root.to_string_lossy())
        .await
        .unwrap();
    service
        .library_manager
        .save_folder_scan_item(
            &root.to_string_lossy(),
            generation,
            &ScanItem::Invalid(crate::import::InvalidCandidate {
                path: root.join("old-key"),
                name: "Old Candidate".to_string(),
                watched_folder_path: root.to_string_lossy().into_owned(),
                display_path: "old-key".to_string(),
                grouping: None,
                reason: crate::import::InvalidReason::NoValidAudio,
            }),
        )
        .await
        .unwrap()
        .expect("the seeded scan generation is current");

    let (scan, events) = test.scan();
    let result = scan.rescan(root).await;

    (events, result)
}

/// The invalid candidates the stored scan of `root` still holds.
async fn stored_invalid_candidates(service: &ImportService, root: &Path) -> usize {
    service
        .library_manager
        .load_folder_scan_items(&root.to_string_lossy())
        .await
        .unwrap()
        .into_iter()
        .filter(|item| matches!(item, ScanItem::Invalid(_)))
        .count()
}

#[tokio::test]
async fn rescan_missing_root_fails_and_preserves_previous_candidates() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("missing-root");
    let (mut events, result) = rescan_seeded_root(&test, &root).await;
    assert!(result.is_err());

    let failed = loop {
        match events.recv().await.unwrap() {
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status:
                    crate::import::WatchedFolderScanStatus {
                        status: crate::import::FolderScanStatus::Failed { error },
                        ..
                    },
            }) => break error,
            crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key,
            }) => panic!("missing root removed {candidate_key}"),
            _ => {}
        }
    };
    // The reported failure names the root that could not be read. Its reason is
    // the OS's own wording for an absent path ("No such file or directory" on
    // Unix, "The system cannot find the path specified" on Windows), so the
    // root — the part core promises — is what this asserts on.
    assert!(
        failed.contains(&root.to_string_lossy().into_owned()),
        "{failed}"
    );
    assert_eq!(stored_invalid_candidates(&test.service, &root).await, 1);
}

#[tokio::test]
async fn rescan_non_directory_root_keeps_previous_candidates() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("not-a-directory");
    std::fs::write(&root, b"not a directory").unwrap();
    let (mut events, result) = rescan_seeded_root(&test, &root).await;
    assert!(result.is_err(), "a non-directory root must fail its scan");

    loop {
        match events.recv().await.unwrap() {
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status:
                    crate::import::WatchedFolderScanStatus {
                        status: crate::import::FolderScanStatus::Failed { error },
                        ..
                    },
            }) => {
                assert!(
                    error.to_lowercase().contains("not a directory"),
                    "got: {error}"
                );
                break;
            }
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                ..
            }) => {}
            event => panic!("expected scan status, got {event:?}"),
        }
    }
    assert_eq!(stored_invalid_candidates(&test.service, &root).await, 1);
}

#[test]
fn resolve_file_content_type_uses_scan_facts_for_new_audio_formats() {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join("audio-format");
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &fixture_root,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("scan audio-format fixtures");
    for (name, expected) in [
        (
            "placeholder-pcm.wav",
            crate::util::content_type::ContentType::Pcm,
        ),
        (
            "placeholder-pcm.aiff",
            crate::util::content_type::ContentType::Pcm,
        ),
        (
            "placeholder-opus.opus",
            crate::util::content_type::ContentType::Opus,
        ),
        (
            "placeholder-vorbis.ogg",
            crate::util::content_type::ContentType::Vorbis,
        ),
        (
            "placeholder-wavpack.wv",
            crate::util::content_type::ContentType::WavPack,
        ),
        (
            "placeholder-dsd.dsf",
            crate::util::content_type::ContentType::Dsd,
        ),
        (
            "placeholder-dsd.dff",
            crate::util::content_type::ContentType::Dsd,
        ),
    ] {
        let file = files
            .release_files()
            .find(|file| file.relative_path == name)
            .expect("fixture is present in the scan");
        assert_eq!(resolve_file_content_type(file).unwrap(), expected, "{name}");
    }
}

/// Re-reading a folder nothing has touched is a scan that finds what it found
/// last time. It must write nothing and announce nothing: a watched folder is
/// re-read on a timer, and a pass that rewrites and re-announces every row it
/// already holds is work the whole app pays for — a database transaction, a
/// broadcast, and a list rebuilt — once per row, forever, over a folder that
/// did not change.
#[tokio::test]
async fn a_second_pass_over_an_unchanged_folder_announces_nothing() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    for album in ["Artist - One", "Artist - Two"] {
        let album = root.join(album);
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("01.flac"), flac()).unwrap();
        std::fs::write(album.join("02.flac"), flac()).unwrap();
    }
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();

    let (scan, mut events) = test.scan();
    let pass = async || scan.rescan(&root).await.expect("the pass reads the folder");
    pass().await;
    while events.try_recv().is_ok() {}
    pass().await;

    assert_eq!(announced_candidates(&mut events), Vec::<String>::new());

    // And a folder that did change still announces — once, and only itself.
    std::fs::write(root.join("Artist - Two").join("03.flac"), flac()).unwrap();
    pass().await;

    assert_eq!(announced_candidates(&mut events), vec!["Artist - Two"]);
}

/// Discovery seeds the draft from what the folder's own files say, in the
/// same write that stores the candidate: the album, its artist and its tracks
/// are there the first time anyone looks, and the reading they came from is
/// stored beside them.
/// Counts what the pre-fill opens, and answers with what the files really say.
struct CountingTagReader {
    reads: std::sync::atomic::AtomicUsize,
}

impl CountingTagReader {
    fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl crate::import::file_tag_snapshot::FileTagReader for CountingTagReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        self.reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        crate::import::file_tag_snapshot::LoftyFileTagReader.read(path)
    }
}

#[tokio::test]
async fn pre_fill_seeds_the_discovered_candidate_from_its_file_tags() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    let album = root.join("Candidate");
    std::fs::create_dir_all(&album).unwrap();
    for name in TAGGED_FLAC_FIXTURES {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/cue_flac")
                .join(name),
            album.join(name),
        )
        .unwrap();
    }
    let root_text = root.to_string_lossy().into_owned();
    test.service
        .library_manager
        .add_watched_import_folder(&root_text)
        .await
        .unwrap();

    let reader = std::sync::Arc::new(CountingTagReader {
        reads: std::sync::atomic::AtomicUsize::new(0),
    });
    let (scan, mut events) = test.scan_with(
        reader.clone(),
        Arc::new(crate::import::folder_scanner::OsDirectoryReader),
    );
    scan.rescan(&root)
        .await
        .expect("the candidate is read and stored");

    // Two writes store one candidate — it arrives tentative, then valid — and
    // the second finds the draft the first seeded, so the folder's two tracks
    // are opened once between them.
    assert_eq!(reader.reads(), TAGGED_FLAC_FIXTURES.len());

    let key = album.to_string_lossy().into_owned();
    let detail = test
        .service
        .library_manager
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the candidate is stored");
    assert_eq!(
        detail.metadata_provenance,
        Some(crate::import::MetadataProvenance::FileMetadata)
    );
    assert_eq!(detail.metadata_draft.album_title, "Test Album");
    assert_eq!(
        detail
            .metadata_draft
            .tracks
            .iter()
            .map(|track| track.title.clone())
            .collect::<Vec<_>>(),
        vec![
            "Track Two (White Noise)".to_string(),
            "Track Three (Brown Noise)".to_string(),
        ]
    );
    let snapshot = test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_text, &key)
        .await
        .unwrap()
        .expect("the candidate has a snapshot")
        .snapshot
        .expect("the reading the draft was projected from is stored");
    assert_eq!(snapshot.file_edit_revision, 0);

    // A scan of a folder nobody touched re-reads no tags: the candidate holds
    // its draft, and the reading it was projected from is carried forward.
    scan.rescan(&root).await.expect("the folder is read again");
    assert_eq!(reader.reads(), TAGGED_FLAC_FIXTURES.len());
    assert!(test
        .service
        .library_manager
        .load_candidate_file_tag_snapshot(&root_text, &key)
        .await
        .unwrap()
        .expect("the candidate is still stored")
        .snapshot
        .is_some());

    while let Ok(event) = events.try_recv() {
        if let crate::import::handle::ImportEvent::Scan(
            crate::import::ScanEvent::FolderCandidate { candidate, .. },
        ) = event
        {
            assert_eq!(candidate.path, album);
            return;
        }
    }
    panic!("the seeded candidate was not announced");
}

/// With the pre-fill off, discovery reads no tags: the draft is blank, no
/// reading is stored, and the folder's own artwork is still found.
#[tokio::test]
async fn without_pre_fill_the_discovered_candidate_starts_blank() {
    let test = setup_import_service().await;
    test.service
        .library_manager
        .set_prefill_with_file_metadata(false).await
        .unwrap();
    let root = test.temp.path().join("watched");
    let album = root.join("Candidate");
    std::fs::create_dir_all(&album).unwrap();
    for name in TAGGED_FLAC_FIXTURES {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/cue_flac")
                .join(name),
            album.join(name),
        )
        .unwrap();
    }
    write_test_jpeg(&album.join("folder.jpg"));
    write_test_jpeg(&album.join("cover.jpg"));
    let root_text = root.to_string_lossy().into_owned();
    test.service
        .library_manager
        .add_watched_import_folder(&root_text)
        .await
        .unwrap();

    let (scan, _events) = test.scan();
    scan.rescan(&root)
        .await
        .expect("the candidate is read and stored");

    let key = album.to_string_lossy().into_owned();
    let detail = test
        .service
        .library_manager
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the candidate is stored");
    assert_eq!(detail.metadata_provenance, None);
    assert!(detail.metadata_draft.is_blank());
    assert_eq!(
        detail.cover.map(|cover| cover.selection),
        Some(CoverSelection::Local("cover.jpg".to_string()))
    );
    assert!(
        test.service
            .library_manager
            .load_candidate_file_tag_snapshot(&root_text, &key)
            .await
            .unwrap()
            .expect("the candidate stamp is stored")
            .snapshot
            .is_none(),
        "no tags were read, so none are stored"
    );
}

/// A completed pass records every directory it read and when it was last
/// touched, and asked straight afterwards the recorded set says nothing moved.
///
/// That is what a folder on a network volume is re-read on instead of a walk:
/// if the record were missing, incomplete, or read at a precision the
/// filesystem does not keep, every check would claim a change and the walk
/// would happen anyway.
#[tokio::test]
async fn a_pass_records_the_directories_it_read() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    let album = root.join("Artist - Album");
    std::fs::create_dir_all(album.join("Artwork")).unwrap();
    std::fs::write(album.join("01.flac"), flac()).unwrap();
    test.service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();

    let (scan, _events) = test.scan();
    scan.rescan(&root).await.expect("the pass reads the folder");

    let recorded = test
        .service
        .library_manager
        .load_folder_scan_directories(&root.to_string_lossy())
        .await
        .unwrap();
    let mut paths: Vec<&str> = recorded.iter().map(|(path, _)| path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        vec![
            root.to_string_lossy().as_ref(),
            album.to_string_lossy().as_ref(),
            album.join("Artwork").to_string_lossy().as_ref(),
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );
    assert_eq!(super::changed_directories(&recorded), Some(Vec::new()));

    // And a file written into one of them is a change the check reports, in
    // that directory and nowhere else.
    std::fs::write(album.join("02.flac"), flac()).unwrap();
    assert_eq!(super::changed_directories(&recorded), Some(vec![album.clone()]));
}

/// The two fixture tracks that carry real Vorbis comments — an album, its
/// artist, and a title per track.
const TAGGED_FLAC_FIXTURES: [&str; 2] = [
    "02 Test Artist - Track Two (White Noise).flac",
    "03 Test Artist - Track Three (Brown Noise).flac",
];

fn flac() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
    )
    .unwrap()
}

/// The candidates a pass told anyone about, in the order it did.
fn announced_candidates(
    events: &mut tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
) -> Vec<String> {
    let mut announced = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderCandidate {
                candidate,
                ..
            })
            | crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateDiscovered {
                candidate,
                ..
            }) => announced.push(candidate.display_path),
            crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key,
            }) => announced.push(format!("removed {candidate_key}")),
            _ => {}
        }
    }
    announced
}
