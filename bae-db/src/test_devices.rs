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

/// Create the library's store on `schema` in `installation` as its first
/// device does, set its storage up on `storage`, and wait until it has
/// synced.
pub async fn create_first_store(
    schema: crate::Schema,
    installation: &coven::Coven,
    storage: Arc<coven::MemoryStorage>,
) -> coven::CovenHandle {
    let handle = crate::configure_store(installation.builder(), schema)
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
    handle.test_sync_status().wait_for(|status| status.state == coven::SyncState::Synced).await;
    handle
}

/// Restore another of the person's devices into `installation` from `code`,
/// a restore code the first device showed, as the app restores one, on
/// `schema` over `storage`, that device's own connection to the library's
/// storage.
pub async fn restore_store(
    schema: crate::Schema,
    installation: &coven::Coven,
    storage: Arc<coven::MemoryStorage>,
    code: &str,
    name: &str,
) -> coven::CovenHandle {
    let builder = crate::configure_store(installation.builder(), schema).storage_connector(storage);
    let (_sender, cancel) = tokio::sync::watch::channel(false);
    coven::restore_from_code(builder, code, Some(owner_credentials()), name, |_| {}, &cancel)
        .await
        .unwrap()
}

/// Open the device's store `store` again with `migrations` in place of
/// `schema`'s own, as an update of the app that brings a new schema does.
pub async fn reopen_store(
    schema: crate::Schema,
    installation: &coven::Coven,
    storage: Arc<coven::MemoryStorage>,
    store: coven::StoreId,
    migrations: Vec<coven::Migration>,
) -> coven::CovenHandle {
    crate::configure_store(installation.builder(), schema)
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

/// How `database`'s last pass ended and when a pass last reached the end
/// of the sequence, as coven's sync status row keeps them, live: a pass
/// that ends synced moves the time, and one that fails changes how it
/// ended.
fn pass_ends(database: &Database) -> coven::LiveQuery<(Option<coven::LastPass>, Option<i64>)> {
    database.subscribe(|sql| {
        Ok(sql.query_row("SELECT last_pass, last_synced FROM coven_sync_status", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?)
    })
}

/// Stop the device's sync loop: the pass running ends first, and once this
/// returns no pass or upload runs until sync starts again.
pub async fn stop_sync(database: &Database) {
    database.stop_sync().await.unwrap();
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
    /// Record the cycles `database` completes from now on: the last pass as
    /// it stands is no cycle.
    pub fn follow(database: &Database) -> Self {
        let cycles = Arc::new(std::sync::Mutex::new(Cycles::default()));
        let mut ends = pass_ends(database);
        let recorded = cycles.clone();
        let watcher = tokio::spawn(async move {
            let mut previous = match ends.next().await {
                Some(Ok(end)) => end,
                _ => return,
            };
            while let Some(Ok(current)) = ends.next().await {
                if current == previous {
                    continue;
                }
                previous = current.clone();
                let last = match &current {
                    (Some(coven::LastPass::Synced), Some(at)) => format!("synced at {at}"),
                    (Some(coven::LastPass::Failed { error, .. }), _) => format!("failed: {error}"),
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
#[cfg(any(test, feature = "test-utils"))]
struct StoreDevice {
    database: Database,
    cycles: SyncCycles,
    _installation: coven::Coven,
    _dir: tempfile::TempDir,
}

#[cfg(any(test, feature = "test-utils"))]
impl StoreDevice {
    fn new(handle: coven::CovenHandle, installation: coven::Coven, dir: tempfile::TempDir) -> Self {
        let database = device_database(handle);
        let cycles = SyncCycles::follow(&database);
        Self { database, cycles, _installation: installation, _dir: dir }
    }
}

/// The library's first device and one restored from it, both syncing over
/// shared memory storage, with no library running on either: what a test of
/// how db's rows sync between devices runs on.
#[cfg(any(test, feature = "test-utils"))]
pub struct TwoStores {
    a: StoreDevice,
    b: StoreDevice,
}

/// Run `test` over two stores on `schema`, each closed before the runtime
/// ends: a store still syncing as its runtime shuts down has its next
/// database call cancelled, which coven reports as a panic.
#[cfg(any(test, feature = "test-utils"))]
pub fn run_two_store_test<F>(schema: crate::Schema, test: F)
where
    F: AsyncFnOnce(&TwoStores) + Send + 'static,
{
    on_device_runtime(async move || {
        let stores = TwoStores::new(schema).await;
        test(&stores).await;
        for store in [&stores.a, &stores.b] {
            store.database.close().await.expect("close the device's store");
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
impl TwoStores {
    async fn new(schema: crate::Schema) -> Self {
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let storage = Arc::new(coven::MemoryStorage::builder().clock(clock).build().unwrap());
        let dir = tempfile::TempDir::new().unwrap();
        let installation = coven::Coven::in_memory(dir.path().to_path_buf());
        let handle = create_first_store(schema, &installation, storage.clone()).await;
        let a = StoreDevice::new(handle, installation, dir);
        let code = a.database.generate_restore_code().await.unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let installation = coven::Coven::in_memory(dir.path().to_path_buf());
        let handle =
            restore_store(schema, &installation, Arc::new(storage.for_device()), &code, "Device B")
                .await;
        let b = StoreDevice::new(handle, installation, dir);
        b.database.start_sync().await.unwrap();
        Self { a, b }
    }

    /// The device that created the library.
    pub fn a(&self) -> &Database {
        &self.a.database
    }

    /// The device restored from the first one's code.
    pub fn b(&self) -> &Database {
        &self.b.database
    }

    /// Stop both sync loops, before making concurrent offline edits.
    pub async fn pause(&self) {
        for store in [&self.a, &self.b] {
            stop_sync(&store.database).await;
        }
    }

    pub async fn resume(&self) {
        for store in [&self.a, &self.b] {
            store.database.start_sync().await.unwrap();
        }
    }

    /// Run sync on both until they answer `query` with the same rows.
    pub async fn converge(&self, query: &str) {
        converge((&self.a.database, &self.a.cycles), (&self.b.database, &self.b.cycles), query)
            .await;
    }

    /// Run sync on both until `settled` holds on each, named by its letter,
    /// and neither holds anything back.
    pub async fn settle(&self, what: &str, settled: impl AsyncFn(&'static str, &Database) -> bool) {
        settle(
            (&self.a.database, &self.a.cycles),
            (&self.b.database, &self.b.cycles),
            what,
            || async {
                settled("A", &self.a.database).await && settled("B", &self.b.database).await
            },
        )
        .await;
    }
}
