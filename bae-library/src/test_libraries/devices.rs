//! Isolated installations of one person's library over shared memory
//! storage, each running the library: the device that created it, and
//! devices restored from its code. The stores under them are made, synced
//! and compared by `bae_db::test_devices`.
use std::sync::Arc;

use tempfile::TempDir;

use crate::LibraryManager;
use bae_app_dir::AppDir;
use bae_db::test_devices::{self as stores, SyncCycles};
use bae_db::Database;

pub struct TestDevice {
    library: RunningLibrary,
    /// What opening the device's store again needs: its installation, its
    /// app directory and its connection to the shared storage.
    installation: coven::Coven,
    app: AppDir,
    storage: Arc<coven::MemoryStorage>,
    _dir: TempDir,
}

/// The library a device runs over its open store, with the record of its
/// sync cycles.
struct RunningLibrary {
    manager: LibraryManager,
    database: Database,
    cycles: SyncCycles,
}

impl TestDevice {
    pub fn manager(&self) -> &LibraryManager {
        &self.library.manager
    }

    pub fn database(&self) -> &Database {
        &self.library.database
    }

    /// Stop the device's sync loop: the pass running ends first, and once
    /// this returns no pass or upload runs until sync starts again.
    pub async fn stop_sync(&self) {
        stores::stop_sync(self.database()).await;
    }

    /// This device's connection to the shared storage; taking it offline
    /// leaves the other device's online.
    pub fn storage(&self) -> &coven::MemoryStorage {
        &self.storage
    }

    /// Run this device's cycles until `condition` holds, or fail naming
    /// `what`. It requests passes of this device only, so the other device
    /// runs only the passes its own work starts.
    pub async fn wait_until<F, Fut>(&self, what: &str, condition: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        stores::wait_until(self.database(), &self.library.cycles, what, condition).await;
    }
}

/// A library on S3 memory storage, created and connected by its first device,
/// which has synced. More of the person's devices restore from its code.
pub struct DeviceLibrary {
    first: TestDevice,
    storage: Arc<coven::MemoryStorage>,
}

pub struct TwoDevices {
    library: DeviceLibrary,
    b: TestDevice,
}

/// Run `test` over a library's first device on a runtime with a deep stack:
/// restoration and snapshot installation recurse further than a default test
/// thread allows. The first device is closed before the runtime ends, as
/// closing a library in the bridge does: a device still syncing as its runtime
/// shuts down has its next database call cancelled, which coven reports as a
/// panic. The test closes each device it restores the same way.
pub fn run_library_test<F>(test: F)
where
    F: AsyncFnOnce(&DeviceLibrary) + Send + 'static,
{
    stores::on_device_runtime(async move || {
        let library = DeviceLibrary::new().await;
        test(&library).await;
        library.first.close().await;
    });
}

/// [`run_library_test`] over the library's first device and one device
/// restored from it; both are closed before the runtime ends. The test may
/// open either device's store again (`TestDevice::reopen_with`).
pub fn run_two_device_test<F>(test: F)
where
    F: AsyncFnOnce(&mut TwoDevices) + Send + 'static,
{
    stores::on_device_runtime(async move || {
        let mut devices = TwoDevices::new().await;
        test(&mut devices).await;
        devices.close().await;
    });
}

impl TwoDevices {
    pub fn a(&self) -> &TestDevice {
        &self.library.first
    }

    pub fn b(&self) -> &TestDevice {
        &self.b
    }

    pub fn a_mut(&mut self) -> &mut TestDevice {
        &mut self.library.first
    }

    pub fn b_mut(&mut self) -> &mut TestDevice {
        &mut self.b
    }
}

impl DeviceLibrary {
    async fn new() -> Self {
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let storage = Arc::new(coven::MemoryStorage::builder().clock(clock).build().unwrap());
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = coven::Coven::in_memory(app.coven_app_dir());
        let handle = stores::create_first_store(&installation, storage.clone()).await;
        Self { first: device(dir, app, handle, installation, storage.clone()).await, storage }
    }

    /// The device that created the library.
    pub fn first(&self) -> &TestDevice {
        &self.first
    }

    /// Another of the person's devices, restored from `code`, a restore code
    /// the first device showed, as the app restores one, and syncing.
    pub async fn restore(&self, code: &str, name: &str) -> TestDevice {
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = coven::Coven::in_memory(app.coven_app_dir());
        let storage = Arc::new(self.storage.for_device());
        let handle = stores::restore_store(&installation, storage.clone(), code, name).await;
        let device = device(dir, app, handle, installation, storage).await;
        device.database().start_sync().await.unwrap();
        device
    }
}

impl TwoDevices {
    async fn new() -> Self {
        let library = DeviceLibrary::new().await;
        let code = library.first.database().generate_restore_code().await.unwrap();
        let b = library.restore(&code, "Device B").await;
        Self { library, b }
    }

    /// Close both devices' libraries.
    async fn close(&self) {
        for device in [self.a(), &self.b] {
            device.close().await;
        }
    }

    /// Stop both loops before making concurrent offline edits.
    pub async fn pause(&self) {
        for device in [self.a(), &self.b] {
            device.stop_sync().await;
        }
    }

    pub async fn resume(&self) {
        for device in [self.a(), &self.b] {
            device.database().start_sync().await.unwrap();
        }
    }

    /// Run cycles on both devices until `settled` holds for both and neither
    /// holds anything back, or fail with what each device's last cycle
    /// reported.
    pub async fn settle(&self, what: &str, settled: impl AsyncFn(&'static str, &Database) -> bool) {
        stores::settle(synced(self.a()), synced(&self.b), what, || async {
            settled("A", self.a().database()).await && settled("B", self.b.database()).await
        })
        .await;
    }

    /// Run cycles until both devices answer `query` with the same rows and
    /// neither holds anything back: each has pulled what the other wrote.
    pub async fn converge(&self, query: &str) {
        stores::converge(synced(self.a()), synced(&self.b), query).await;
    }
}

impl TestDevice {
    /// Close the device's library: stop its sync loop, end the manager's
    /// tasks and close its store.
    pub async fn close(&self) {
        self.library.manager.close().await.expect("close the device's library");
    }

    /// Close the device's library and open its store again with
    /// `migrations` in place of bae's own, as an update of the app that
    /// brings a new schema does, then sync again.
    pub async fn reopen_with(&mut self, migrations: Vec<coven::Migration>) {
        self.close().await;
        let handle = stores::reopen_store(
            &self.installation,
            self.storage.clone(),
            self.database().store_id(),
            migrations,
        )
        .await;
        self.library = RunningLibrary::new(self.app.clone(), handle).await;
        self.database().start_sync().await.unwrap();
    }
}

/// The store a device runs and the record of its sync cycles, which the
/// stores' sync helpers read together.
fn synced(device: &TestDevice) -> (&Database, &SyncCycles) {
    (&device.library.database, &device.library.cycles)
}

async fn device(
    dir: TempDir,
    app: AppDir,
    handle: coven::CovenHandle,
    installation: coven::Coven,
    storage: Arc<coven::MemoryStorage>,
) -> TestDevice {
    TestDevice {
        library: RunningLibrary::new(app.clone(), handle).await,
        installation,
        app,
        storage,
        _dir: dir,
    }
}

impl RunningLibrary {
    async fn new(app_dir: AppDir, handle: coven::CovenHandle) -> Self {
        let database = stores::device_database(handle);
        let library = crate::test_libraries::TestLibrary::open(
            database.clone(),
            app_dir,
            bae_catalogs::providers::Providers::offline(),
        )
        .await;
        let manager = LibraryManager::clone(&library);
        let cycles = SyncCycles::follow(&database);
        Self { manager, database, cycles }
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
