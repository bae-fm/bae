// Fixtures for the cloud-backed release states the storage, transfer and
// download tests assert over: connecting a manager to an in-memory cloud home,
// putting a release through make-Remote at each stage it can be caught in, and
// reading back what coven kept on this device.
//
// `include!`d into `manager::tests` beside the tests that use them.

/// Connect a real `SyncManager` over an in-memory cloud home (opaque,
/// encrypted) so the manager's cloud read/write/transition paths run against
/// it — the in-module counterpart of the integration tests' `setup_with_cloud`.
/// After this, `has_cloud_home()` holds.
///
/// No sync loop runs behind it: the tests here drive the upload queue with
/// `drain_uploads_expecting_work` and assert what that pass moved, which is only
/// a fact if nothing else drains. A test that needs the loop's own work — the
/// Store write that publishes a transition, or `is_sync_ready()` — takes
/// [`connect_test_cloud_with_sync_loop`] instead.
#[cfg(feature = "test-utils")]
async fn connect_test_cloud(manager: &LibraryManager) -> Arc<InMemoryCloudHome> {
    let home = Arc::new(InMemoryCloudHome::new());
    manager
        .connect_test_cloud_home_caller_driven(
            home.clone(),
            CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
        )
        .await
        .expect("connect in-memory cloud home");
    home
}

/// [`connect_test_cloud`] with the production sync loop running behind it, for
/// the tests that wait on a cycle to publish a transition's Store write. Their
/// drains are the loop's as well as their own, so they assert on published
/// state rather than on a drain's count.
#[cfg(feature = "test-utils")]
async fn connect_test_cloud_with_sync_loop(manager: &LibraryManager) -> Arc<InMemoryCloudHome> {
    let home = Arc::new(InMemoryCloudHome::new());
    manager
        .connect_test_cloud_home(
            home.clone(),
            CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
        )
        .await
        .expect("connect in-memory cloud home");
    home
}

/// A release mid-make-Remote: its uploads are enqueued in coven's durable queue
/// but not drained, so the gate has not flipped and the release still reads
/// Local. This is the state the Uploading filter and the outbox snapshot render.
/// The manager must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn insert_release_with_queued_uploads(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.coven_make_remote(&release.id, false).await.unwrap();
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
        let album_id = manager
            .database
            .find_release_by_id(&id)
            .await
            .unwrap()
            .expect("the release exists")
            .album_id;
        Self { id, album_id }
    }
}

/// Wait until a host-provided blob on a Remote release has a committed cloud
/// object.
///
/// Publication is the sync loop's Store write, not the row write: storing a
/// cover leaves its blob `PendingRemote` with no locator, and only the next
/// cycle gives it one — which is what a cloud tombstone needs to name. A test
/// that asserts on the tombstone has to be past that point.
#[cfg(feature = "test-utils")]
async fn wait_for_published_blob(manager: &LibraryManager, namespace: &str, row_id: &str) {
    for tick in 0..2_000 {
        // Re-kick periodically: a cycle already in flight ignores the nudge, and
        // the write only activates on a cycle that starts after it was queued.
        if tick % 50 == 0 {
            manager.database.sync_now();
        }
        let blob = manager
            .database
            .row_blob_ref(namespace, row_id)
            .await
            .expect("the blob-bearing row exists");
        if blob.stored().is_some() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let (pending, blocked) = manager
        .database
        .pending_and_blocked_writes_for_test()
        .await
        .unwrap();
    panic!(
        "blob {namespace}/{row_id} never reached the cloud; pending={pending} blocked={blocked}"
    );
}

/// Create a Remote release the real way: a Local release with one source file
/// on disk (coven's external ref), made Remote (`pin` keeps its blob in the
/// offline cache) and drained so the gate flips. Returns its id. The manager
/// must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn make_remote_release(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    make_remote_release_with_files(
        manager,
        dir,
        album_title,
        &[("track.flac", b"track-bytes")],
        pin,
    )
    .await
}

#[cfg(feature = "test-utils")]
async fn make_remote_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
    pin: bool,
) -> String {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.coven_make_remote(&release.id, pin).await.unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, files.len(), "each release blob uploaded");
    release.id
}

/// [`make_remote_release`] for a test connected with
/// [`connect_test_cloud_with_sync_loop`]: the loop drains the queue, so this
/// waits for the make-Remote to finish rather than counting a drain pass this
/// test does not own.
#[cfg(feature = "test-utils")]
async fn make_remote_release_under_sync_loop(
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
    manager.coven_make_remote(&release.id, pin).await.unwrap();
    wait_for_landed_make_remote(manager, &release.id).await;
    release.id
}

/// Wait for a release's make-Remote to finish under a running sync loop, and
/// assert it landed: no upload work outstanding and the gate flipped.
#[cfg(feature = "test-utils")]
async fn wait_for_landed_make_remote(manager: &LibraryManager, release_id: &str) {
    wait_for_settled_uploads(manager, release_id).await;
    assert!(
        manager
            .database
            .find_release_by_id(release_id)
            .await
            .unwrap()
            .unwrap()
            .remote,
        "every upload landed, so the release is Remote"
    );
}

#[cfg(feature = "test-utils")]
async fn insert_partially_uploaded_make_remote_release(
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

    manager.coven_make_remote(&release.id, true).await.unwrap();
    assert!(manager
        .database
        .make_remote_progress_for_release(&release.id)
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        manager
            .database
            .queued_upload_count_for_test()
            .await
            .unwrap(),
        2
    );

    std::fs::remove_file(source_dir.join("b.flac")).unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 1);
    assert!(
        !manager
            .database
            .find_release_by_id(&release.id)
            .await
            .unwrap()
            .unwrap()
            .remote,
        "the release must still be Local while one upload is unresolved"
    );
    assert_eq!(
        manager
            .database
            .queued_delete_count_for_test()
            .await
            .unwrap(),
        0
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
    let mut release = create_test_release(album_id);
    release.remote = false;
    manager.database.insert_release(&release).await.unwrap();
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
    let mut release = create_test_release(album_id);
    release.remote = false;
    manager.database.insert_release(&release).await.unwrap();
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
            let _ = error_tx.send(error);
        }),
    );
    // Register demand so the fill fetches; the failed fetch cancels the
    // buffer, which unblocks this read with `None`.
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
            .into_ui_reason()
    })
    .await
    .expect("a playback error must be reported");
    demand.await.expect("demand read task");
    reason
}

/// Store a cover on `release_id` the way the importer does — the `covers` row
/// and its blob in one coven batch — so the release carries the cover blob that
/// coven's `make_remote` pins along with its files.
#[cfg(feature = "test-utils")]
async fn store_test_cover(manager: &LibraryManager, release_id: &str, bytes: &[u8]) {
    let cover = DbLibraryImage::cover(
        release_id,
        &Uuid::new_v4().to_string(),
        "local",
        None,
        bytes,
        manager.clock.now(),
    );
    manager
        .store_library_image_blob(&cover, bytes)
        .await
        .unwrap();
}

/// Whether coven keeps a row's blob in `storage/pinned/`. Asked of that row's
/// own blob rather than through `release_pinned`, which answers from a
/// representative *file* and so cannot see a stranded cover.
#[cfg(feature = "test-utils")]
async fn blob_pinned(manager: &LibraryManager, namespace: &str, row_id: &str) -> bool {
    manager
        .database
        .rows_pinned(namespace, vec![row_id.to_string()])
        .await
        .expect("read the row's pin state")
        .first()
        .copied()
        .flatten()
        .expect("the blob-bearing row exists")
}
