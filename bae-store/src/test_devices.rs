//! Isolated installations of one person's library over shared memory
//! storage, at the store level: making the first device's store, restoring
//! another of the person's devices from its code, and running two devices'
//! sync until they agree. Nothing a crate above uses holds a device's store;
//! the harness a test runs on holds each device and passes its parts in.

use std::sync::Arc;

use bae_util::test_channels::HANG_GUARD;
use coven::Storage;

use crate::Store;

/// A builder over `installation` on the system clock and the ids it times,
/// for a test that opens no library services over what it builds.
pub fn test_builder(installation: &coven::Coven) -> coven::CovenBuilder {
    crate::TestSources::system().builder(installation)
}

/// The S3 key the library's storage is reached with on every device.
pub fn owner_credentials() -> coven::S3Credentials {
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
    sources: &crate::TestSources,
    storage: Arc<coven::MemoryStorage>,
) -> coven::CovenHandle {
    let handle = crate::configure_store(sources.builder(installation), schema)
        .storage_connector(storage.clone())
        .create_store("Two Devices", "Device A", &[], |_, _| Ok(()))
        .await
        .unwrap();
    let credentials = owner_credentials();
    handle
        .setup_s3_storage(
            storage.config(),
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
    sources: &crate::TestSources,
    storage: Arc<coven::MemoryStorage>,
    code: &str,
    name: &str,
) -> coven::CovenHandle {
    let builder =
        crate::configure_store(sources.builder(installation), schema).storage_connector(storage);
    let (_sender, cancel) = tokio::sync::watch::channel(false);
    coven::restore_from_code(builder, code, Some(owner_credentials()), name, |_| {}, &cancel)
        .await
        .unwrap()
}

/// Pair another of the person's devices into `installation` with `code`, a
/// pairing code the first device showed, on `schema` over `storage`, with
/// `credentials` when the code carries no key. It returns once the showing
/// device has approved the request, or `None` when the pairing ended first.
pub async fn pair_store(
    schema: crate::Schema,
    installation: &coven::Coven,
    sources: &crate::TestSources,
    storage: Arc<coven::MemoryStorage>,
    code: &str,
    credentials: Option<coven::S3Credentials>,
    name: &str,
) -> Option<coven::CovenHandle> {
    let builder =
        crate::configure_store(sources.builder(installation), schema).storage_connector(storage);
    coven::join_with_pairing(builder, code, credentials, name, |_| {}, &coven::JoinControl::new())
        .await
        .unwrap()
}

/// Open the device's store `store` again with `migrations` in place of
/// `schema`'s own, as an update of the app that brings a new schema does.
pub async fn reopen_store(
    schema: crate::Schema,
    installation: &coven::Coven,
    sources: &crate::TestSources,
    storage: Arc<coven::MemoryStorage>,
    store: coven::StoreId,
    migrations: Vec<coven::Migration>,
) -> coven::CovenHandle {
    crate::configure_store(sources.builder(installation), schema)
        .migrations(migrations)
        .storage_connector(storage)
        .open(store, |_| ())
        .await
        .expect("open the device's store again")
}

/// The store over `handle`, stamping its writes by `clock`, for a test that
/// opens no library services over it: it carries no [`crate::TestSources`]
/// (use [`crate::TestSources::store`] for one a library opens over). A bug
/// one of its calls carries back fails the test as the store drops.
pub fn store_under(handle: coven::CovenHandle, clock: coven::ClockRef) -> Store {
    Store::new(handle, clock, coven::task::HeardBugs::shared())
}

/// How `store`'s last pass ended and when a pass last reached the end
/// of the sequence, as coven's sync status row keeps them, live: a pass
/// that ends synced moves the time, and one that fails changes how it
/// ended.
fn pass_ends(
    store: &Store,
) -> coven::LiveQuery<(Option<coven::LastPass>, Option<i64>), crate::DbError> {
    store.subscribe(|sql| {
        Ok(sql.query_row("SELECT last_pass, last_synced FROM coven_sync_status", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?)
    })
}

/// Stop the device's sync loop: the pass running ends first, and once this
/// returns no pass or upload runs until sync starts again.
pub async fn stop_sync(store: &Store) {
    store.stop_sync().await.unwrap();
}

/// What the latest sync cycle a device's store completed reported, for as
/// long as the record lives: what a test that never got where it waited for
/// names.
pub struct SyncCycles {
    last: Arc<std::sync::Mutex<String>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl SyncCycles {
    /// Record the cycles `store` completes from now on: the last pass as
    /// it stands is no cycle.
    pub fn follow(store: &Store) -> Self {
        let last = Arc::new(std::sync::Mutex::new(String::new()));
        let mut ends = pass_ends(store);
        let recorded = last.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "test support: the watcher ends with its test"
        )]
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
                *recorded.lock().expect("cycle record") = last;
            }
        });
        Self { last, _watcher: watcher }
    }

    /// What the latest completed cycle reported.
    pub fn last(&self) -> String {
        self.last.lock().expect("cycle record").clone()
    }
}

/// Ask `store` for a pass and wait for the pass that answers it: one that
/// began after the asking, so it read what storage held then.
async fn pass(store: &Store) {
    let answer = store.retry_now().await.expect("a device's sync takes a retry");
    // A failed pass is one more cycle; the caller's condition is the verdict.
    answer.await;
}

/// Run `store`'s passes until `condition` holds, or fail naming `what`.
/// It runs passes of this device only, so another device runs only the
/// passes its own work starts.
pub async fn wait_until<F, Fut>(store: &Store, cycles: &SyncCycles, what: &str, condition: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let reached = tokio::time::timeout(HANG_GUARD, async {
        while !condition().await {
            pass(store).await;
        }
    })
    .await;
    if reached.is_err() {
        panic!("{what} never came. The last cycle: {}", cycles.last());
    }
}

/// Wait until `condition` holds and neither device has a write left to
/// send, then for two more passes on each device — so the verdict is a pass
/// that began after every write either device made was in storage — and
/// require all of it still holds and nothing is held back. A device's write
/// leaves its queue only once its slot is in storage, so a condition can
/// hold on one device while the other's write that changes the outcome is
/// still on its way out.
pub async fn settle<F, Fut>(
    (a, a_cycles): (&Store, &SyncCycles),
    (b, b_cycles): (&Store, &SyncCycles),
    what: &str,
    condition: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let quiet = || async {
        a.unsent_writes_for_test().await.unwrap() == 0
            && b.unsent_writes_for_test().await.unwrap() == 0
            && condition().await
    };
    let settled = tokio::time::timeout(HANG_GUARD, async {
        loop {
            if quiet().await {
                for _ in 0..2 {
                    pass(a).await;
                    pass(b).await;
                }
                if quiet().await
                    && a.pending().await.unwrap().is_empty()
                    && b.pending().await.unwrap().is_empty()
                {
                    return;
                }
            } else {
                pass(a).await;
                pass(b).await;
            }
        }
    })
    .await;
    if settled.is_err() {
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
}

/// Run sync on both devices until they answer `query` with the same rows
/// and neither holds anything back: each has pulled what the other wrote.
pub async fn converge(a: (&Store, &SyncCycles), b: (&Store, &SyncCycles), query: &str) {
    settle(a, b, &format!("the rows of {query:?}"), || async {
        sorted_rows(a.0, query).await == sorted_rows(b.0, query).await
    })
    .await;
}

async fn sorted_rows(store: &Store, query: &str) -> Vec<String> {
    let mut rows = store.query_texts_for_test(query).await.expect("read the compared rows");
    rows.sort();
    rows
}

/// Run `body` to its end on its own thread and multi-threaded runtime, both
/// with deep stacks: restoration and snapshot installation recurse further
/// than a default test thread allows.
pub fn on_device_runtime(body: impl AsyncFnOnce() + Send + 'static) {
    #[expect(
        clippy::disallowed_methods,
        reason = "test support: the thread is joined at once and its panic resumed on the test"
    )]
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

/// One device of a store-level test: its store's store, the record of its
/// sync cycles, and what keeps its installation alive.
#[cfg(any(test, feature = "test-utils"))]
struct StoreDevice {
    store: Store,
    cycles: SyncCycles,
    _installation: coven::Coven,
    _dir: tempfile::TempDir,
}

#[cfg(any(test, feature = "test-utils"))]
impl StoreDevice {
    fn new(
        handle: coven::CovenHandle,
        sources: &crate::TestSources,
        installation: coven::Coven,
        dir: tempfile::TempDir,
    ) -> Self {
        let store = sources.store(handle);
        let cycles = SyncCycles::follow(&store);
        Self { store, cycles, _installation: installation, _dir: dir }
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
/// store call cancelled, which coven reports as a panic.
#[cfg(any(test, feature = "test-utils"))]
pub fn run_two_store_test<F>(schema: crate::Schema, test: F)
where
    F: AsyncFnOnce(&TwoStores) + Send + 'static,
{
    on_device_runtime(async move || {
        let stores = TwoStores::new(schema).await;
        test(&stores).await;
        for store in [&stores.a, &stores.b] {
            store.store.close().await.expect("close the device's store");
        }
    });
}

#[cfg(any(test, feature = "test-utils"))]
impl TwoStores {
    async fn new(schema: crate::Schema) -> Self {
        let sources = crate::TestSources::system();
        let storage = Arc::new(
            coven::MemoryStorage::builder().clock(sources.clock().clone()).build().unwrap(),
        );
        let dir = tempfile::TempDir::new().unwrap();
        let installation = coven::Coven::in_memory(dir.path().to_path_buf());
        let handle = create_first_store(schema, &installation, &sources, storage.clone()).await;
        let a = StoreDevice::new(handle, &sources, installation, dir);
        let code = a.store.generate_restore_code().await.unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let installation = coven::Coven::in_memory(dir.path().to_path_buf());
        let handle = restore_store(
            schema,
            &installation,
            &sources,
            Arc::new(storage.for_device()),
            &code,
            "Device B",
        )
        .await;
        let b = StoreDevice::new(handle, &sources, installation, dir);
        b.store.start_sync().await.unwrap();
        Self { a, b }
    }

    /// The device that created the library.
    pub fn a(&self) -> &Store {
        &self.a.store
    }

    /// The device restored from the first one's code.
    pub fn b(&self) -> &Store {
        &self.b.store
    }

    /// Stop both sync loops, before making concurrent offline edits.
    pub async fn pause(&self) {
        for store in [&self.a, &self.b] {
            stop_sync(&store.store).await;
        }
    }

    pub async fn resume(&self) {
        for store in [&self.a, &self.b] {
            store.store.start_sync().await.unwrap();
        }
    }

    /// Run sync on both until they answer `query` with the same rows.
    pub async fn converge(&self, query: &str) {
        converge((&self.a.store, &self.a.cycles), (&self.b.store, &self.b.cycles), query).await;
    }

    /// Run sync on both until `settled` holds on each, named by its letter,
    /// and neither holds anything back.
    pub async fn settle(&self, what: &str, settled: impl AsyncFn(&'static str, &Store) -> bool) {
        settle((&self.a.store, &self.a.cycles), (&self.b.store, &self.b.cycles), what, || async {
            settled("A", &self.a.store).await && settled("B", &self.b.store).await
        })
        .await;
    }
}
