//! Isolated installations of one person's library over shared memory
//! storage, at the store level: making the first device's store, restoring
//! another of the person's devices from its code, and running two devices'
//! sync until they agree. Nothing a crate above uses holds a device's store;
//! the harness a test runs on holds each device and passes its parts in.

use std::sync::Arc;
use std::time::Duration;

use coven::Storage;

use crate::Database;

/// How long a test waits for sync to reach a state before failing.
const SETTLE: Duration = Duration::from_secs(20);

/// The S3 key the library's storage is reached with on every device.
fn owner_credentials() -> coven::S3Credentials {
    coven::S3Credentials {
        access_key_id: "owner-key".into(),
        secret_access_key: coven::SecretText::new("owner-secret".into()),
    }
}

/// Create the library's store in `layout` as its first device does, set its
/// storage up on `storage`, and wait until it has synced.
pub async fn create_first_store(
    installation: &coven::TestCoven,
    layout: coven::StoreLayout,
    storage: Arc<coven::MemoryStorage>,
) -> coven::CovenHandle {
    let handle = crate::configure_store(installation.builder(layout))
        .storage_connector(storage.clone())
        .create_store("Two Devices")
        .await
        .unwrap();
    let credentials = owner_credentials();
    handle
        .setup_s3_storage(
            storage.config(),
            "Device A",
            credentials.access_key_id,
            credentials.secret_access_key,
            &|_| (),
        )
        .await
        .unwrap();
    handle
        .test_sync_status()
        .wait_for(|status| matches!(status, coven::SyncStatus::Synced { .. }))
        .await;
    handle
}

/// Restore another of the person's devices into `layout` from `code`, a
/// restore code the first device showed, as the app restores one, over
/// `storage`, that device's own connection to the library's storage.
pub async fn restore_store(
    installation: &coven::TestCoven,
    layout: coven::StoreLayout,
    storage: Arc<coven::MemoryStorage>,
    code: &str,
    name: &str,
) -> coven::CovenHandle {
    let builder = crate::configure_store(installation.builder(layout)).storage_connector(storage);
    let (_sender, cancel) = tokio::sync::watch::channel(false);
    coven::restore_from_code(builder, code, Some(owner_credentials()), name, |_| {}, &cancel)
        .await
        .unwrap()
}

/// Open the device's store `store` again with `migrations` in place of
/// bae's own, as an update of the app that brings a new schema does.
pub async fn reopen_store(
    installation: &coven::TestCoven,
    layout: coven::StoreLayout,
    storage: Arc<coven::MemoryStorage>,
    store: coven::StoreId,
    migrations: Vec<coven::Migration>,
) -> coven::CovenHandle {
    crate::configure_store(installation.builder(layout))
        .migrations(migrations)
        .storage_connector(storage)
        .open(store, |_| ())
        .await
        .expect("open the device's store again")
}

/// The database over a device's opened store, under the system clock.
pub fn device_database(handle: coven::CovenHandle) -> Database {
    Database::from_handle(handle, Arc::new(coven::SystemClock), Arc::new(coven::UuidIds))
}

/// coven's sync status on `database`, live.
fn sync_status(database: &Database) -> coven::LiveQuery<coven::SyncStatus> {
    database.subscribe(|sql| {
        crate::client::sync_status::coven_status_on(&sql).map_err(coven::CovenError::from)
    })
}

/// Stop the device's sync loop and wait until it has: the pass running ends
/// first, and no pass starts until sync starts again.
pub async fn stop_sync(database: &Database) {
    database.stop_sync().await.unwrap();
    let mut status = sync_status(database);
    tokio::time::timeout(SETTLE, async {
        loop {
            let current = status.next().await.expect("the status ended").expect("read the status");
            if matches!(current, coven::SyncStatus::Stopped) {
                return;
            }
        }
    })
    .await
    .expect("sync stopped");
}

/// What a device's completed sync cycles reported: how many have completed,
/// and what the latest one held back from the library.
#[derive(Default)]
struct Cycles {
    completed: u64,
    last: String,
}

/// The record of every sync cycle a device's store completes, for as long
/// as the record lives.
pub struct SyncCycles {
    cycles: Arc<std::sync::Mutex<Cycles>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl SyncCycles {
    /// Record the cycles `database` completes from now on: the status as it
    /// stands is no cycle.
    pub fn follow(database: &Database) -> Self {
        let cycles = Arc::new(std::sync::Mutex::new(Cycles::default()));
        let mut status = sync_status(database);
        let recorded = cycles.clone();
        let watcher = tokio::spawn(async move {
            let mut previous = match status.next().await {
                Some(Ok(status)) => status,
                _ => return,
            };
            while let Some(Ok(current)) = status.next().await {
                if current == previous {
                    continue;
                }
                previous = current.clone();
                let last = match &current {
                    coven::SyncStatus::Synced { .. } => format!("{current:?}"),
                    coven::SyncStatus::Failed { error, .. } => format!("failed: {error}"),
                    _ => continue,
                };
                let mut cycles = recorded.lock().expect("cycle record");
                cycles.completed += 1;
                cycles.last = last;
            }
        });
        Self { cycles, _watcher: watcher }
    }

    /// How many cycles have completed.
    pub fn completed(&self) -> u64 {
        self.cycles.lock().expect("cycle record").completed
    }

    /// What the latest completed cycle reported.
    pub fn last(&self) -> String {
        self.cycles.lock().expect("cycle record").last.clone()
    }
}

/// Run `database`'s cycles until `condition` holds, or fail naming `what`.
/// It requests passes of this device only, so another device runs only the
/// passes its own work starts.
pub async fn wait_until<F, Fut>(database: &Database, cycles: &SyncCycles, what: &str, condition: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + SETTLE;
    loop {
        database.sync_now();
        if condition().await {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("{what} never came. The last cycle: {}", cycles.last());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Wait until `condition` holds and neither device has a write left to
/// send, then for two more completed cycles on each device — so the verdict
/// is a cycle that began after every write either device made was in
/// storage — and require all of it still holds and nothing is held back. A
/// device's write leaves its queue only once its slot is in storage, so a
/// condition can hold on one device while the other's write that changes the
/// outcome is still on its way out.
pub async fn settle<F, Fut>(
    (a, a_cycles): (&Database, &SyncCycles),
    (b, b_cycles): (&Database, &SyncCycles),
    what: &str,
    condition: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + SETTLE;
    let mut settling: Option<(u64, u64)> = None;
    loop {
        a.sync_now();
        b.sync_now();
        let quiet = a.unsent_writes_for_test().await.unwrap() == 0
            && b.unsent_writes_for_test().await.unwrap() == 0
            && condition().await;
        let (completed_a, completed_b) = (a_cycles.completed(), b_cycles.completed());
        if !quiet {
            settling = None;
        } else {
            match settling {
                None => settling = Some((completed_a, completed_b)),
                Some((from_a, from_b))
                    if completed_a >= from_a + 2 && completed_b >= from_b + 2 =>
                {
                    if a.pending().await.unwrap().is_empty()
                        && b.pending().await.unwrap().is_empty()
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
                a.pending().await.unwrap(),
                a.unsent_writes_for_test().await.unwrap(),
                a_cycles.last(),
                b.pending().await.unwrap(),
                b.unsent_writes_for_test().await.unwrap(),
                b_cycles.last(),
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Run sync on both devices until they answer `query` with the same rows
/// and neither holds anything back: each has pulled what the other wrote.
pub async fn converge(a: (&Database, &SyncCycles), b: (&Database, &SyncCycles), query: &str) {
    settle(a, b, &format!("the rows of {query:?}"), || async {
        sorted_rows(a.0, query).await == sorted_rows(b.0, query).await
    })
    .await;
}

async fn sorted_rows(database: &Database, query: &str) -> Vec<String> {
    let mut rows = database.query_texts_for_test(query).await.expect("read the compared rows");
    rows.sort();
    rows
}

/// Run `body` to its end on its own thread and multi-threaded runtime, both
/// with deep stacks: restoration and snapshot installation recurse further
/// than a default test thread allows.
pub fn on_device_runtime(body: impl AsyncFnOnce() + Send + 'static) {
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

/// One device of a store-level test: its store's database, the record of its
/// sync cycles, and what keeps its installation alive.
#[cfg(test)]
pub(crate) struct StoreDevice {
    database: Database,
    cycles: SyncCycles,
    _installation: coven::TestCoven,
    _dir: tempfile::TempDir,
}

#[cfg(test)]
impl StoreDevice {
    fn new(
        handle: coven::CovenHandle,
        installation: coven::TestCoven,
        dir: tempfile::TempDir,
    ) -> Self {
        let database = device_database(handle);
        let cycles = SyncCycles::follow(&database);
        Self { database, cycles, _installation: installation, _dir: dir }
    }

    fn synced(&self) -> (&Database, &SyncCycles) {
        (&self.database, &self.cycles)
    }
}

/// The library's first device and one restored from it, both syncing over
/// shared memory storage, with no library running on either: what a test of
/// how db's rows sync between devices runs on.
#[cfg(test)]
pub(crate) struct TwoStores {
    a: StoreDevice,
    b: StoreDevice,
}

/// Run `test` over two stores, each closed before the runtime ends: a store
/// still syncing as its runtime shuts down has its next database call
/// cancelled, which coven reports as a panic.
#[cfg(test)]
pub(crate) fn run_two_store_test<F>(test: F)
where
    F: AsyncFnOnce(&TwoStores) + Send + 'static,
{
    on_device_runtime(async move || {
        let stores = TwoStores::new().await;
        test(&stores).await;
        for store in [&stores.a, &stores.b] {
            store.database.close().await.expect("close the device's store");
        }
    });
}

#[cfg(test)]
impl TwoStores {
    async fn new() -> Self {
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let storage = Arc::new(coven::MemoryStorage::builder().clock(clock).build().unwrap());
        let dir = tempfile::TempDir::new().unwrap();
        let installation = coven::TestCoven::new();
        let handle = create_first_store(
            &installation,
            coven::StoreLayout::new(dir.path().to_path_buf()),
            storage.clone(),
        )
        .await;
        let a = StoreDevice::new(handle, installation, dir);
        let code = a.database.generate_restore_code().await.unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let installation = coven::TestCoven::new();
        let handle = restore_store(
            &installation,
            coven::StoreLayout::new(dir.path().to_path_buf()),
            Arc::new(storage.for_device()),
            &code,
            "Device B",
        )
        .await;
        let b = StoreDevice::new(handle, installation, dir);
        b.database.start_sync().await.unwrap();
        Self { a, b }
    }

    /// The device that created the library.
    pub(crate) fn a(&self) -> &Database {
        &self.a.database
    }

    /// The device restored from the first one's code.
    pub(crate) fn b(&self) -> &Database {
        &self.b.database
    }

    /// Wait until both sync loops have stopped, before making concurrent
    /// offline edits.
    pub(crate) async fn pause(&self) {
        for store in [&self.a, &self.b] {
            stop_sync(&store.database).await;
        }
    }

    pub(crate) async fn resume(&self) {
        for store in [&self.a, &self.b] {
            store.database.start_sync().await.unwrap();
        }
    }

    /// Run sync on both until they answer `query` with the same rows.
    pub(crate) async fn converge(&self, query: &str) {
        converge(self.a.synced(), self.b.synced(), query).await;
    }
}
