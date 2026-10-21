// Fixtures for the cloud-backed release states the storage, transfer and
// download tests assert over: connecting a manager to an in-memory cloud home,
// putting a release through upload at each stage it can be caught in, and
// reading back what coven kept on this device.
//
// `include!`d into `manager::tests` beside the tests that use them.

/// Connect injected storage and pause uploads while a test seeds its queue.
#[cfg(feature = "test-utils")]
async fn connect_test_cloud(manager: &TestManager) -> Arc<MemoryStorage> {
    connect_test_cloud_with_sync_loop(manager).await;
    manager.database.stop_sync();
    let mut status = manager.database.subscribe_sync_status();
    tokio::time::timeout(std::time::Duration::from_secs(20), status.wait_for(|status| matches!(status, coven::SyncStatus::Stopped))).await.unwrap().unwrap();
    manager.database.unlock_store_key().await.unwrap();
    manager.database.set_uploads_paused(true).await.unwrap();
    manager.sync_status.set_status(status.borrow().clone());
    manager.storage.clone()
}

#[cfg(feature = "test-utils")]
async fn connect_test_cloud_with_sync_loop(manager: &TestManager) -> Arc<MemoryStorage> {
    manager.database.setup_s3_storage(manager.storage.config(), "Test Device", "test-key".into(), coven::SecretText::new("test-secret".into())).await.unwrap();
    let mut status = manager.database.subscribe_sync_status();
    tokio::time::timeout(std::time::Duration::from_secs(20), status.wait_for(|status| matches!(status, coven::SyncStatus::Synced { .. }))).await.unwrap().unwrap();
    manager.sync_status.set_status(status.borrow().clone());
    manager.storage.clone()
}

/// An attached release whose files are queued in coven and have not uploaded.
/// The Uploading filter and outbox snapshot render this committed queue.
/// The manager must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn insert_release_with_queued_uploads(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.upload_release(&release.id, false).await.unwrap();
    assert_eq!(
        manager
            .database
            .queued_upload_count_for_root_for_test("releases", &release.id)
            .await
            .unwrap(),
        files.len(),
        "every file must be queued before the drain runs"
    );
    release
}

/// A release id paired with its album id, for the fixtures that make a release
/// Remote through the real transition (which mints its own album) and then need
/// to name that album.
#[cfg(feature = "test-utils")]
struct ReleaseRef {
    id: String,
    album_id: String,
}

#[cfg(feature = "test-utils")]
impl ReleaseRef {
    async fn of(manager: &LibraryManager, id: String) -> Self {
        let album_id = find_release(manager, &id)
            .await
            .expect("the release exists")
            .album_id;
        Self { id, album_id }
    }
}

/// Wait for the current file's upload to leave coven's durable queue.
#[cfg(feature = "test-utils")]
async fn wait_for_published_file(manager: &LibraryManager, table: &str, row_id: &str) {
    let reference = manager.database.file_ref(table, row_id).await.unwrap();
    let mut uploads = manager.database.subscribe_cloud_outbox();
    manager.database.sync_now();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if !uploads.next().await.unwrap().files.iter().any(|file| file.file == reference) { return; }
        }
    }).await.expect("file uploaded");
}

/// Create a Remote release the real way: a Local release with one source file
/// on disk, attached for upload (`pin` keeps its offline copy), then uploaded.
/// Returns its id. The manager
/// must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn uploaded_release(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    uploaded_release_with_files(
        manager,
        dir,
        album_title,
        &[("track.flac", b"track-bytes")],
        pin,
    )
    .await
}

#[cfg(feature = "test-utils")]
async fn uploaded_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
    pin: bool,
) -> String {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.upload_release(&release.id, pin).await.unwrap();
    if pin {
        wait_for_offline_copy_request(manager, &release.id).await;
    }
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, files.len(), "each release file uploaded");
    release.id
}

/// [`uploaded_release`] for a test connected with
/// [`connect_test_cloud_with_sync_loop`]: the loop drains the queue, so this
/// waits for the upload to finish rather than counting a drain pass this
/// test does not own.
#[cfg(feature = "test-utils")]
async fn upload_release_under_sync_loop(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    let release = insert_local_release_with_files(
        manager,
        dir,
        album_title,
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, pin).await.unwrap();
    if pin {
        wait_for_offline_copy_request(manager, &release.id).await;
    }
    wait_for_landed_upload(manager, &release.id).await;
    release.id
}

#[cfg(feature = "test-utils")]
async fn wait_for_landed_upload(manager: &LibraryManager, release_id: &str) {
    wait_for_settled_uploads(manager, release_id).await;
    assert!(release_files(manager, release_id).await.iter().all(|file| file.stored));
}

#[cfg(feature = "test-utils")]
async fn insert_partially_uploaded_release(
    manager: &LibraryManager,
    temp_dir: &std::path::Path,
) -> DbRelease {
    let source_dir = temp_dir.join(Uuid::new_v4().to_string());
    let release = insert_local_release_with_files(
        manager,
        &source_dir,
        "Partially Uploaded",
        &[("a.flac", b"uploaded"), ("b.flac", b"missing")],
    )
    .await;

    manager.upload_release(&release.id, true).await.unwrap();
    wait_for_offline_copy_request(manager, &release.id).await;
    assert_eq!(queued_upload_count(manager).await, 2);

    std::fs::remove_file(source_dir.join("b.flac")).unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 1);
    // The uploaded file's row names it; the other's names none until its
    // upload completes.
    assert_eq!(
        release_files(manager, &release.id).await.iter().filter(|file| file.stored).count(),
        1
    );
    release
}

#[cfg(feature = "test-utils")]
async fn insert_local_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let mut album = create_test_album();
    album.title = album_title.to_string();
    manager.database.insert_album(&album).await.unwrap();
    insert_local_release_in_album(manager, &album.id, dir, files).await
}

/// A local release with `files` under `dir`, on an album that already exists —
/// what a test needs when one album holds several releases.
async fn insert_local_release_in_album(
    manager: &LibraryManager,
    album_id: &str,
    dir: &std::path::Path,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let release = create_test_release(album_id);
    insert_release(manager, &release).await;
    std::fs::create_dir_all(dir).unwrap();
    let created_at = Utc::now();
    for (index, (name, bytes)) in files.iter().enumerate() {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            bae_test_support::test_uuid(&format!("{}-test-file-{index}", release.id)),
            created_at,
        );
        manager
            .add_external_file_for_test(&file, &path)
            .await
            .unwrap();
    }
    release
}

/// Insert a local release rooted at a nonexistent directory, so no local copy
/// resolves on this device. Seeds a `DbFile` row so the release is otherwise
/// complete.
async fn insert_local_release_without_local_files(
    manager: &LibraryManager,
    album_id: &str,
) -> DbRelease {
    let release = create_test_release(album_id);
    insert_release(manager, &release).await;
    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();
    release
}

/// Read one byte through the production audio reader and return the playback
/// error reason its fill-error handler reports.
async fn playback_error_reason_for_file(
    manager: &LibraryManager,
    file: &DbFile,
) -> crate::ui::PlaybackErrorReason {
    use crate::playback::data_source::{create_audio_reader, FetchArbiter};
    use crate::playback::sparse_buffer::create_sparse_buffer;

    let buffer = create_sparse_buffer(file.file_size as u64);
    let reader = create_audio_reader(manager, &file.id, FetchArbiter::new(), None, false);
    let (error_tx, mut error_rx) = tokio::sync::mpsc::unbounded_channel();
    reader.start_reading(
        buffer.clone(),
        Box::new(move |error| {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a test that stopped listening for the failure needs no report"
            )]
            let _ = error_tx.send(error);
        }),
    );
    // Register demand so the fill fetches; the failed fetch fails the
    // buffer, which unblocks this read with the failure.
    let demand = tokio::task::spawn_blocking(move || {
        let mut r = buffer.new_reader();
        let mut b = [0u8; 1];
        r.read(&mut b)
    });
    let reason = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        error_rx
            .recv()
            .await
            .expect("error channel open")
            .ui_reason()
    })
    .await
    .expect("a playback error must be reported");
    let read = demand.await.expect("demand read task");
    assert!(
        matches!(
            read,
            Err(crate::playback::sparse_buffer::BufferStop::Failed(_))
        ),
        "the waiting read ends on the fetch failure: {read:?}"
    );
    reason
}

/// Store a cover on `release_id` the way the importer does — the `covers` row
/// and its file in one coven batch. Storing different bytes again replaces the
/// cover, as `change_cover` does.
async fn store_test_cover(manager: &LibraryManager, release_id: &str, bytes: &[u8]) {
    let cover = DbLibraryImage::cover(
        release_id,
        "local",
        None,
        bytes,
        manager.clock.now().into(),
    );
    manager
        .store_library_image(&cover, bytes)
        .await
        .unwrap();
}

/// Whether coven keeps a row's file in the pinned cache. Asked of that row's
/// own file rather than through `release_pinned`, which checks the release's
/// files and deliberately excludes covers.
#[cfg(feature = "test-utils")]
async fn file_pinned(manager: &LibraryManager, namespace: &str, row_id: &str) -> bool {
    manager
        .database
        .rows_pinned(namespace, vec![row_id.to_string()])
        .await
        .expect("read the row's pin state")
        .first()
        .copied()
        .flatten()
        .expect("the file-bearing row exists")
}

#[cfg(feature = "test-utils")]
async fn wait_for_offline_copy_request(manager: &LibraryManager, release: &str) {
    let mut values = manager.subscribe_download_queue();
    let queue = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        values.wait_for(|queue| {
            queue.ops.iter().all(|op| {
                op.release_id != release
                    || matches!(op.state, crate::library::DownloadState::Failed { .. })
            })
        }),
    )
    .await
    .expect("offline copy request finishes")
    .unwrap();
    assert!(
        !queue.ops.iter().any(|op| op.release_id == release),
        "offline copy failed: {queue:?}"
    );
}
