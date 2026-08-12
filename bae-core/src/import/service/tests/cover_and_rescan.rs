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

/// `common_ancestor` derives the local-path root by folding over the
/// files' parent dirs. It must compare path components, not string
/// prefixes, so `/m/Album` and `/m/Album2` collapse to `/m` (a string
/// prefix would wrongly keep `/m/Album`), and an ancestor argument returns
/// itself rather than descending.
#[test]
fn common_ancestor_cases() {
    use std::path::Path;
    // Sibling files share their parent.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/01.flac"), Path::new("/m/Album/02.flac")),
        Path::new("/m/Album")
    );
    // `a` is already an ancestor of `b`: keep `a`.
    assert_eq!(
        common_ancestor(Path::new("/m/Album"), Path::new("/m/Album/Disc1/01.flac")),
        Path::new("/m/Album")
    );
    // Component-wise, not string-prefix: Album vs Album2 don't share /m/Album.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/x"), Path::new("/m/Album2/y")),
        Path::new("/m")
    );
    // Disjoint trees collapse to the root.
    assert_eq!(
        common_ancestor(Path::new("/a/b"), Path::new("/c/d")),
        Path::new("/")
    );
}

/// image_cover_priority decides which folder image wins as the cover when
/// the user makes no explicit pick: a name containing "cover" or "front"
/// (case-insensitive, anywhere in the name) ranks first, everything else
/// second. The fallback sort relies on this ordering.
#[test]
fn image_cover_priority_ranks_front_and_cover_first() {
    assert_eq!(ImportService::image_cover_priority("Cover.jpg"), 0);
    assert_eq!(ImportService::image_cover_priority("front.png"), 0);
    assert_eq!(ImportService::image_cover_priority("FRONT.JPG"), 0);
    assert_eq!(
        ImportService::image_cover_priority("album-front-scan.jpg"),
        0
    );
    assert_eq!(ImportService::image_cover_priority("Back.jpg"), 1);
    assert_eq!(ImportService::image_cover_priority("inlay.png"), 1);
    assert_eq!(ImportService::image_cover_priority("disc1.jpg"), 1);
}

#[tokio::test]
async fn explicit_bmp_cover_is_selected() {
    let (service, tmp) = setup_import_service().await;
    let bmp = tmp.path().join("cover.bmp");
    let jpg = tmp.path().join("front.jpg");
    std::fs::write(&bmp, b"bmp bytes").unwrap();
    std::fs::write(&jpg, b"jpg bytes").unwrap();
    let discovered = vec![
        ScannedFile::new(bmp.clone(), "cover.bmp".to_string(), 9),
        ScannedFile::new(jpg, "front.jpg".to_string(), 9),
    ];

    let candidate = service
        .pick_folder_cover(&discovered, Some("cover.bmp"))
        .unwrap()
        .expect("selected cover should be picked");

    assert_eq!(candidate.source, "local");
    assert_eq!(candidate.source_url.as_deref(), Some("release://cover.bmp"));
    assert_eq!(candidate.bytes, b"bmp bytes");
}

#[tokio::test]
async fn explicit_local_cover_missing_from_discovered_images_is_an_error() {
    let (service, tmp) = setup_import_service().await;
    let fallback = tmp.path().join("front.jpg");
    std::fs::write(&fallback, b"jpg bytes").unwrap();
    let discovered = vec![ScannedFile::new(fallback, "front.jpg".to_string(), 9)];

    let err = service
        .pick_folder_cover(&discovered, Some("cover.bmp"))
        .unwrap_err();

    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn explicit_local_cover_with_no_discovered_images_is_an_error() {
    let (service, _tmp) = setup_import_service().await;

    let err = service
        .pick_folder_cover(&[], Some("cover.bmp"))
        .unwrap_err();

    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Selected cover") && detail.contains("not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn selected_local_cover_path_must_match_discovered_file() {
    let (service, tmp) = setup_import_service().await;
    let folder = tmp.path().join("release");
    std::fs::create_dir(&folder).unwrap();
    write_test_jpeg(&folder.join("front.jpg"));
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("01.flac"),
    )
    .unwrap();
    let expected_content_hash =
        crate::import::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            crate::import::ReleaseFileScope::Recursive,
            &crate::import::folder_scanner::StoredCandidateEdits::none(),
        )
        .unwrap()
        .content_hash();

    let result = service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            folder.to_string_lossy().into_owned(),
            folder,
            crate::import::folder_scanner::ReleaseFileScope::Recursive,
            expected_content_hash,
            0,
            Some(CoverSelection::Local("cover.bmp".to_string())),
            StorageMode::Local,
            false,
            crate::import::IdentityChoice::Unknown,
            None,
        )
        .await;

    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Selected cover cover.bmp not found")),
        "got: {err}"
    );
}

#[tokio::test]
async fn failed_import_before_finalize_leaves_only_import_audit_row() {
    let (service, tmp) = setup_import_service().await;
    let folder = tmp.path().join("release");
    std::fs::create_dir(&folder).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("01.flac"),
    )
    .unwrap();

    let import_id = "import-1".to_string();
    let expectation = super::ImportExpectation {
        content_hash: crate::import::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            crate::import::ReleaseFileScope::Recursive,
            &crate::import::folder_scanner::StoredCandidateEdits::none(),
        )
        .unwrap()
        .content_hash(),
        edit_revision: 0,
    };
    service
        .do_import(
            ImportCommand {
                import_id: import_id.clone(),
                candidate_key: folder.to_string_lossy().into_owned(),
                folder,
                scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
                selected_cover: Some(CoverSelection::Remote(
                    "http://127.0.0.1:9/missing.jpg".to_string(),
                    MetadataSource::MusicBrainz,
                )),
                storage_mode: StorageMode::Local,
                pin: false,
                identity_choice: crate::import::IdentityChoice::Unknown,
                user_edit: None,
            },
            expectation,
        )
        .await;

    let (artist_count, artist_image_count) = service
        .library_manager
        .artist_and_image_counts_for_test()
        .await
        .unwrap();

    assert_eq!(artist_count, 0);
    assert_eq!(artist_image_count, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_selected_cover_is_an_error() {
    use std::os::unix::fs::PermissionsExt;

    let (service, tmp) = setup_import_service().await;
    let cover = tmp.path().join("cover.jpg");
    std::fs::write(&cover, b"jpg bytes").unwrap();
    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o000)).unwrap();
    let discovered = vec![ScannedFile::new(cover.clone(), "cover.jpg".to_string(), 9)];

    let result = service.pick_folder_cover(&discovered, Some("cover.jpg"));

    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o600)).unwrap();
    let err = result.unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::CoverArt { detail } if detail.contains("Failed to read cover art")),
        "got: {err}"
    );
}

async fn rescan_seeded_root(
    service: &ImportService,
    root: &Path,
) -> (
    tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
    crate::import::candidate_store::CandidateStore,
    Result<(), crate::import::ImportError>,
) {
    let (event_tx, events) = tokio::sync::broadcast::channel(16);
    let folder_registry = Arc::new(Mutex::new(
        crate::import::folder_registry::ImportFolderRegistry::default(),
    ));
    let candidate_state = crate::import::candidate_store::CandidateStore::default();
    let (fs_tx, _fs_rx) = tokio::sync::mpsc::unbounded_channel();
    let folder_watcher = Arc::new(super::FolderWatcher::new(fs_tx));
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    service
        .library_manager
        .add_watched_import_folder(&root.to_string_lossy())
        .await
        .unwrap();
    folder_registry
        .lock()
        .unwrap()
        .apply_added(root.to_string_lossy().into_owned());
    candidate_state.begin_root_scan(root, 0);
    candidate_state
        .apply_scan_item_if_current(
            root,
            0,
            ScanItem::Invalid(crate::import::InvalidCandidate {
            path: root.join("old-key"),
            name: "Old Candidate".to_string(),
            watched_folder_path: root.to_string_lossy().into_owned(),
            display_path: "old-key".to_string(),
            resolved_boundaries: Vec::new(),
            reason: crate::import::InvalidReason::NoValidAudio,
            }),
            false,
            false,
        )
        .unwrap();

    let result = ImportService::rescan_and_reconcile(
        root,
        &event_tx,
        &service.library_manager,
        &folder_registry,
        &candidate_state,
        &Arc::new(tokio::sync::Mutex::new(())),
        &folder_watcher,
        &cancellation,
    )
    .await;

    (events, candidate_state, result)
}

#[tokio::test]
async fn rescan_missing_root_fails_and_preserves_previous_candidates() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("missing-root");
    let (mut events, candidate_state, result) = rescan_seeded_root(&service, &root).await;
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
    assert_eq!(
        candidate_state.snapshot(vec![crate::import::WatchedFolder::from_path(
                root.to_string_lossy().into_owned(),
            )])
            .invalid_candidates
            .len(),
        1
    );
}

#[tokio::test]
async fn rescan_non_directory_root_keeps_previous_candidates() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("not-a-directory");
    std::fs::write(&root, b"not a directory").unwrap();
    let (mut events, candidate_state, result) = rescan_seeded_root(&service, &root).await;
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
    assert_eq!(
        candidate_state.snapshot(vec![crate::import::WatchedFolder::from_path(
                root.to_string_lossy().into_owned(),
            )])
            .invalid_candidates
            .len(),
        1
    );
}

#[test]
fn resolve_file_content_type_uses_probe_for_new_audio_formats() {
    let fixture = |name: &str| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("audio-format")
            .join(name)
    };
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
        assert_eq!(
            resolve_file_content_type(&fixture(name)).unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn import_trace_line_escapes_json_strings() {
    let line = import_trace_line(
        "2024-01-01T00:00:00+00:00".to_string(),
        "import-1",
        "Album \\ Title\nA",
        "Artist \"Name\"",
        Duration::from_millis(42),
        &[("resolve_metadata", Duration::from_millis(7))],
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&line).expect("trace line must be valid JSON");
    assert_eq!(parsed["album"], "Album \\ Title\nA");
    assert_eq!(parsed["artist"], "Artist \"Name\"");
    assert_eq!(parsed["steps"]["resolve_metadata"], 7);
}
