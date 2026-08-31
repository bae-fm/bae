/// Create a Remote, pinned, exportable release: a Local release with
/// known-byte source files on disk (coven external refs) and a
/// `source_folder_name`, made Remote with pin so its blobs stay readable from
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
    manager.coven_make_remote(&release.id, true).await.unwrap();
    // The sync loop this fixture's tests connect drains the queue itself, so
    // wait for the make-Remote to finish rather than counting a drain pass this
    // test does not own.
    wait_for_landed_make_remote(manager, &release.id).await;
    release.id
}

/// Wait until a release's make-Remote is fully finished — not just uploaded.
///
/// The drain flips the gate, but coven holds each queue entry until the Store
/// write that publishes the transition activates, a cycle later. A test that
/// asserts "no upload work outstanding" has to be past that, or it reads the
/// transition's own leftovers as new work.
#[cfg(feature = "test-utils")]
async fn wait_for_settled_uploads(manager: &LibraryManager, release_id: &str) {
    for tick in 0..2_000 {
        if tick % 50 == 0 {
            manager.database.sync_now();
        }
        if !manager
            .database
            .has_pending_uploads_for_release(release_id)
            .await
            .unwrap()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("release {release_id} never finished its make-Remote");
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
    release.remote = false;
    release.source_folder_name = Some(folder_name.to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    std::fs::create_dir_all(source_dir).unwrap();
    let created_at = Utc::now();
    let mut inserted_files = Vec::with_capacity(files.len());
    for (index, (name, bytes)) in files.iter().enumerate() {
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
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

/// A Local release with one readable source file, whose stored path fragments are
/// then overwritten with `poison` — the shape a row pulled from another device can
/// have. bae's own row-write refuses these values, but coven applies a pulled
/// changeset straight into SQLite, so the guard that matters is the one at the
/// join: the export copy-out and make-Local both validate the fragment before they
/// join it onto the user's folder.
#[cfg(feature = "test-utils")]
async fn insert_local_export_release_with_poisoned_fragment(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    bytes: &[u8],
    poison: PoisonedFragment<'_>,
) -> String {
    let (release, db_files) = insert_export_release_rows(
        manager,
        source_dir,
        "Album Title",
        &[("track.flac", bytes)],
    )
    .await;
    match poison {
        PoisonedFragment::OriginalFilename(value) => manager
            .database
            .set_original_filename_for_test(&db_files[0].id, value)
            .await
            .unwrap(),
        PoisonedFragment::SourceFolderName(value) => manager
            .database
            .set_source_folder_name_for_test(&release.id, value)
            .await
            .unwrap(),
    }
    release.id
}

#[cfg(feature = "test-utils")]
enum PoisonedFragment<'a> {
    OriginalFilename(&'a str),
    SourceFolderName(&'a str),
}

/// Pausing before the first enqueue parks the worker, so the queue's in-memory
/// state (enqueue, dedup, target_dir, cancel) is observable deterministically
/// without the export path racing the assertions.
#[tokio::test]
async fn output_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    manager.set_outputs_paused(true);

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();
    let snap = manager.output_snapshot();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].title, "Test Album");
    assert_eq!(snap.ops[0].file_count, 1);
    assert_eq!(snap.ops[0].payload.target_dir, target);
    assert_eq!(snap.ops[0].state, crate::library::OutputState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();
    assert_eq!(manager.output_snapshot().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_output(&release_id);
    assert!(manager.output_snapshot().ops.is_empty());
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
    let before = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(before.remote);
    assert!(!manager
        .database
        .has_pending_uploads_for_release(&release_id)
        .await
        .unwrap());

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(&release_id, target.clone())
        .await
        .unwrap();

    // Success removes the entry from the queue.
    let done = wait_for(|| manager.output_snapshot().ops.is_empty()).await;
    assert!(done, "the export should complete and clear the queue");

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
    let after = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(after.remote, "export leaves the release Remote");
    assert!(
        !manager
            .database
            .has_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        "export enqueues no cloud uploads"
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_rejects_parent_component_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PoisonedFragment::OriginalFilename("../escape.flac"),
    )
    .await;

    assert_export_rejects_invalid_path(
        &manager,
        &release_id,
        temp_dir.path(),
        "parent component in original_filename is rejected",
        &["export-out/escape.flac", "export-out/Album Title"],
    )
    .await;
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_rejects_absolute_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let absolute_escape = temp_dir.path().join("absolute-escape.flac");
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PoisonedFragment::OriginalFilename(absolute_escape.to_str().unwrap()),
    )
    .await;

    assert_export_rejects_invalid_path(
        &manager,
        &release_id,
        temp_dir.path(),
        "absolute original_filename is rejected",
        &["absolute-escape.flac", "export-out/Album Title"],
    )
    .await;
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_rejects_parent_component_source_folder_name() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"track-bytes",
        PoisonedFragment::SourceFolderName("../escape-folder"),
    )
    .await;

    assert_export_rejects_invalid_path(
        &manager,
        &release_id,
        temp_dir.path(),
        "parent component in source_folder_name is rejected",
        &["escape-folder", "export-out"],
    )
    .await;
}

/// make-Local hands coven a map of blob id → local destination, built by joining
/// each file's stored `original_filename` onto the folder the user picked. coven
/// writes wherever that map points, so a `../` in a row another device wrote would
/// materialize the release's bytes outside the chosen folder. The join refuses it,
/// and refuses the whole release: no destination in the map may escape the target.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn make_local_dest_rejects_a_traversing_original_filename() {
    let (manager, temp_dir) = setup_test_manager().await;
    let target = temp_dir.path().join("make-local-out");
    let release_id = insert_local_export_release_with_poisoned_fragment(
        &manager,
        &temp_dir.path().join("source"),
        b"escape-bytes",
        PoisonedFragment::OriginalFilename("../escape.flac"),
    )
    .await;

    let error = manager
        .make_local_dest(&release_id, target.to_str().unwrap())
        .await
        .expect_err("a traversing original_filename must not produce a destination");
    assert!(
        error.to_string().contains("invalid path fragment"),
        "unexpected error: {error}",
    );
    assert!(
        !temp_dir.path().join("escape.flac").exists(),
        "nothing is written outside the target folder",
    );
}

#[cfg(feature = "test-utils")]
async fn assert_export_rejects_invalid_path(
    manager: &LibraryManager,
    release_id: &str,
    temp_dir: &std::path::Path,
    message: &str,
    absent_paths: &[&str],
) {
    let target = temp_dir.join("export-out");
    let error = manager
        .export_release(release_id, &target, crate::library::OutputKind::Export)
        .await
        .expect_err(message);

    assert!(
        error.to_string().contains("invalid path fragment"),
        "unexpected error: {error}"
    );
    for path in absent_paths {
        assert!(
            !temp_dir.join(path).exists(),
            "invalid export wrote {}",
            temp_dir.join(path).display()
        );
    }
}

/// A write error (an unwritable target) marks the export `Failed` with a message
/// and keeps it in the queue; `retry_outputs` flips it back to `Queued`.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_write_error_marks_failed_and_retries() {
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

    let failed = wait_for(|| {
        matches!(
            manager.output_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::OutputState::Failed { .. })
        )
    })
    .await;
    assert!(failed, "an unwritable target marks the export Failed");
    assert_eq!(manager.output_snapshot().total.failed, 1);

    // Retry flips it back to Queued (it'll fail again, but stays tracked).
    manager.retry_outputs();
    assert!(manager
        .output_snapshot()
        .ops
        .first()
        .is_some_and(|op| matches!(
            op.state,
            crate::library::OutputState::Queued
                | crate::library::OutputState::Active { .. }
                | crate::library::OutputState::Failed { .. }
        )));

    manager.cancel_output(&release_id);
    let cleared = wait_for(|| manager.output_snapshot().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

/// A failure partway through the export (a read error on a later file, after an
/// earlier file has already been written to staging) leaves NO output at the
/// final `<target>/<source_folder_name>/` path — the staging directory is
/// removed, so the export is all-or-nothing.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_mid_failure_leaves_no_partial_output_at_final_path() {
    let (manager, temp_dir) = setup_test_manager().await;

    // A Local release whose two source files live on disk as coven external refs
    // (UserProvided reads straight from the user's own file). Keeping it Local —
    // never made Remote — means the export reads these files directly, so removing
    // one forces a read error on a later file mid-export.
    let source_dir = temp_dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    release.source_folder_name = Some("Album Title".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let files: &[(&str, &[u8])] = &[("01.flac", b"first-ok"), ("02.flac", b"second-fails")];
    for (name, bytes) in files {
        std::fs::write(source_dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
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
    manager.set_outputs_paused(true);
    let target = temp_dir.path().join("export-out");
    std::fs::create_dir_all(&target).unwrap();
    manager
        .enqueue_export(&release.id, target.clone())
        .await
        .unwrap();
    std::fs::remove_file(source_dir.join("02.flac")).unwrap();
    manager.set_outputs_paused(false);

    let failed = wait_for(|| {
        matches!(
            manager.output_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::OutputState::Failed { .. })
        )
    })
    .await;
    assert!(
        failed,
        "a read error on a later file marks the export Failed"
    );

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

/// Poll `predicate` up to ~2s (40 × 50ms), returning whether it became true.
/// Used by the async download-worker tests instead of a fixed sleep.
async fn wait_for(predicate: impl Fn() -> bool) -> bool {
    for _ in 0..40 {
        if predicate() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    predicate()
}
