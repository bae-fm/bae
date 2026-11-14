//! Isolated installations of one person's library over shared memory
//! storage, each running the library: the device that created it, and
//! devices restored from its code. The stores under them are made, synced
//! and compared by `bae_store::test_devices`.
use std::sync::Arc;

use tempfile::TempDir;

use crate::LibraryManager;
use bae_app_dir::AppDir;
use bae_store::test_devices::{self as stores, SyncCycles};
use bae_store::Store;
use bae_sync_status::SyncStatus;

pub struct TestDevice {
    library: RunningLibrary,
    /// What opening the device's store again needs: its installation, its
    /// app directory and its connection to the shared storage.
    installation: coven::Coven,
    app: AppDir,
    storage: Arc<coven::MemoryStorage>,
    /// The clock and ids every device of the library runs on.
    sources: super::TestSources,
    _dir: TempDir,
}

/// The library a device runs over its open store, with the record of its
/// sync cycles.
struct RunningLibrary {
    manager: LibraryManager,
    store: Store,
    records: bae_records::Records,
    people: bae_people::People,
    membership: bae_people::Membership,
    sync_status: SyncStatus,
    cycles: SyncCycles,
}

impl TestDevice {
    pub fn manager(&self) -> &LibraryManager {
        &self.library.manager
    }

    pub fn store(&self) -> &Store {
        &self.library.store
    }

    pub fn records(&self) -> &bae_records::Records {
        &self.library.records
    }

    /// The device's people.
    pub fn people(&self) -> &bae_people::People {
        &self.library.people
    }

    pub fn membership(&self) -> &bae_people::Membership {
        &self.library.membership
    }

    /// The device's sync status.
    pub fn sync_status(&self) -> &SyncStatus {
        &self.library.sync_status
    }

    /// Stop the device's sync loop: the pass running ends first, and once
    /// this returns no pass or upload runs until sync starts again.
    pub async fn stop_sync(&self) {
        stores::stop_sync(self.store()).await;
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
        stores::wait_until(self.store(), &self.library.cycles, what, condition).await;
    }
}

/// A library on S3 memory storage, created and connected by its first device,
/// which has synced. More of the person's devices restore from its code.
pub struct DeviceLibrary {
    first: TestDevice,
    storage: Arc<coven::MemoryStorage>,
    sources: super::TestSources,
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

impl DeviceLibrary {
    /// Another of the person's devices, paired with `code`, a pairing code
    /// the first device showed, as the app pairs one, and syncing. It
    /// returns once the first device approved the request; `None` when the
    /// pairing ended first.
    pub async fn pair(
        &self,
        code: &str,
        credentials: Option<coven::S3Credentials>,
        name: &str,
    ) -> Option<TestDevice> {
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = self.sources.installation(app.coven_app_dir());
        let storage = Arc::new(self.storage.for_device());
        let code = crate::sync::restore_code::entered(code);
        let handle = stores::pair_store(
            crate::SCHEMA,
            &installation,
            &self.sources,
            storage.clone(),
            &code,
            credentials,
            name,
        )
        .await?;
        let device = device(dir, app, handle, installation, storage, self.sources.clone()).await;
        device.store().start_sync().await.unwrap();
        Some(device)
    }
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
        let sources = super::TestSources::system();
        let storage = Arc::new(
            coven::MemoryStorage::builder().clock(sources.clock().clone()).build().unwrap(),
        );
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = sources.installation(app.coven_app_dir());
        let handle =
            stores::create_first_store(crate::SCHEMA, &installation, &sources, storage.clone())
                .await;
        let first = device(dir, app, handle, installation, storage.clone(), sources.clone()).await;
        Self { first, storage, sources }
    }

    /// The device that created the library.
    pub fn first(&self) -> &TestDevice {
        &self.first
    }

    /// Another of the person's devices, restored from `code`, a restore code
    /// the first device showed, as the app restores one: read back as a
    /// person enters it, in its groups or not, and syncing.
    pub async fn restore(&self, code: &str, name: &str) -> TestDevice {
        let dir = TempDir::new().unwrap();
        let app = AppDir::at(dir.path()).unwrap();
        let installation = self.sources.installation(app.coven_app_dir());
        let storage = Arc::new(self.storage.for_device());
        let code = crate::sync::restore_code::entered(code);
        let handle = stores::restore_store(
            crate::SCHEMA,
            &installation,
            &self.sources,
            storage.clone(),
            &code,
            name,
        )
        .await;
        let device = device(dir, app, handle, installation, storage, self.sources.clone()).await;
        device.store().start_sync().await.unwrap();
        device
    }
}

impl TwoDevices {
    async fn new() -> Self {
        let library = DeviceLibrary::new().await;
        let code = library.first.store().generate_restore_code().await.unwrap();
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
            device.store().start_sync().await.unwrap();
        }
    }

    /// Run cycles on both devices until `settled` holds for both and neither
    /// holds anything back, or fail with what each device's last cycle
    /// reported.
    pub async fn settle(&self, what: &str, settled: impl AsyncFn(&'static str, &Store) -> bool) {
        stores::settle(synced(self.a()), synced(&self.b), what, || async {
            settled("A", self.a().store()).await && settled("B", self.b.store()).await
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
            crate::SCHEMA,
            &self.installation,
            &self.sources,
            self.storage.clone(),
            self.store().store_id(),
            migrations,
        )
        .await;
        self.library = RunningLibrary::new(self.app.clone(), handle, &self.sources).await;
        self.store().start_sync().await.unwrap();
    }
}

/// The store a device runs and the record of its sync cycles, which the
/// stores' sync helpers read together.
fn synced(device: &TestDevice) -> (&Store, &SyncCycles) {
    (&device.library.store, &device.library.cycles)
}

async fn device(
    dir: TempDir,
    app: AppDir,
    handle: coven::CovenHandle,
    installation: coven::Coven,
    storage: Arc<coven::MemoryStorage>,
    sources: super::TestSources,
) -> TestDevice {
    TestDevice {
        library: RunningLibrary::new(app.clone(), handle, &sources).await,
        installation,
        app,
        storage,
        sources,
        _dir: dir,
    }
}

impl RunningLibrary {
    async fn new(
        app_dir: AppDir,
        handle: coven::CovenHandle,
        sources: &super::TestSources,
    ) -> Self {
        let store = sources.store(handle);
        let library = crate::test_libraries::TestLibrary::open(
            store.clone(),
            app_dir,
            bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
            bae_util::http::Http::for_test(),
        )
        .await;
        let manager = LibraryManager::clone(library.manager());
        let cycles = SyncCycles::follow(&store);
        Self {
            manager,
            store,
            records: library.records().clone(),
            people: library.people().clone(),
            membership: library.membership().clone(),
            sync_status: library.sync_status().clone(),
            cycles,
        }
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
