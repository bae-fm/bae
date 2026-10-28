#![cfg(feature = "test-utils")]
//! What restoring a library on a new device costs locally with bae's own
//! schema, triggers and SQL functions (`configure_store`), on in-memory
//! storage, so every millisecond is coven's and bae's work, not the network.
//! Run explicitly, in the release profile, with coven's timing lines:
//!
//!   RUST_LOG=info cargo test --release -p bae-core --features test-utils \
//!     --test test_restore_cost -- --ignored --nocapture --test-threads 1
//!
//! The large library holds 3,000 albums of 13 tracks, written ten albums a
//! write as the fixture writer seeds them, with a sync pass after each, as an
//! import's writes upload. The small one holds ten albums and 300 renames,
//! each its own slot, restored with 100 ms per provider request. Runs that age
//! the snapshots past retention's week of grace restore from the floor, as on
//! an older library; the others restore from the first slot.

use bae_core::config::{AppDir, Config, ConfigHandle};
use bae_core::library::{FixtureAlbum, LibraryFixture, LibraryManager};
use coven::{Clock, CovenHandle, MemoryStorage, SecretText, Storage, SyncStatus, TestCoven};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TRACKS: usize = 13;

/// What one measurement's library holds and how storage answers.
struct Library {
    albums: usize,
    albums_per_write: usize,
    /// Single-album renames after the import, each its own write and slot.
    edits: usize,
    /// Age the snapshots past retention's grace, so the restore starts from
    /// the floor.
    floor: bool,
    /// Each provider request waits this long, as a round trip to S3 does.
    latency: Duration,
}

/// The fixture's wall clock moves with real time and can jump ahead, so
/// snapshots age past retention's grace; elapsed time is real, so coven's
/// timing lines report real durations.
struct BenchClock {
    origin: Instant,
    ahead: AtomicU64,
}

impl Clock for BenchClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH
            + Duration::from_secs(1_800_000_000 + self.ahead.load(Ordering::SeqCst))
            + self.origin.elapsed()
    }
    fn elapsed(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// One sync pass that starts after this call.
async fn pass(handle: &CovenHandle, clock: &BenchClock) {
    let after = clock.now();
    let mut status = handle.subscribe_sync_status();
    status.borrow_and_update();
    handle.sync_now();
    loop {
        status.changed().await.unwrap();
        match &*status.borrow_and_update() {
            SyncStatus::Synced { finished_at } if *finished_at >= after => return,
            SyncStatus::Failed { error } => panic!("{error:?}"),
            _ => {}
        }
    }
}

fn albums(write: usize, per_write: usize) -> LibraryFixture {
    LibraryFixture {
        albums: (write * per_write..(write + 1) * per_write)
            .map(|album| FixtureAlbum {
                title: format!("Album {album}"),
                artists: vec![format!("Artist {album}")],
                tracks: (1..=TRACKS)
                    .map(|track| format!("Track {track} of album {album}"))
                    .collect(),
                cover: None,
            })
            .collect(),
        watched_folders: Vec::new(),
        playing: None,
    }
}

async fn restore_cost(library: Library) {
    let floor = library.floor;
    let clock = Arc::new(BenchClock {
        origin: Instant::now(),
        ahead: AtomicU64::new(0),
    });
    let storage = Arc::new(
        MemoryStorage::builder()
            .clock(clock.clone())
            .build()
            .unwrap(),
    );
    let owner_dir = tempfile::TempDir::new().unwrap();
    let app_dir = AppDir::at(owner_dir.path()).unwrap();
    let installation = TestCoven::new();
    let handle = bae_core::library::configure_store(installation.builder(app_dir.store_layout()))
        .clock(clock.clone())
        .storage_connector(storage.clone())
        .create_store("Library")
        .await
        .unwrap();
    handle
        .setup_s3_storage(
            storage.config(),
            "Ana's Mac",
            "key".into(),
            SecretText::new("secret".into()),
        )
        .await
        .unwrap();
    handle.start_sync().await.unwrap();
    let config = Config::with_defaults(
        handle.store_id(),
        app_dir.registered_library(&handle.store_id().to_string()),
    );
    let manager = LibraryManager::new(
        bae_core::db::Database::from_handle(handle.clone(), clock.clone()),
        app_dir.clone(),
        Arc::new(ConfigHandle::new(config)),
        clock.clone(),
        Arc::new(coven::UuidIds),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test()),
        bae_core::providers::Providers::offline(),
    );
    let building = Instant::now();
    for write in 0..library.albums / library.albums_per_write {
        manager
            .write_fixture(&albums(write, library.albums_per_write))
            .await
            .unwrap();
        pass(&handle, &clock).await;
    }
    for edit in 0..library.edits {
        let title = format!("Renamed {edit}");
        handle
            .write(move |sql| {
                sql.execute(
                    "UPDATE albums SET title = ?1 WHERE id = (SELECT id FROM albums ORDER BY id LIMIT 1)",
                    [title],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        pass(&handle, &clock).await;
    }
    if floor {
        clock.ahead.fetch_add(8 * 24 * 60 * 60, Ordering::SeqCst);
        pass(&handle, &clock).await;
        pass(&handle, &clock).await;
    }
    eprintln!(
        "built the library in {:?} (floor {floor})",
        building.elapsed()
    );
    let code = handle.restore_code_for_transfer().await.unwrap();
    drop(manager);
    handle.close().await.unwrap();
    let mut faults = coven::Faults::none();
    faults.delay = library.latency;
    storage.set_faults(faults).await;

    let device_dir = tempfile::TempDir::new().unwrap();
    let device_app_dir = AppDir::at(device_dir.path()).unwrap();
    let (_cancelling, cancel) = tokio::sync::watch::channel(false);
    let restoring = Instant::now();
    let restored = bae_core::library::restore_from_code(
        TestCoven::new()
            .builder(device_app_dir.store_layout())
            .clock(clock.clone())
            .storage_connector(Arc::new(storage.for_device())),
        &code,
        None,
        "Ana's phone",
        |_| {},
        &cancel,
    )
    .await
    .unwrap();
    let restored_in = restoring.elapsed();
    let tracks = restored
        .read(|sql| {
            let count = sql.query_row("SELECT count(*) FROM tracks", [], |row| {
                row.get::<_, i64>(0)
            })?;
            Ok(count)
        })
        .await
        .unwrap();
    assert_eq!(tracks, (library.albums * TRACKS) as i64);
    eprintln!(
        "restored {tracks} tracks in {restored_in:?} (floor {floor}, latency {:?})",
        library.latency
    );
    restored.close().await.unwrap();
}

fn logs() {
    let installed = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();
    // An earlier measurement in this process installed the subscriber.
    if let Err(error) = installed {
        eprintln!("keeping the installed log subscriber: {error}");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "one timing measurement, run explicitly in the release profile"]
async fn a_library_restore_from_its_floor_costs() {
    logs();
    restore_cost(Library {
        albums: 3_000,
        albums_per_write: 10,
        edits: 0,
        floor: true,
        latency: Duration::ZERO,
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "one timing measurement, run explicitly in the release profile"]
async fn a_young_library_restore_costs() {
    logs();
    restore_cost(Library {
        albums: 3_000,
        albums_per_write: 10,
        edits: 0,
        floor: false,
        latency: Duration::ZERO,
    })
    .await;
}

/// Ten albums with a long history of edits, as a development library has,
/// restored with 100 ms per provider request.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "one timing measurement, run explicitly in the release profile"]
async fn a_small_library_with_a_long_history_over_slow_storage_costs() {
    logs();
    restore_cost(Library {
        albums: 10,
        albums_per_write: 1,
        edits: 300,
        floor: false,
        latency: Duration::from_millis(100),
    })
    .await;
}

/// The same library past its floor, restoring from the snapshot.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "one timing measurement, run explicitly in the release profile"]
async fn a_small_library_from_its_floor_over_slow_storage_costs() {
    logs();
    restore_cost(Library {
        albums: 10,
        albums_per_write: 1,
        edits: 300,
        floor: true,
        latency: Duration::from_millis(100),
    })
    .await;
}
