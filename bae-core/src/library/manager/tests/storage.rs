/// Insert N albums each with one release; return `(albums, releases)`.
/// Each album's title is `"Album {i}"` so ordering is deterministic.
async fn seed_albums(manager: &LibraryManager, count: usize) -> (Vec<DbAlbum>, Vec<DbRelease>) {
    let mut albums = Vec::new();
    let mut releases = Vec::new();
    for i in 0..count {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        albums.push(album);
        releases.push(release);
    }
    (albums, releases)
}

fn sort_by_album_title_asc() -> crate::db::StorageSortCriterion {
    crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Ascending,
    }
}

#[tokio::test]
async fn storage_page_returns_all_rows_for_all_filter() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
    assert_eq!(page.total_count, 3);
}

#[tokio::test]
async fn storage_page_paginates() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 5).await;

    let page1 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            2,
        )
        .await
        .unwrap();
    let page2 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            2,
            2,
        )
        .await
        .unwrap();
    let page3 = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            4,
            2,
        )
        .await
        .unwrap();

    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page2.rows.len(), 2);
    assert_eq!(page3.rows.len(), 1);
    // total_count is the full filtered universe, not the page.
    assert_eq!(page1.total_count, 5);
    assert_eq!(page2.total_count, 5);
    assert_eq!(page3.total_count, 5);
}

#[tokio::test]
async fn storage_page_sorts_album_title_ascending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 0", "Album 1", "Album 2"]);
}

#[tokio::test]
async fn storage_page_sorts_album_title_descending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let sort = crate::db::StorageSortCriterion {
        field: crate::db::StorageSortField::AlbumTitle,
        direction: crate::db::SortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&sort, crate::db::StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 2", "Album 1", "Album 0"]);
}

/// Each storage-page row carries the state-appropriate `storage_actions`
/// the Storage Manager row context menu renders — pinned offers unpin +
/// make-Local, cloud-only offers pin + make-Local, local offers make-Remote.
/// With a cloud home present every remote/local transition is open.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_rows_carry_state_appropriate_actions() {
    use crate::album_detail::ReleaseStorageAction::{MakeLocal, MakeRemote, Pin, Unpin};

    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_with_sync_loop(&manager).await;

    // Pinned: made Remote with pin, so its blob lands in coven's offline cache.
    let pinned = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("pinned"),
        "Pinned Album",
        true,
    )
    .await;
    // Cloud-only: made Remote without pin, so its blob is evictable, not pinned.
    let cloud_only = make_remote_release_under_sync_loop(
        &manager,
        &temp_dir.path().join("cloud"),
        "Cloud Album",
        false,
    )
    .await;

    // Local: not remote, files at a local path.
    let mut local_album = create_test_album();
    local_album.title = "Local Album".to_string();
    let mut local = create_test_release(&local_album.id);
    local.remote = false;
    manager.database.insert_album(&local_album).await.unwrap();
    manager.database.insert_release(&local).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&local.id, "/tmp/local")
        .await
        .unwrap();

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let actions: std::collections::HashMap<_, _> = page
        .rows
        .iter()
        .map(|r| (r.release.id.clone(), r.release.storage_actions.clone()))
        .collect();

    assert_eq!(actions[&pinned], vec![Unpin, MakeLocal]);
    assert_eq!(actions[&cloud_only], vec![Pin, MakeLocal]);
    assert_eq!(actions[&local.id], vec![MakeRemote]);
}

/// With no cloud home, no remote storage exists, so the rows offer no
/// transitions — the context menu is empty everywhere.
#[tokio::test]
async fn storage_page_rows_have_no_actions_without_cloud_home() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, "/tmp/local")
        .await
        .unwrap();

    let page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].release.storage_actions.is_empty());
}

#[tokio::test]
async fn storage_page_local_filter_matches_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album_remote = create_test_album();
    let mut album_local = create_test_album();
    album_local.title = "Local Album".to_string();
    let remote_release = create_test_release(&album_remote.id);
    let mut local_release = create_test_release(&album_local.id);
    local_release.remote = false;

    manager.database.insert_album(&album_remote).await.unwrap();
    manager.database.insert_album(&album_local).await.unwrap();
    manager
        .database
        .insert_release(&remote_release)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&local_release)
        .await
        .unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&local_release.id, "/tmp/local")
        .await
        .unwrap();

    let local = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Local,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(local.rows.len(), 1);
    assert_eq!(local.total_count, 1);
    assert_eq!(local.rows[0].release.id, local_release.id);
    assert_eq!(
        local.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    let remote = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Remote,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(remote.rows.len(), 1);
    assert_eq!(remote.rows[0].release.id, remote_release.id);
    assert_ne!(
        remote.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );
}

#[tokio::test]
async fn storage_count_matches_filtered_page_total() {
    let (manager, _temp_dir) = setup_test_manager().await;
    // Three albums, one release each. Mark the second release
    // local at insert time so filters produce distinct counts.
    let mut inserted_local = None;
    for i in 0..3 {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let mut release = create_test_release(&album.id);
        let local = i == 1;
        if local {
            release.remote = false;
            inserted_local = Some(release.id.clone());
        }
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        if local {
            manager
                .database
                .register_release_external_refs_for_test(&release.id, "/tmp/local")
                .await
                .unwrap();
        }
    }

    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::All)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Local)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        manager
            .get_storage_count(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        0
    );

    let all_page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(all_page.total_count, 3);

    let local_page = manager
        .get_storage_page(
            &sort_by_album_title_asc(),
            crate::db::StorageFilter::Local,
            0,
            10,
        )
        .await
        .unwrap();
    assert_eq!(local_page.rows.len(), 1);
    assert_eq!(local_page.rows[0].release.id, inserted_local.unwrap());
}

/// `get_storage_total_size` sums `total_size` over every storage row matching
/// `filter` — the same universe `get_storage_page` pages over — independent of
/// how many pages have loaded. For each filter, the aggregate must equal the
/// sum of `total_size` over that filter's full (unpaginated) storage page.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_total_size_matches_page_total_size_sum() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    // Three releases: one local, one remote-and-quiet, and one mid-make-Remote
    // — its uploads are queued but undrained, so its gate has not flipped and it
    // is still Local as well as Uploading.
    let album_local = create_test_album();
    let album_remote = create_test_album();
    let mut release_local = create_test_release(&album_local.id);
    release_local.remote = false;
    let release_remote = create_test_release(&album_remote.id);

    manager.database.insert_album(&album_local).await.unwrap();
    manager.database.insert_album(&album_remote).await.unwrap();
    manager
        .database
        .insert_release(&release_local)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_remote)
        .await
        .unwrap();

    for (release_id, file_size) in [(&release_local.id, 1_000i64), (&release_remote.id, 100)] {
        let file = DbFile {
            id: bae_test_support::test_uuid(&format!("{release_id}-file")),
            release_id: release_id.clone(),
            original_filename: "a.flac".to_string(),
            file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(b"fixture"),
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    // The uploading release goes through the real transition, so its 10_000
    // bytes are what coven actually has queued.
    insert_release_with_queued_uploads(
        &manager,
        &temp_dir.path().join("uploading"),
        "Uploading Album",
        &[("a.flac", &vec![0u8; 10_000])],
    )
    .await;

    for filter in [
        crate::db::StorageFilter::All,
        crate::db::StorageFilter::Local,
        crate::db::StorageFilter::Remote,
        crate::db::StorageFilter::Uploading,
    ] {
        let page = manager
            .get_storage_page(&sort_by_album_title_asc(), filter, 0, 10)
            .await
            .unwrap();
        let page_sum: i64 = page.rows.iter().map(|row| row.release.total_size).sum();

        let aggregate = manager.get_storage_total_size(filter).await.unwrap();
        assert_eq!(
            aggregate, page_sum as u64,
            "{filter:?}: aggregate must equal the page's own total_size sum"
        );
    }

    // Concrete expectations, so a bug that moves the *same* wrong figure on
    // both sides doesn't slip through. The uploading release counts as Local
    // too — its gate flips only once every upload lands.
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::All)
            .await
            .unwrap(),
        11_100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Local)
            .await
            .unwrap(),
        11_000
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Remote)
            .await
            .unwrap(),
        100
    );
    assert_eq!(
        manager
            .get_storage_total_size(crate::db::StorageFilter::Uploading)
            .await
            .unwrap(),
        10_000
    );
}

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
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let created_at = Utc::now();
    for (index, (name, bytes)) in files.iter().enumerate() {
        std::fs::write(dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            bae_test_support::test_uuid(&format!("{}-test-file-{index}", release.id)),
            created_at,
            crate::util::fs::hash_bytes(bytes),
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &dir.to_string_lossy())
        .await
        .unwrap();
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
    manager
        .database
        .register_release_external_refs_for_test(
            &release.id,
            &format!("/nonexistent/origin-device/{}", Uuid::new_v4()),
        )
        .await
        .unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
        // No real file backs this fixture (the whole point is "no local copy
        // resolves"), so there is no plaintext to hash.
        crate::util::fs::hash_bytes(b"fixture"),
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
