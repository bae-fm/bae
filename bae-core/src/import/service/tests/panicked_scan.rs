/// Lists folders as the OS does, except that the first folder under the
/// root a walk asks for panics the first time.
struct ListingThatPanicsOnce {
    panicked: std::sync::atomic::AtomicBool,
}

impl crate::import::folder_scanner::DirectoryReader for ListingThatPanicsOnce {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<
        crate::import::folder_scanner::DirectoryListing,
        crate::import::folder_scanner::FolderScanError,
    > {
        if directory.components().count() == 1
            && !self.panicked.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("the listing broke");
        }
        crate::import::folder_scanner::OsDirectoryReader.read(root, directory, cancellation)
    }

    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<
        Vec<crate::import::folder_scanner::FolderTimes>,
        crate::import::folder_scanner::FolderScanError,
    > {
        crate::import::folder_scanner::OsDirectoryReader.folder_times(root, folders, cancellation)
    }
}

/// Where the stored scan of the only watched root stands.
async fn root_scan_status(service: &ImportService) -> crate::import::FolderScanStatus {
    let mut statuses = service.library_manager.load_folder_scan_progress().await.unwrap().statuses;
    assert_eq!(statuses.len(), 1, "one root is watched");
    statuses.remove(0).status
}

/// A refresh whose pass panics partway through the walk is answered, the
/// root's scan is stored as failed with the panic, the crash reporter hears
/// of it once, and the root is free for the next refresh, which reads it.
#[tokio::test]
async fn a_pass_that_panics_fails_the_scan_and_frees_the_root() {
    let mut test = setup_import_service().await;
    let bugs = Arc::new(crate::diagnostics::RecordingBugReporter::default());
    test.service.diagnostics =
        crate::diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let root = test.temp.path().join("watched");
    for name in ["Release A", "Release B"] {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("track.flac"), flac()).unwrap();
    }
    test.service.library_manager.add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan_with(
        Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader),
        Arc::new(ListingThatPanicsOnce { panicked: std::sync::atomic::AtomicBool::new(false) }),
    );
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (_fs_tx, fs_rx) = tokio::sync::mpsc::unbounded_channel();
    let coordinator = ImportService::start_watcher(command_rx, fs_rx, scan.services.clone());
    let refresh = || {
        let (completion, answered) = tokio::sync::oneshot::channel();
        commands.send(WatcherCommand::Refresh { path: root.clone(), completion }).unwrap();
        answered
    };

    assert_eq!(
        tokio::time::timeout(HANG_GUARD, refresh())
            .await
            .expect("the refresh whose pass panicked is answered")
            .unwrap(),
        Ok(())
    );
    let crate::import::FolderScanStatus::Failed { error } = root_scan_status(&test.service).await
    else {
        panic!("the panicked pass is stored as failed");
    };
    assert!(error.contains("the listing broke"), "{error}");
    assert_eq!(
        bugs.reports(),
        vec![(
            "walk a folder under a watched root".to_string(),
            "walk a folder under a watched root panicked: the listing broke".to_string()
        )]
    );

    assert_eq!(
        tokio::time::timeout(HANG_GUARD, refresh())
            .await
            .expect("the next refresh is answered")
            .unwrap(),
        Ok(())
    );
    assert!(matches!(
        root_scan_status(&test.service).await,
        crate::import::FolderScanStatus::Complete
    ));

    let (completion, stopped) = std::sync::mpsc::channel();
    commands.send(WatcherCommand::Shutdown { completion }).unwrap();
    tokio::task::spawn_blocking(move || {
        stopped.recv().unwrap();
        coordinator.join().unwrap();
    })
    .await
    .unwrap();
}
