//! Isolated installations of one person's library over shared memory
//! storage: the device that created it, and devices restored from its code.
use std::sync::Arc;
use std::time::Duration;

use coven::Storage;
use tempfile::TempDir;

use crate::config::{AppDir, Config, ConfigHandle};
use crate::db::Database;
use crate::library::LibraryManager;

/// How long a test waits for sync to reach a state before failing.
const SETTLE: Duration = Duration::from_secs(20);

pub(crate) struct TestDevice {
    library: RunningLibrary,
    /// What opening the device's store again needs: its keychain, its app
    /// directory, its settings and its connection to the shared storage.
    installation: coven::TestCoven,
    app: AppDir,
    config: Config,
    storage: Arc<coven::MemoryStorage>,
    _dir: TempDir,
}

/// The library a device runs over its open store, with the record of its
/// sync cycles.
struct RunningLibrary {
    manager: LibraryManager,
    database: Database,
    cycles: Arc<std::sync::Mutex<Cycles>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl TestDevice {
    pub(crate) fn manager(&self) -> &LibraryManager {
        &self.library.manager
    }

    pub(crate) fn database(&self) -> &Database {
        &self.library.database
    }

    /// Stop the device's sync loop and wait until it has: the pass running
    /// ends first, and no pass starts until sync starts again.
    pub(crate) async fn stop_sync(&self) {
        self.database().stop_sync();
        let mut status = self.database().subscribe_sync_status();
        tokio::time::timeout(
            SETTLE,
            status.wait_for(|status| matches!(status, coven::SyncStatus::Stopped)),
        )
        .await
        .unwrap()
        .unwrap();
    }

    /// This device's connection to the shared storage; taking it offline
    /// leaves the other device's online.
    pub(crate) fn storage(&self) -> &coven::MemoryStorage {
        &self.storage
    }

    /// Run this device's cycles until `condition` holds, or fail naming
    /// `what`. It requests passes of this device only, so the other device
    /// runs only the passes its own work starts.
    pub(crate) async fn wait_until<F, Fut>(&self, what: &str, condition: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let deadline = tokio::time::Instant::now() + SETTLE;
        loop {
            self.database().sync_now();
            if condition().await {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("{what} never came. The last cycle: {}", self.last());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// A library on S3 memory storage, created and connected by its first device,
/// which has synced. More of the person's devices restore from its code.
pub(crate) struct Library {
    first: TestDevice,
    storage: Arc<coven::MemoryStorage>,
}

pub(crate) struct TwoDevices {
    library: Library,
    b: TestDevice,
}

/// Run `test` over a library's first device on a runtime with a deep stack:
/// restoration and snapshot installation recurse further than a default test
/// thread allows. The first device is closed before the runtime ends, as
/// closing a library in the bridge does: a device still syncing as its runtime
/// shuts down has its next database call cancelled, which coven reports as a
/// panic. The test closes each device it restores the same way.
pub(crate) fn run_library_test<F>(test: F)
where
    F: AsyncFnOnce(&Library) + Send + 'static,
{
    on_device_runtime(async move || {
        let library = Library::new().await;
        test(&library).await;
        library.first.close().await;
    });
}

/// [`run_library_test`] over the library's first device and one device
/// restored from it; both are closed before the runtime ends. The test may
/// open either device's store again (`TestDevice::reopen_with`).
pub(crate) fn run_two_device_test<F>(test: F)
where
    F: AsyncFnOnce(&mut TwoDevices) + Send + 'static,
{
    on_device_runtime(async move || {
        let mut devices = TwoDevices::new().await;
        test(&mut devices).await;
        devices.close().await;
    });
}

/// Run `body` to its end on its own thread and multi-threaded runtime, both
/// with deep stacks.
fn on_device_runtime(body: impl AsyncFnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("device-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("build the device test runtime")
                .block_on(body())
        })
        .expect("spawn the device test thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

impl TwoDevices {
    pub(crate) fn a(&self) -> &TestDevice {
        &self.library.first
    }

    pub(crate) fn b(&self) -> &TestDevice {
        &self.b
    }

    pub(crate) fn a_mut(&mut self) -> &mut TestDevice {
        &mut self.library.first
    }

    pub(crate) fn b_mut(&mut self) -> &mut TestDevice {
        &mut self.b
    }
}

impl Library {
    async fn new() -> Self {
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let storage = Arc::new(
            coven::MemoryStorage::builder()
                .clock(clock)
                .build()
                .unwrap(),
        );
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = coven::TestCoven::new();
        let handle = crate::library::configure_store(installation.builder(app.store_layout()))
            .storage_connector(storage.clone())
            .create_store("Two Devices")
            .await
            .unwrap();
        handle
            .setup_s3_storage(
                storage.config(),
                "Device A",
                "owner-key".into(),
                coven::SecretText::new("owner-secret".into()),
            )
            .await
            .unwrap();
        let mut status = handle.subscribe_sync_status();
        tokio::time::timeout(
            SETTLE,
            status.wait_for(|status| matches!(status, coven::SyncStatus::Synced { .. })),
        )
        .await
        .unwrap()
        .unwrap();
        let config = Config::with_defaults(
            handle.store_id(),
            app.registered_library(&handle.store_id().to_string()),
        );
        Self {
            first: device(dir, app, config, handle, installation, storage.clone()),
            storage,
        }
    }

    /// The device that created the library.
    pub(crate) fn first(&self) -> &TestDevice {
        &self.first
    }

    /// Another of the person's devices, restored from `code`, a restore code
    /// the first device showed, as the app restores one, and syncing.
    pub(crate) async fn restore(&self, code: &str, name: &str) -> TestDevice {
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = coven::TestCoven::new();
        let storage = Arc::new(self.storage.for_device());
        let builder = crate::library::configure_store(installation.builder(app.store_layout()))
            .storage_connector(storage.clone());
        let (_sender, cancel) = tokio::sync::watch::channel(false);
        let handle = coven::restore_from_code(
            builder,
            code,
            Some(coven::S3Credentials {
                access_key_id: "owner-key".into(),
                secret_access_key: coven::SecretText::new("owner-secret".into()),
            }),
            name,
            |_| {},
            &cancel,
        )
        .await
        .unwrap();
        let config = Config::with_defaults(
            handle.store_id(),
            app.registered_library(&handle.store_id().to_string()),
        );
        let device = device(dir, app, config, handle, installation, storage);
        device.database().start_sync().await.unwrap();
        device
    }
}

impl TwoDevices {
    async fn new() -> Self {
        let library = Library::new().await;
        let code = library
            .first
            .database()
            .generate_restore_code()
            .await
            .unwrap();
        let b = library.restore(&code, "Device B").await;
        Self { library, b }
    }

    /// Close both devices' libraries.
    async fn close(&self) {
        for device in [self.a(), &self.b] {
            device.close().await;
        }
    }

    /// Wait until both loops have stopped before making concurrent offline edits.
    pub(crate) async fn pause(&self) {
        for device in [self.a(), &self.b] {
            device.stop_sync().await;
        }
    }

    pub(crate) async fn resume(&self) {
        for device in [self.a(), &self.b] {
            device.database().start_sync().await.unwrap();
        }
    }

    /// Run cycles on both devices until `settled` holds for both and neither
    /// holds anything back, or fail with what each device's last cycle
    /// reported.
    pub(crate) async fn settle<F, Fut>(&self, what: &str, settled: F)
    where
        F: Fn(&'static str, Database) -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        self.settle_on(what, || async {
            settled("A", self.a().database().clone()).await
                && settled("B", self.b.database().clone()).await
        })
        .await;
    }

    /// Run cycles until both devices answer `query` with the same rows and
    /// neither holds anything back: each has pulled what the other wrote.
    pub(crate) async fn converge(&self, query: &str) {
        self.settle_on(&format!("the rows of {query:?}"), || async {
            sorted_rows(self.a().database(), query).await
                == sorted_rows(self.b.database(), query).await
        })
        .await;
    }

    /// Wait until `condition` holds and neither device has a write left to
    /// send, then for two more completed cycles on each device — so the
    /// verdict is a cycle that began after every write either device made was
    /// in storage — and require all of it still holds and nothing is held back.
    /// A device's write leaves its queue only once its slot is in storage, so
    /// a condition can hold on one device while the other's write that
    /// changes the outcome is still on its way out.
    async fn settle_on<F, Fut>(&self, what: &str, condition: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let deadline = tokio::time::Instant::now() + SETTLE;
        let mut settling: Option<(u64, u64)> = None;
        loop {
            self.a().database().sync_now();
            self.b.database().sync_now();
            let quiet = self.a().database().unsent_writes_for_test().await.unwrap() == 0
                && self.b.database().unsent_writes_for_test().await.unwrap() == 0
                && condition().await;
            let (completed_a, completed_b) = (self.a().completed(), self.b.completed());
            if !quiet {
                settling = None;
            } else {
                match settling {
                    None => settling = Some((completed_a, completed_b)),
                    Some((from_a, from_b))
                        if completed_a >= from_a + 2 && completed_b >= from_b + 2 =>
                    {
                        if self.a().database().pending().await.unwrap().is_empty()
                            && self.b.database().pending().await.unwrap().is_empty()
                        {
                            return;
                        }
                    }
                    Some(_) => {}
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!(
                    "{what} never settled.\nA held {:?} with {} unsent writes after {}\n\
                     B held {:?} with {} unsent writes after {}",
                    self.a().database().pending().await.unwrap(),
                    self.a().database().unsent_writes_for_test().await.unwrap(),
                    self.a().last(),
                    self.b.database().pending().await.unwrap(),
                    self.b.database().unsent_writes_for_test().await.unwrap(),
                    self.b.last(),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

async fn sorted_rows(database: &Database, query: &str) -> Vec<String> {
    let mut rows = database
        .query_texts_for_test(query)
        .await
        .expect("read the compared rows");
    rows.sort();
    rows
}

/// What a device's completed sync cycles reported: how many have completed,
/// and what the latest one held back from the library.
#[derive(Default)]
struct Cycles {
    completed: u64,
    last: String,
}

impl TestDevice {
    /// Close the device's library: stop its sync loop, end the manager's
    /// tasks and close its store.
    pub(crate) async fn close(&self) {
        self.library
            .manager
            .close()
            .await
            .expect("close the device's library");
    }

    /// Close the device's library and open its store again with
    /// `migrations` in place of bae's own, as an update of the app that
    /// brings a new schema does, then sync again.
    pub(crate) async fn reopen_with(&mut self, migrations: Vec<coven::Migration>) {
        self.close().await;
        let store = self.database().store_id();
        let handle =
            crate::library::configure_store(self.installation.builder(self.app.store_layout()))
                .migrations(migrations)
                .storage_connector(self.storage.clone())
                .open(store)
                .await
                .expect("open the device's store again");
        self.library = RunningLibrary::new(self.app.clone(), self.config.clone(), handle);
        self.database().start_sync().await.unwrap();
    }

    fn completed(&self) -> u64 {
        self.library.cycles.lock().expect("cycle record").completed
    }

    fn last(&self) -> String {
        self.library
            .cycles
            .lock()
            .expect("cycle record")
            .last
            .clone()
    }
}

/// Record every cycle `database` completes, for as long as the device lives.
fn record_cycles(
    database: &Database,
) -> (Arc<std::sync::Mutex<Cycles>>, tokio::task::JoinHandle<()>) {
    let cycles = Arc::new(std::sync::Mutex::new(Cycles::default()));
    let mut status = database.subscribe_sync_status();
    let recorded = cycles.clone();
    let watcher = tokio::spawn(async move {
        while status.changed().await.is_ok() {
            let current = status.borrow_and_update().clone();
            let last = match &current {
                coven::SyncStatus::Synced { .. } => format!("{current:?}"),
                coven::SyncStatus::Failed { error } => format!("failed: {error}"),
                _ => continue,
            };
            let mut cycles = recorded.lock().expect("cycle record");
            cycles.completed += 1;
            cycles.last = last;
        }
    });
    (cycles, watcher)
}

fn device(
    dir: TempDir,
    app: AppDir,
    config: Config,
    handle: coven::CovenHandle,
    installation: coven::TestCoven,
    storage: Arc<coven::MemoryStorage>,
) -> TestDevice {
    TestDevice {
        library: RunningLibrary::new(app.clone(), config.clone(), handle),
        installation,
        app,
        config,
        storage,
        _dir: dir,
    }
}

impl RunningLibrary {
    fn new(app_dir: AppDir, config: Config, handle: coven::CovenHandle) -> Self {
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let ids: coven::IdSourceRef = Arc::new(coven::UuidIds);
        let database = Database::from_handle(handle, clock.clone());
        let manager = LibraryManager::new(
            database.clone(),
            app_dir,
            Arc::new(ConfigHandle::new(config)),
            clock,
            ids,
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_catalogs::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test()),
            crate::providers::Providers::offline(),
        );
        let (cycles, watcher) = record_cycles(&database);
        Self {
            manager,
            database,
            cycles,
            _watcher: watcher,
        }
    }
}

/// A two-device test closes both devices before its runtime ends, as closing a
/// library in the bridge does before it drops the library's runtime: nothing of
/// either device is left running for the runtime's shutdown to cancel under it.
#[test]
fn a_two_device_test_closes_both_devices_before_its_runtime_ends() {
    let databases = Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = databases.clone();
    run_two_device_test(async move |devices| {
        kept.lock().expect("kept databases").extend([
            devices.a().database().clone(),
            devices.b().database().clone(),
        ]);
    });
    let databases = std::mem::take(&mut *databases.lock().expect("kept databases"));
    assert_eq!(databases.len(), 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build a runtime to read the devices from");
    for database in databases {
        let read = runtime.block_on(database.unsent_writes_for_test());
        assert!(
            matches!(
                read,
                Err(crate::db::DbError::Coven(coven::CovenError::Database(
                    coven::DbError::StoreClosed
                )))
            ),
            "the device's store is closed: {read:?}"
        );
    }
}
