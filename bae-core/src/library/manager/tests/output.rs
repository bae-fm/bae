/// Create a Remote, pinned, exportable release: a Local release with
/// known-byte source files on disk (device-local paths) and a
/// `source_folder_name`, made Remote with pin so its files stay readable from
/// the offline cache. Returns its id. The manager must already be connected via
/// [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn make_exportable_release(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> String {
    let (release, _) =
        insert_export_release_rows(manager, source_dir, folder_name, files).await;
    manager.upload_release(&release.id, true).await.unwrap();
    // The sync loop this fixture's tests connect drains the queue itself, so
    // wait for the upload to finish rather than counting a drain pass this
    // test does not own.
    wait_for_landed_upload(manager, &release.id).await;
    release.id
}

/// Wait until coven no longer lists this release's uploads.
#[cfg(feature = "test-utils")]
async fn wait_for_settled_uploads(manager: &LibraryManager, release_id: &str) {
    for tick in 0..2_000 {
        if tick % 50 == 0 {
            manager.database.sync_now();
        }
        if !has_pending_uploads(manager, release_id).await {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("release {release_id} never finished its upload");
}

#[cfg(feature = "test-utils")]
async fn insert_export_release_rows(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> (DbRelease, Vec<DbFile>) {
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.source_folder_name = Some(folder_name.to_string());
    manager.database.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    std::fs::create_dir_all(source_dir).unwrap();
    let created_at = Utc::now();
    let mut inserted_files = Vec::with_capacity(files.len());
    for (index, (name, bytes)) in files.iter().enumerate() {
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            bae_util::content_type::ContentType::Flac,
            bae_test_support::test_uuid(&format!("{}-export-file-{index}", release.id)),
            created_at,
        );
        let path = source_dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        manager
            .add_external_file_for_test(&file, &path)
            .await
            .unwrap();
        inserted_files.push(file);
    }
    (release, inserted_files)
}

/// A Local release with one readable source file, whose stored name is then
/// overwritten with `name` — the shape a row pulled from another device can
/// have. coven applies a pulled changeset straight into SQLite, so bae's own
/// row-write never sees it: the export copy-out and audio detachment build each
/// path from the names as they read them.
#[cfg(feature = "test-utils")]
async fn insert_local_export_release_named(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    bytes: &[u8],
    name: PeerName<'_>,
) -> String {
    let (release, db_files) = insert_export_release_rows(
        manager,
        source_dir,
        "Album Title",
        &[("track.flac", bytes)],
    )
    .await;
    match name {
        PeerName::OriginalFilename(value) => manager
            .database
            .set_original_filename_for_test(&db_files[0].id, value)
            .await
            .unwrap(),
        PeerName::SourceFolderName(value) => manager
            .database
            .set_source_folder_name_for_test(&release.id, value)
            .await
            .unwrap(),
    }
    release.id
}

#[cfg(feature = "test-utils")]
enum PeerName<'a> {
    OriginalFilename(&'a str),
    SourceFolderName(&'a str),
}

/// Pausing before the first enqueue parks the worker, so the queue's
/// state (enqueue, dedup, target_dir, cancel) is observable deterministically
/// without the export path racing the assertions.
#[tokio::test]
async fn output_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    manager.set_outputs_paused(true).await.unwrap();

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();
    let snap = output_queue_now(&manager).await;
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.rows.len(), 1);
    assert_eq!(snap.rows[0].op.payload.target_dir, target);
    assert_eq!(snap.rows[0].op.state, crate::library::OutputPhase::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();
    assert_eq!(output_queue_now(&manager).await.rows.len(), 1);

    // Cancel drops the entry.
    manager.cancel_output(&release_id).await.unwrap();
    assert!(output_queue_now(&manager).await.rows.is_empty());
}

/// The verbatim copy-out: exported bytes equal the source bytes, laid out at
/// `<target>/<source_folder_name>/<original_filename>` (including nested
/// subfolders), and the export changes no release state — it stays Remote with
/// no new cloud-outbox rows.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_writes_exact_bytes_in_source_folder_and_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    let source_dir = temp_dir.path().join("source");
    let files: &[(&str, &[u8])] = &[
        ("cover.jpg", b"cover-bytes-abc"),
        ("CD1/track.flac", b"flac-bytes-0123456789"),
    ];
    let release_id =
        make_exportable_release(&manager, &source_dir, "Album Title (2020)", files).await;

    // Precondition: Remote, no pending uploads after the drain.
    assert!(release_files(&manager, &release_id).await.iter().all(|file| file.stored));
    assert!(!has_pending_uploads(&manager, &release_id).await);

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();

    // Success removes the entry from the queue.
    output_queue_until(&manager, |queue| queue.rows.is_empty()).await;

    // Byte-accuracy + folder layout.
    for (name, bytes) in files {
        let written = target.join("Album Title (2020)").join(name);
        let got = std::fs::read(&written).unwrap_or_else(|e| panic!("read exported {name}: {e}"));
        assert_eq!(&got, bytes, "exported bytes for {name} match the source");
    }

    // The staging directory was renamed into place, leaving nothing behind it.
    let leftover = std::fs::read_dir(&target)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        leftover,
        vec!["Album Title (2020)".to_string()],
        "only the final export folder remains under the target; no staging dir"
    );

    // Export changed no release state: still Remote, no new outbox rows.
    assert!(release_files(&manager, &release_id).await.iter().all(|file| file.stored));
    assert!(
        !has_pending_uploads(&manager, &release_id).await,
        "export enqueues no cloud uploads"
    );
}

/// A file named `..` is an ordinary name: coven writes it as `.．`, so the
/// export writes the file inside its folder and nothing outside the target.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_writes_a_parent_name_inside_its_folder() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_named(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PeerName::OriginalFilename("../escape.flac"),
    )
    .await;

    let target = temp_dir.path().join("export-out");
    manager
        .export_release(&release_id, &target, crate::library::OutputKind::Export)
        .await
        .expect("a parent name exports");

    assert_eq!(
        std::fs::read(target.join("Album Title").join(".．").join("escape.flac")).unwrap(),
        b"escape-bytes"
    );
    assert!(!target.join("escape.flac").exists());
    assert!(!temp_dir.path().join("escape.flac").exists());
}

/// An absolute path's leading `/` gives an empty name, which names no file:
/// the export writes nothing.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_refuses_an_absolute_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_named(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PeerName::OriginalFilename("/absolute-escape.flac"),
    )
    .await;

    let target = temp_dir.path().join("export-out");
    let error = manager
        .export_release(&release_id, &target, crate::library::OutputKind::Export)
        .await
        .expect_err("an absolute original_filename is refused");

    assert!(
        error.to_string().contains("names no file"),
        "unexpected error: {error}"
    );
    assert!(!target.join("Album Title").exists());
}

/// A folder name is one name, `/` included: coven writes `../escape-folder`
/// as `..／escape-folder`, one folder inside the target.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_writes_a_folder_name_holding_a_slash_as_one_folder() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_named(
        &manager,
        &temp_dir.path().join("source"),
        b"track-bytes",
        PeerName::SourceFolderName("../escape-folder"),
    )
    .await;

    let target = temp_dir.path().join("export-out");
    manager
        .export_release(&release_id, &target, crate::library::OutputKind::Export)
        .await
        .expect("a folder name holding a slash exports");

    assert_eq!(
        std::fs::read(target.join("..／escape-folder").join("track.flac")).unwrap(),
        b"track-bytes"
    );
    assert!(!temp_dir.path().join("escape-folder").exists());
}

/// A write error (an unwritable target) marks the export `Failed` with a message
/// and keeps it in the queue until it is cancelled.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_write_error_marks_failed_until_cancelled() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    let source_dir = temp_dir.path().join("source");
    let release_id = make_exportable_release(
        &manager,
        &source_dir,
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    // Target a path that is actually a file, so creating the release subfolder
    // under it fails with an I/O error (the read succeeds; the write doesn't).
    let blocker = temp_dir.path().join("blocker");
    std::fs::write(&blocker, b"a file, not a directory").unwrap();

    manager
        .enqueue_export(&release_id, blocker.clone())
        .await
        .unwrap();

    // An unwritable target marks the export Failed.
    let failed = output_queue_until(&manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(crate::library::OutputPhase::Failed { .. })
        )
    })
    .await;
    assert_eq!(failed.total.failed, 1);

    manager.cancel_output(&release_id).await.unwrap();
    assert!(
        output_queue_now(&manager).await.rows.is_empty(),
        "cancel removes the entry"
    );
}

/// A failure partway through the export (a read error on a later file, after an
/// earlier file has already been written to staging) leaves NO output at the
/// final `<target>/<source_folder_name>/` path — the staging directory is
/// removed, so the export is all-or-nothing.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_mid_failure_leaves_no_partial_output_at_final_path() {
    let (manager, temp_dir) = setup_test_manager().await;

    // A Local release whose two source files live on disk as device-local paths
    // (UserProvided reads straight from the user's own file). Keeping it Local —
    // never made Remote — means the export reads these files directly, so removing
    // one forces a read error on a later file mid-export.
    let source_dir = temp_dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.source_folder_name = Some("Album Title".to_string());
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;

    let files: &[(&str, &[u8])] = &[("01.flac", b"first-ok"), ("02.flac", b"second-fails")];
    for (name, bytes) in files {
        std::fs::write(source_dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            bae_util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            Utc::now(),
        );
        manager
            .add_external_file_for_test(&file, &source_dir.join(name))
            .await
            .unwrap();
    }

    // Pause so the source file can be deleted before the worker runs, making the
    // later read fail deterministically rather than racing the copy.
    manager.set_outputs_paused(true).await.unwrap();
    let target = temp_dir.path().join("export-out");
    std::fs::create_dir_all(&target).unwrap();
    manager
        .enqueue_export(&release.id, target.clone())
        .await
        .unwrap();
    std::fs::remove_file(source_dir.join("02.flac")).unwrap();
    manager.set_outputs_paused(false).await.unwrap();

    // A read error on a later file marks the export Failed.
    output_queue_until(&manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(crate::library::OutputPhase::Failed { .. })
        )
    })
    .await;

    // The all-or-nothing guarantee: nothing at the final path, and the staging
    // directory was removed — the target holds no export output at all.
    assert!(
        !target.join("Album Title").exists(),
        "no partial output at the final export path"
    );
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "staging directory cleaned up; target left empty"
    );
}
