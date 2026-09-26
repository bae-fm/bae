fn local_import_command(import_id: &str, candidate_key: &str, folder: &Path) -> ImportCommand {
    ImportCommand {
        import_id: import_id.to_string(),
        candidate_key: candidate_key.to_string(),
        source: crate::import::release_candidate::CandidateSource {
            path: folder.to_path_buf(),
            scope: crate::import::ReleaseFileScope::Recursive,
            parts: Vec::new(),
        },
        selected_cover: None,
        storage_mode: StorageMode::Local,
        pin: false,
        metadata_provenance: None,
        user_edit: None,
    }
}

/// Keeps `path` from opening until dropped, without touching its size or
/// modification time: no permission to read it on Unix, an exclusive handle
/// on Windows.
struct UnopenableFile {
    #[cfg(unix)]
    path: std::path::PathBuf,
    #[cfg(windows)]
    _exclusive: std::fs::File,
}

impl UnopenableFile {
    fn block(path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
            Self {
                path: path.to_path_buf(),
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let exclusive = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap();
            Self {
                _exclusive: exclusive,
            }
        }
    }
}

#[cfg(unix)]
impl Drop for UnopenableFile {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// A source file that cannot be opened mid-import fails the import with the
/// open's own error and leaves nothing of the release behind: no album, no
/// release, and the candidate's failure recorded for its pane. Importing the
/// same candidate again once the file opens lands the whole release.
#[tokio::test]
async fn an_import_that_cannot_open_a_source_writes_nothing_and_a_retry_lands_it() {
    let test = setup_import_service().await;
    test.service
        .library_manager
        .set_prefill_with_file_metadata(false).await
        .unwrap();
    let folder = test.temp.path().join("box-set");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for disc in 1..=2 {
        let disc_dir = folder.join(format!("CD{disc}"));
        std::fs::create_dir_all(&disc_dir).unwrap();
        for track in 1..=3 {
            std::fs::copy(&fixture, disc_dir.join(format!("0{track} Track {track}.flac")))
                .unwrap();
        }
    }
    let (candidate_key, candidate) =
        store_scanned_candidate(&test, &folder, "Box Set").await;
    let expectation = || super::ImportExpectation {
        candidate: candidate.clone(),
        file_tag_snapshot: None,
    };

    // The scan read every file; now one stops opening. Its size and
    // modification time are unchanged, so the import's identity check passes
    // and the failure is the open itself.
    let blocked_name = Path::new("CD2").join("02 Track 2.flac");
    let blocked = UnopenableFile::block(&folder.join(&blocked_name));
    test.service
        .import_cancels
        .register(&candidate_key, "import-blocked");
    test.service
        .do_import(
            local_import_command("import-blocked", &candidate_key, &folder),
            expectation(),
        )
        .await;
    drop(blocked);

    let failure = test
        .service
        .library_manager
        .load_import_candidate_pane_rows(&candidate.content_hash)
        .await
        .unwrap()
        .failure
        .expect("the failed import is recorded on its candidate");
    assert!(
        failure.error.contains(&*blocked_name.to_string_lossy()),
        "the failure names the file that would not open: {}",
        failure.error
    );
    assert!(
        test.service
            .library_manager
            .get_albums(&[])
            .await
            .unwrap()
            .is_empty(),
        "a failed import writes no album"
    );
    assert!(
        test.service
            .library_manager
            .import_replacement_plans_for_content_hash(&candidate.content_hash)
            .await
            .unwrap()
            .is_empty(),
        "a failed import writes no release of its files"
    );

    test.service
        .import_cancels
        .register(&candidate_key, "import-retry");
    test.service
        .prepare_and_run_folder_import(
            "import-retry".to_string(),
            candidate_key.clone(),
            crate::import::release_candidate::CandidateSource {
                path: folder.clone(),
                scope: crate::import::ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            expectation(),
            StorageMode::Local,
            false,
        )
        .await
        .expect("the retried import lands");

    let albums = test.service.library_manager.get_albums(&[]).await.unwrap();
    assert_eq!(albums.len(), 1, "the retry writes the release once");
    let releases = test
        .service
        .library_manager
        .get_releases_for_album(&albums[0].id)
        .await
        .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(
        test.service
            .library_manager
            .get_tracks_for_release(&releases[0].id)
            .await
            .unwrap()
            .len(),
        6
    );
}
