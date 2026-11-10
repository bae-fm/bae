//! The fixtures the manager's tests share (a test library with its catalog
//! fakes and memory storage, the rows they seed, and the cloud they connect),
//! and the library errors an import or a storage setup becomes.

use super::*;
use crate::test_libraries::TestLibrary;
use crate::LibraryError;
use bae_diagnostics::UiErrorCategory;
use bae_import::ImportError;
use bae_records::{DbAlbum, DbFile, DbLibraryImage, DbRelease, LibraryImageType, NewReleaseFile};
use bae_types::release_storage::ReleaseStorageAction;
use bae_util::content_type::ContentType;
use chrono::Utc;
use coven::{MemoryStorage, Storage};
use tempfile::TempDir;

// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
pub(super) const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882"; // was "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
pub(super) const TRACK_1: &str = "f2f77437-aa03-4583-8b1c-d12bcf984967"; // was "track-1"
pub(super) const TRACK_2: &str = "9ebc44e7-5513-4550-8d29-5d8919ec917b"; // was "track-2"
pub(super) const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc"; // was "track-a"

/// The library's views over `library`'s store, which the tests read its
/// state back through.
pub(super) fn views(library: &LibraryManager) -> bae_views::LibraryViews {
    bae_views::LibraryViews::new(library.store.clone())
}

pub(super) struct TestManager {
    pub(super) library: TestLibrary,
    /// The catalog fakes the library asks, which a test seeds.
    pub(super) fakes: bae_catalogs::fake_catalogs::FakeCatalogs,
    pub(super) storage: Arc<MemoryStorage>,
    pub(super) installation: coven::Coven,
}

pub(super) async fn setup_test_manager() -> (TestManager, TempDir) {
    setup_test_manager_with_clock(Arc::new(coven::SystemClock)).await
}

pub(super) async fn setup_test_manager_with_clock(
    clock: coven::ClockRef,
) -> (TestManager, TempDir) {
    setup_test_manager_with(clock, bae_diagnostics::Diagnostics::noop()).await
}

pub(super) async fn setup_test_manager_with(
    clock: coven::ClockRef,
    diagnostics: bae_diagnostics::Diagnostics,
) -> (TestManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let app_dir = bae_app_dir::AppDir::at(temp_dir.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = Arc::new(MemoryStorage::builder().clock(clock.clone()).build().unwrap());
    let sources = bae_store::TestSources::on(&clock);
    let handle = bae_store::configure_store(sources.builder(&installation), crate::SCHEMA)
        .storage_connector(storage.clone())
        .create_store("Test Library")
        .await
        .unwrap();
    let database = sources.store(handle);
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let manager = TestLibrary::open_with(
        database,
        app_dir,
        fakes.providers(),
        bae_util::http::Http::for_test(),
        diagnostics,
    )
    .await;

    // Insert the test artist that create_test_album() references
    let artist = DbArtist {
        id: bae_util::test_ids::test_uuid("e36744a5-1a36-460f-891c-e7e558034edf"),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    manager.records().insert_artist(&artist).await.unwrap();
    (TestManager { library: manager, fakes, storage, installation }, temp_dir)
}

// One-line reads and writes against the manager's database. Spelled out at a
// call site each is `manager.database.x(..).await.unwrap()`, which rustfmt
// breaks across five lines.
pub(super) async fn find_release(manager: &LibraryManager, release_id: &str) -> Option<DbRelease> {
    manager.records.find_release_by_id(release_id).await.unwrap()
}

pub(super) async fn find_album(manager: &LibraryManager, album_id: &str) -> Option<DbAlbum> {
    manager.records.find_album_by_id(album_id).await.unwrap()
}

pub(super) async fn release_files(manager: &LibraryManager, release_id: &str) -> Vec<DbFile> {
    manager.records.get_files_for_release(release_id).await.unwrap()
}

/// The coven file the release's cover row references.
pub(super) async fn cover_file(manager: &LibraryManager, release_id: &str) -> coven::FileRef {
    manager
        .library_image_file_for_test(release_id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("the release has a cover")
}

/// How many of a release's files wait in this device's upload queue: moving
/// to the cloud from here.
pub(super) async fn moving_file_count(manager: &LibraryManager, release_id: &str) -> i64 {
    manager.count_pending_uploads_for_release(release_id).await.unwrap()
}

pub(super) async fn album_releases(manager: &LibraryManager, album_id: &str) -> Vec<DbRelease> {
    manager.records.get_releases_for_album(album_id).await.unwrap()
}

pub(super) async fn has_pending_uploads(manager: &LibraryManager, release_id: &str) -> bool {
    manager.records.has_pending_uploads_for_release(release_id).await.unwrap()
}

/// Insert `release` with its first medium, one CD side, which
/// [`DbTrack::new_test`] tracks sit on.
/// The download queue as its table holds it now.
pub(super) async fn download_queue_now(
    manager: &LibraryManager,
) -> bae_transfers::DownloadSnapshot {
    manager.download_snapshot().await.unwrap()
}

/// Wait until the download queue's live read holds what `done` looks for.
pub(super) async fn download_queue_until(
    manager: &LibraryManager,
    done: impl Fn(&bae_transfers::DownloadSnapshot) -> bool,
) -> bae_transfers::DownloadSnapshot {
    let mut values = manager.subscribe_download_snapshot();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let queue = values.next().await.expect("the query stays open").unwrap();
            if done(&queue) {
                return queue;
            }
        }
    })
    .await
    .expect("the download queue reaches the state waited for")
}

pub(super) async fn insert_release(manager: &LibraryManager, release: &DbRelease) {
    crate::test_libraries::insert_release(&manager.records, release).await;
}

pub(super) fn create_test_album() -> DbAlbum {
    crate::test_libraries::test_album()
}

pub(super) fn create_test_release(album_id: &str) -> DbRelease {
    crate::test_libraries::test_release(album_id)
}

/// A manager holding one album with one release — the spine most of these
/// tests start from before adding tracks, files, or covers of their own. The
/// `TempDir` owns the library directory, so callers keep it.
pub(super) async fn manager_with_release() -> (TestManager, TempDir, DbAlbum, DbRelease) {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    (test, temp_dir, album, release)
}

// Fixtures for the cloud-backed release states the storage, upload, pin and
// download tests assert over: connecting a manager to an in-memory storage,
// putting a release through upload at each stage it can be caught in, and
// reading back what coven kept on this device.

/// Connect injected storage as [`connect_test_cloud_uploading`] does, then
/// pause uploads while a test seeds its queue. Sync stays started: coven
/// uploads only while it is, so the test's own drain sends what it seeds.
pub(super) async fn connect_test_cloud(manager: &TestManager) -> Arc<MemoryStorage> {
    connect_test_cloud_uploading(manager).await;
    manager.library.manager().store.set_uploads_paused(true).await.unwrap();
    manager.storage.clone()
}

/// Connect injected storage and wait for a pass begun after the setup to end
/// synced, so what this device wrote before the setup is in storage; sync and
/// uploads run. The setup's own end reads as synced already, while those
/// writes still wait in the send queue for a pass to send them.
pub(super) async fn connect_test_cloud_uploading(manager: &TestManager) -> Arc<MemoryStorage> {
    manager
        .library
        .manager()
        .store
        .setup_s3_storage(
            manager.storage.config(),
            "Test Device",
            "test-key".into(),
            coven::SecretText::new("test-secret".into()),
            &|_| (),
        )
        .await
        .unwrap();
    let pass = manager.library.manager().retry_now().await.unwrap().await;
    assert!(matches!(pass, bae_sync_status::SyncRetryOutcome::Synced), "{pass:?}");
    manager.storage.clone()
}

/// An attached release whose files are queued in coven and have not uploaded.
/// The releases uploading and each release's upload read this committed queue.
/// The manager must already be connected via [`connect_test_cloud`].
pub(super) async fn insert_release_with_queued_uploads(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.upload_release(&release.id, false).await.unwrap();
    assert_eq!(
        manager.records.queued_upload_count_for_release_for_test(&release.id).await.unwrap(),
        files.len(),
        "every file must be queued before the drain runs"
    );
    release
}

/// A release id paired with its album id, for the fixtures that make a release
/// Remote through the real transition (which mints its own album) and then need
/// to name that album.
pub(super) struct ReleaseRef {
    pub(super) id: String,
    pub(super) album_id: String,
}

impl ReleaseRef {
    pub(super) async fn of(manager: &LibraryManager, id: String) -> Self {
        let album_id = find_release(manager, &id).await.expect("the release exists").album_id;
        Self { id, album_id }
    }
}

/// The queues the Storage Manager's list value carries, as one read of it
/// finds them.
pub(super) async fn listed_queues(manager: &LibraryManager) -> bae_views::StorageQueues {
    let mut browse = bae_views::LibraryViews::new(manager.store.clone()).subscribe_storage_browse(
        bae_views::StorageBrowseRequest {
            sort: bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::Storage,
                direction: bae_store::SortDirection::Ascending,
            },
            filter: bae_views::StorageFilter::All,
            search: String::new(),
            windows: Default::default(),
            selected: Default::default(),
        },
    );
    browse.next().await.expect("the browse query answers").result.unwrap().queues
}

/// Wait for the current file's upload to leave coven's durable queue.
pub(super) async fn wait_for_published_file(manager: &LibraryManager, table: &str, row_id: &str) {
    let mut queued = manager.records.subscribe_row_file_queued_for_test(table, row_id);
    manager.store.sync_now();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while queued.next().await.expect("the query stays open").unwrap() {}
    })
    .await
    .expect("file uploaded");
}

/// Create a Remote release the real way: a Local release with one source file
/// on disk, attached for upload (`pin` keeps its offline copy), then uploaded.
/// Returns its id. The manager
/// must already be connected via [`connect_test_cloud`].
pub(super) async fn uploaded_release(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    uploaded_release_with_files(manager, dir, album_title, &[("track.flac", b"track-bytes")], pin)
        .await
}

pub(super) async fn uploaded_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
    pin: bool,
) -> String {
    let release = insert_local_release_with_files(manager, dir, album_title, files).await;
    manager.upload_release(&release.id, pin).await.unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, files.len(), "each release file uploaded");
    if pin {
        wait_for_kept_release(manager, &release.id).await;
    }
    release.id
}

/// Wait for every file of a release kept here as it moved to the cloud to be
/// kept whole on this device.
pub(super) async fn wait_for_kept_release(manager: &LibraryManager, release_id: &str) {
    manager.wait_for_kept_release_for_test(release_id).await;
}

/// [`uploaded_release`] for a test connected with
/// [`connect_test_cloud_uploading`]: coven drains the queue, so this
/// waits for the upload to finish rather than counting a drain pass this
/// test does not own.
pub(super) async fn upload_release_unpaused(
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
    wait_for_landed_upload(manager, &release.id).await;
    if pin {
        wait_for_kept_release(manager, &release.id).await;
    }
    release.id
}

pub(super) async fn wait_for_landed_upload(manager: &LibraryManager, release_id: &str) {
    wait_for_settled_uploads(manager, release_id).await;
    assert!(release_files(manager, release_id).await.iter().all(|file| file.stored));
}

pub(super) async fn insert_local_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let mut album = create_test_album();
    album.title = album_title.to_string();
    manager.records.insert_album(&album).await.unwrap();
    insert_local_release_in_album(manager, &album.id, dir, files).await
}

/// A local release with `files` under `dir`, on an album that already exists —
/// what a test needs when one album holds several releases.
pub(super) async fn insert_local_release_in_album(
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
        let file = NewReleaseFile::new(
            &release.id,
            name,
            bae_util::content_type::ContentType::Flac,
            bae_records::ReleaseFileRole::Other,
            bae_util::test_ids::test_uuid(&format!("{}-test-file-{index}", release.id)),
            created_at,
        );
        manager.add_external_file_for_test(&file, &path).await.unwrap();
    }
    release
}

/// A local release imported on `devices`' first device, with its one file
/// on that device only, once the second device has it too: what the second
/// device sees of a release another device holds.
pub(super) async fn release_on_the_first_device(
    devices: &crate::test_libraries::TwoDevices,
    dir: &std::path::Path,
    album_title: &str,
) -> DbRelease {
    let release = insert_local_release_with_files(
        devices.a().manager(),
        dir,
        album_title,
        &[("track1.flac", b"audio")],
    )
    .await;
    devices
        .converge(
            "SELECT f.id, f.file, c.coordinates FROM release_files AS f \
             JOIN coven_files AS c ON c.id = f.file",
        )
        .await;
    release
}

/// Read one byte through the production audio reader and return the playback
/// error reason its fill-error handler reports.
pub(super) async fn playback_error_reason_for_file(
    manager: &LibraryManager,
    file: &DbFile,
) -> bae_playback::PlaybackErrorReason {
    use bae_audio::sparse_buffer::create_sparse_buffer;
    let buffer = create_sparse_buffer(file.file_size);
    let reader = manager.create_audio_reader(&file.id);
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
    let demand = bae_util::test_blocking::on_blocking_pool(move || {
        let mut r = buffer.new_reader();
        let mut b = [0u8; 1];
        r.read(&mut b)
    });
    let reason = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        bae_playback::read_failure_reason(&error_rx.recv().await.expect("error channel open"))
    })
    .await
    .expect("a playback error must be reported");
    let read = demand.await;
    assert!(
        matches!(read, Err(bae_audio::sparse_buffer::BufferStop::Failed(_))),
        "the waiting read ends on the fetch failure: {read:?}"
    );
    reason
}

/// Store a cover on `release_id` the way the importer does — the `covers` row
/// and its file in one coven batch. Storing different bytes again replaces the
/// cover, as `change_cover` does.
pub(super) async fn store_test_cover(manager: &LibraryManager, release_id: &str, bytes: &[u8]) {
    let cover = DbLibraryImage::cover(release_id);
    manager.store_library_image(&cover, bytes).await.unwrap();
}

/// Run the bound uploads waiting now, a cover's, an artist image's or an
/// avatar's, to their end. [`connect_test_cloud`]'s pause holds only release
/// files, and coven's worker starts a bound upload as its write commits, so
/// this joins that upload rather than racing it. A retry while uploads are
/// paused tries only bound uploads, so the paused queue stays as it was.
pub(super) async fn finish_bound_uploads(manager: &LibraryManager) {
    manager.store.retry_uploads_now().await.unwrap();
}

/// Whether coven keeps the coven file a row references in the pinned cache.
/// Asked of that row's own file rather than through a release's pin marker,
/// which counts the release's files and never its cover.
pub(super) async fn file_pinned(manager: &LibraryManager, namespace: &str, row_id: &str) -> bool {
    let file = match namespace {
        bae_records::synced::RELEASE_FILES_NAMESPACE => {
            manager.release_file_ref(row_id).await.unwrap()
        }
        bae_records::synced::COVERS_NAMESPACE => cover_file(manager, row_id).await,
        other => panic!("no pinned files in {other}"),
    };
    manager
        .store
        .files_pinned(&[file.id()])
        .await
        .expect("read the file's pin state")
        .first()
        .copied()
        .flatten()
        .expect("this device has a record of the file")
}

/// Upload one real file and evict its cache so the release needs a download.
pub(super) async fn insert_pinnable_release(manager: &TestManager) -> String {
    connect_test_cloud(manager).await;
    let source = manager.library.manager().library_path().join("pin-source");
    let release = uploaded_release(manager.library.manager(), &source, "Test Album", false).await;
    let file = release_files(manager.library.manager(), &release).await.remove(0);
    manager.library.manager().evict_file_for_test(file.file).await.unwrap();
    release
}

/// The queue entry a test's direct pin of `release_id` ends in its write: the
/// release queued and started as the download worker starts it, with the
/// queue paused so the worker leaves the entry to the test.
pub(super) async fn started_pin_entry(
    manager: &LibraryManager,
    release_id: &str,
) -> bae_transfers::ActiveEntry<bae_transfers::DownloadRequest> {
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![release_id.to_string()]).await.unwrap();
    let started = release_id.to_string();
    let activated = manager
        .store
        .call(move |sql| {
            bae_transfers::tables::worker_moves::activate_on::<bae_transfers::DownloadRequest>(
                sql, &started,
            )
        })
        .await
        .unwrap();
    assert!(activated, "the pin's release is in the download queue");
    bae_transfers::tables::worker_moves::active_entry::<bae_transfers::DownloadRequest>(release_id)
}

/// Queue `release_ids` to be made local into `folder` and wait until the
/// download queue has run them: `Ok` when it holds none waiting or running
/// and none failed, else the first failure's error.
pub(super) async fn make_local_and_wait(
    manager: &LibraryManager,
    release_ids: &[String],
    folder: &std::path::Path,
) -> Result<(), String> {
    manager.make_releases_local(release_ids.to_vec(), folder).await.map_err(|e| e.to_string())?;
    let queue = download_queue_until(manager, |queue| {
        queue
            .rows
            .iter()
            .all(|row| matches!(row.op.state, bae_transfers::DownloadPhase::Failed { .. }))
    })
    .await;
    match queue.rows.into_iter().find_map(|row| match row.op.state {
        bae_transfers::DownloadPhase::Failed { error } => Some(error),
        _ => None,
    }) {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Insert `n` plain track rows on the release's one CD. Mirrors the row shape
/// `prepared.parsed.tracks` would produce so the track-count check in
/// `re_identify_release` accepts the picked release.
pub(super) async fn insert_n_tracks(records: &bae_records::Records, release_id: &str, n: usize) {
    for i in 1..=n {
        let track = bae_records::DbTrack {
            id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {i}"),
            medium_id: bae_records::identity::release_medium_id(release_id, 1),
            side: std::num::NonZeroU8::MIN,
            track_number: std::num::NonZeroU32::new(
                u32::try_from(i).expect("a test track number fits u32"),
            )
            .expect("track numbers count from one"),
            duration_ms: 200_000,
            created_at: Utc::now(),
        };
        records.insert_track(&track).await.unwrap();
    }
}

// ── set_records ────────────────────────────────────────────────────

/// Exactly one record of a release reads its draft; these fixtures put that on
/// MusicBrainz, so a release carrying both records still has one.
pub(super) fn mb_identity(group: &str, release: &str) -> bae_catalogs::records::ReleaseRecord {
    bae_catalogs::records::ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            release,
        ),
        Some(group.to_string()),
    )
}

/// An import error that crosses into a library error stays the refusal it
/// is, so the library's category is the import's own, and a store failure
/// import met is the library's store failure.
#[test]
pub(super) fn an_import_error_keeps_its_kind_through_a_library_error() {
    let library = LibraryError::from(ImportError::TiedMediums { mediums: vec![1, 3] });
    assert_eq!(library.category(), UiErrorCategory::TiedMediums { mediums: vec![1, 3] });

    let library = LibraryError::from(ImportError::MetadataTrackCount {
        folder_tracks: 10,
        release_tracks: 12,
    });
    assert_eq!(
        library.category(),
        UiErrorCategory::MetadataTrackCount { folder_tracks: 10, release_tracks: 12 }
    );

    let from_db = LibraryError::from(ImportError::Database(bae_store::DbError::unreadable(
        "a column", "a value",
    )));
    assert!(matches!(from_db, LibraryError::Database(_)), "{from_db:?}");
}

/// The storage actions a release offers on `manager`'s device, and its
/// download control.
pub(super) async fn offered(
    manager: &LibraryManager,
    release_id: &str,
) -> (Vec<ReleaseStorageAction>, Option<bae_views::album_detail::ReleaseDownloadStatus>) {
    let detail = bae_views::LibraryViews::new(manager.store.clone())
        .find_release_detail(release_id)
        .await
        .unwrap()
        .expect("the release exists");
    (detail.summary.storage_actions, detail.download_status)
}

/// Wait until coven no longer lists this release's uploads. The upload
/// queue empties when the sync loop's passes send its files; one pass is
/// asked for up front so none waits on the loop's own schedule.
pub(super) async fn wait_for_settled_uploads(manager: &LibraryManager, release_id: &str) {
    let mut pending = manager.records.subscribe_has_pending_uploads_for_release(release_id);
    manager.store.sync_now();
    crate::followed::value_matching(&mut pending, |has_pending| !has_pending).await;
}

// The pending list the sync status shows: coven's pending records named by
// the rows they are about, each with the error behind it while this process
// holds one.

/// Follow the pending list as the running library does, and wait for the
/// first list `matches` accepts.
pub(super) async fn pending_list_until(
    values: &mut impl bae_store::live_query::LiveSource<bae_sync_status::SyncStatusSnapshot>,
    matches: impl Fn(&[bae_sync_status::PendingItem]) -> bool,
) -> Vec<bae_sync_status::PendingItem> {
    crate::followed::value_matching(values, |status| matches(&status.pending)).await.pending
}

/// The first rows the inspector's read gives for the release `release_id`
/// that `accept` takes: its files with each one's pin as coven's tables hold
/// it, whether or not the inspector shows it.
pub(super) async fn inspector_rows_until(
    manager: &LibraryManager,
    release_id: &str,
    accept: impl Fn(&bae_views::DbStorageInspector) -> bool,
) -> bae_views::DbStorageInspector {
    let mut rows = bae_views::LibraryViews::new(manager.store.clone())
        .subscribe_storage_inspector(Some(release_id.to_string()));
    rows_until(&mut rows, accept).await
}

/// The first rows `rows` gives from now that `accept` takes.
pub(super) async fn rows_until(
    rows: &mut coven::ReconfigurableLiveQuery<
        Option<String>,
        Option<bae_views::DbStorageInspector>,
        bae_store::DbError,
    >,
    accept: impl Fn(&bae_views::DbStorageInspector) -> bool,
) -> bae_views::DbStorageInspector {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let event = rows.next().await.expect("the inspector's read runs");
            if let Some(value) = event.result.unwrap().filter(|value| accept(value)) {
                break value;
            }
        }
    })
    .await
    .expect("the inspector's read gives the expected rows")
}

// The harness the storage rows' upkeep tests write through: the pinned
// count (pin_tests) and the queued count (storage_tests) `release_storage`
// keeps per release, and `release_listing` copies, each against what coven
// lists, probed file by file.

/// One release the harness wrote, and where its files stand.
pub(super) struct HarnessRelease {
    pub(super) id: String,
    pub(super) stage: HarnessStage,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HarnessStage {
    Local,
    Attached,
    Remote,
}

#[test]
pub(super) fn setup_failure_classes_retain_the_actionable_category() {
    use bae_diagnostics::UiErrorCategory as C;
    use coven::StorageFailure as F;
    let missing_entitlement = || {
        coven::KeyError::MissingKeychainEntitlement(
            keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into(),
        )
    };
    let bug = || coven::task::Bug::new("test", "boom");
    let cases: Vec<(LibraryError, C)> = vec![
        (coven::StorageError::from(F::Authentication).into(), C::Storage(F::Authentication)),
        (coven::StorageError::from(F::ContainerNotFound).into(), C::Storage(F::ContainerNotFound)),
        (coven::StorageError::from(F::Network).into(), C::Storage(F::Network)),
        // A full store and a wrong clock say so, rather than blaming the
        // credentials or reporting an internal fault.
        (coven::StorageError::from(F::QuotaExceeded).into(), C::Storage(F::QuotaExceeded)),
        (coven::SyncError::Storage(F::ClockSkew.into()).into(), C::Storage(F::ClockSkew)),
        (coven::SyncError::Storage(F::Conflict.into()).into(), C::Storage(F::Conflict)),
        (coven::KeyError::PassphraseAuthentication.into(), C::Keyring),
        (coven::KeyError::InvalidServiceName.into(), C::Internal),
        (bae_preferences::PreferencesError::Invalid("invalid preference".into()).into(), C::Config),
        (coven::SyncError::SecureStorage(coven::KeyError::Unsupported).into(), C::Keyring),
        (coven::SyncError::Storage(F::Network.into()).into(), C::Storage(F::Network)),
        (coven::SyncError::NoStorage.into(), C::Config),
        (coven::SyncError::Removed.into(), C::DeviceRemoved),
        (coven::SyncError::NotAdmitted.into(), C::DeviceRemoved),
        (coven::KeyError::Left.into(), C::DeviceRemoved),
        (coven::SyncError::SecureStorage(coven::KeyError::Left).into(), C::DeviceRemoved),
        (missing_entitlement().into(), C::KeychainEntitlementMissing),
        (
            coven::SyncError::SecureStorage(missing_entitlement()).into(),
            C::KeychainEntitlementMissing,
        ),
        // Block Store's limits on Android say which one was met, not that the
        // keyring broke.
        (coven::KeyError::SavedEntriesFull.into(), C::SavedRestoreCodesFull),
        (
            coven::SyncError::SecureStorage(coven::KeyError::SavedEntryTooLarge).into(),
            C::SavedRestoreCodeTooLarge,
        ),
        (LibraryError::StorageSetup(coven::StorageSetupError::Removed), C::DeviceRemoved),
        (LibraryError::CloudUnlock(coven::StoreKeyUnlockError::Removed), C::DeviceRemoved),
        (
            LibraryError::StorageSetup(coven::StorageSetupError::UpdateRequired(
                coven::RequiredUpdate::AppSchema { version: 9 },
            )),
            C::SyncUpdateRequired,
        ),
        (
            LibraryError::CloudUnlock(coven::StoreKeyUnlockError::UpdateRequired(
                coven::RequiredUpdate::CovenFormat { version: 9 },
            )),
            C::SyncUpdateRequired,
        ),
        (LibraryError::Storage("pin ended without completion".into()), C::Internal),
        (LibraryError::Validation("library name cannot be empty".into()), C::Config),
        // A panic in coven's own storage work is a bug, never a storage
        // reason, even under a cleanup that failed for a reason of its own.
        (coven::StorageError::Bug(bug()).into(), C::Internal),
        (
            coven::SyncError::Storage(coven::StorageError::Cleanup {
                operation: Box::new(coven::StorageError::Bug(bug())),
                cleanup: Box::new(F::Network.into()),
            })
            .into(),
            C::Internal,
        ),
        (coven::SyncError::Code(coven::CodeError::Invalid).into(), C::Config),
        // A worker's panic answered again to a later call: the same bug,
        // shown as one.
        (coven::SyncError::WorkerFailed(bug()).into(), C::Internal),
        // A database call's bug, reported as it returned, and the store
        // failing on an earlier one: bugs, never the database's failure.
        (coven::DbError::Bug(bug()).into(), C::Internal),
        (coven::DbError::StoreFailed(bug()).into(), C::Internal),
        (LibraryError::file_read("read a cover", coven::FileReadError::Bug(bug())), C::Internal),
        (LibraryError::MakeLocal(Box::new(coven::DeviceOnlyError::Bug(bug()))), C::Internal),
    ];
    for (error, expected) in cases {
        assert_eq!(error.category(), expected, "{error}");
    }
}

/// A credit the library refuses shows the refusal's own line, not the
/// import's.
#[test]
fn a_refused_credit_shows_its_own_line() {
    use bae_diagnostics::UiErrorCategory as C;
    use bae_records::{IdentityRefusal, IdentityWriteError};
    let refusal = IdentityRefusal::ArtistGone("gone".to_string());
    let error = LibraryError::from(IdentityWriteError::from(refusal));
    assert_eq!(error.category(), C::ChangedMeanwhile, "{error}");
}

#[test]
pub(super) fn every_storage_setup_failure_keeps_its_exact_user_action() {
    use coven::StorageFailure as F;
    for failure in [
        F::Authentication,
        F::PermissionDenied,
        F::ContainerNotFound,
        F::RegionMismatch,
        F::QuotaExceeded,
        F::InvalidConfiguration,
        F::Network,
    ] {
        let error = LibraryError::StorageSetup(coven::StorageSetupError::Storage(failure.into()));
        assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::Storage(failure));
    }
}

/// Store `track`'s audio over new release files, `sources` in segment order:
/// the file its pregap is stored in, when there are two, then the file its
/// body plays from.
pub(super) async fn add_track_audio_sources(
    manager: &LibraryManager,
    track: &bae_records::DbTrack,
    sources: &[(&str, bae_records::source_audio::SourceAudioLayout)],
) -> Vec<NewReleaseFile> {
    assert!(matches!(sources.len(), 1 | 2), "a track's audio is a body and at most one pregap");
    let now = Utc::now();
    let mut files = Vec::with_capacity(sources.len());
    for (name, layout) in sources {
        let mut file = NewReleaseFile::new(
            &track.release_id,
            name,
            ContentType::Flac,
            bae_records::ReleaseFileRole::Audio,
            coven::IdSource::new_id(&coven::UuidIds).to_string(),
            now,
        );
        file.source_audio = Some(bae_records::source_audio::SourceAudioFile {
            layout: Some(*layout),
            format: bae_types::audio_format::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
            content_type: ContentType::Flac,
            container: bae_util::audio_container::AudioContainer::Flac,
            duration_ms: 1_000,
        });
        manager.add_file(&file, 1_000).await.unwrap();
        files.push(file);
    }
    let audio_format = bae_records::DbAudioFormat {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        track_id: track.id.clone(),
        content_type: ContentType::Flac,
        pregap_samples: None,
        generated_pregap_samples: None,
        sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
        bits_per_sample: Some(std::num::NonZeroU32::new(16).unwrap()),
        channels: std::num::NonZeroU32::new(2).unwrap(),
        track_loudness_lufs: None,
        track_peak_linear: None,
        created_at: now,
    };
    let segments = files
        .iter()
        .enumerate()
        .map(|(index, file)| bae_records::DbAudioSegment {
            id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
            audio_format_id: audio_format.id.clone(),
            segment_index: i64::try_from(index).unwrap(),
            role: if index + 1 == files.len() {
                bae_records::DbAudioSegmentRole::Main
            } else {
                bae_records::DbAudioSegmentRole::AudioPregap
            },
            file_id: file.id.clone(),
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: now,
        })
        .collect::<Vec<_>>();
    manager.insert_audio_format_with_segments_for_test(&audio_format, &segments).await.unwrap();
    files
}

/// A release-only preset (single-file CUE) that a track save must refuse.
#[cfg(test)]
pub(super) fn release_only_image_preset() -> bae_preferences::SavePreset {
    bae_preferences::SavePreset {
        id: "flac-image".to_string(),
        name: "FLAC image".to_string(),
        codec: bae_preferences::SaveCodec::Flac {
            bit_depth: bae_preferences::SaveBitDepth::Bits16,
        },
        pregap_placement: bae_preferences::SavePregapPlacement::SingleFileWithCue,
        applies_to_track: false,
        applies_to_release: true,
        embed_cover: true,
    }
}

pub(super) fn sort_by_album_title_asc() -> bae_views::StorageSortCriterion {
    bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::AlbumTitle,
        direction: bae_store::SortDirection::Ascending,
    }
}

/// A page of the storage list under `filter`, ordered by album title
/// ascending — the ordering every assertion below reads rows in.
pub(super) async fn storage_page(
    manager: &LibraryManager,
    filter: bae_views::StorageFilter,
    offset: u64,
    limit: u64,
) -> bae_views::album_detail::StoragePage {
    bae_views::LibraryViews::new(manager.store.clone())
        .get_storage_page(&sort_by_album_title_asc(), filter, offset, limit)
        .await
        .unwrap()
}

/// The release ids of the Storage Manager list under `filter`, sorted by
/// storage in `direction`.
pub(super) async fn by_storage(
    manager: &LibraryManager,
    filter: bae_views::StorageFilter,
    direction: bae_store::SortDirection,
) -> Vec<String> {
    let sort =
        bae_views::StorageSortCriterion { field: bae_views::StorageSortField::Storage, direction };
    bae_views::LibraryViews::new(manager.store.clone())
        .get_storage_page(&sort, filter, 0, 10)
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.release.id)
        .collect()
}

/// The opener the queued-upload tests share: a manager on a connected test
/// cloud holding one "Test Album" release whose single 1000-byte `a.flac` is
/// queued for upload. Returns the release and the id of that queued file;
/// `folder` names the source directory under the temp dir.
pub(super) async fn queued_upload_fixture(
    folder: &str,
) -> (TestManager, TempDir, DbRelease, String) {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let release = insert_release_with_queued_uploads(
        manager,
        &temp_dir.path().join(folder),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = release_files(manager, &release.id).await[0].id.clone();
    (test, temp_dir, release, file_id)
}
