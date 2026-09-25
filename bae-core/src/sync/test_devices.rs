//! Two devices of one library, syncing through one in-memory cloud home.
//!
//! The first device opens the library with its keys held in memory, connects
//! it to the home, and admits the second through the production pairing flow;
//! the second joins over the same home and opens the store the join installed.
//! Each device is a whole `LibraryManager`, so a test drives the same writers
//! the app does and reads back what sync left on each side.

use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;

use crate::config::{AppDir, Config, ConfigHandle};
use crate::db::Database;
use crate::library::LibraryManager;

/// The store key both devices seal the home's objects with.
const STORE_KEY: [u8; 32] = [23; 32];

/// How long a test waits for sync to reach a state before failing.
const SETTLE: Duration = Duration::from_secs(20);

pub(crate) struct TestDevice {
    manager: LibraryManager,
    database: Database,
    cycles: Arc<std::sync::Mutex<Cycles>>,
    _watcher: tokio::task::JoinHandle<()>,
    _dir: TempDir,
}

impl TestDevice {
    pub(crate) fn manager(&self) -> &LibraryManager {
        &self.manager
    }

    pub(crate) fn database(&self) -> &Database {
        &self.database
    }
}

pub(crate) struct TwoDevices {
    a: TestDevice,
    b: TestDevice,
    home: Arc<coven::InMemoryCloudHome>,
}

/// Run `test` on a runtime with a deep stack: pairing and snapshot installation
/// recurse further than a default test thread allows.
pub(crate) fn run_two_device_test<F, Fut>(test: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()>,
{
    std::thread::Builder::new()
        .name("two-device-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("build the two-device runtime")
                .block_on(test())
        })
        .expect("spawn the two-device test thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

impl TwoDevices {
    pub(crate) fn a(&self) -> &TestDevice {
        &self.a
    }

    pub(crate) fn b(&self) -> &TestDevice {
        &self.b
    }
}

impl TwoDevices {
    /// Open device A, connect it to a fresh home, and pair device B onto the
    /// same library. Both sync loops are running when this returns.
    pub(crate) async fn pair() -> Self {
        crate::config::install_test_keyring();
        let home = Arc::new(coven::InMemoryCloudHome::new());
        let store_id = uuid::Uuid::new_v4().to_string();
        let encryption = coven::EncryptionService::from_key(STORE_KEY);

        // Device A holds its store key and device identity in memory: the
        // process-wide test keyring names both by store id, and device B —
        // same store id — keeps its own there once its join installs them.
        let dir_a = TempDir::new().expect("device A directory");
        let app_a = AppDir::at(dir_a.path());
        let store_dir_a = coven::StoreDir::new(app_a.registered_library(&store_id));
        let config_a = Config::with_defaults(
            store_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            &store_dir_a,
            "Two Devices".to_string(),
        );
        let handle_a = coven::Coven::builder(store_dir_a, config_a.to_coven())
            .synced_tables(crate::sync::synced_tables())
            .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
            .migrations(crate::migrations::all())
            .key_custody(coven::KeyCustody::InMemory(coven::MasterKeyring::from(
                encryption,
            )))
            .identity_custody(coven::IdentityCustody::InMemory(
                coven::UserKeypair::generate(),
            ))
            .open()
            .expect("open device A");
        let a = device(dir_a, app_a, config_a, handle_a);
        connect_a(a.database(), &home).await;
        wait_for_cycle(a.database()).await;

        let dir_b = TempDir::new().expect("device B directory");
        let app_b = AppDir::at(dir_b.path());
        let layout_b = app_b.store_layout();
        let joined = join(a.database(), &layout_b, home.clone()).await;
        let store_dir_b = layout_b.store_dir(&joined.store_id);
        let config_b = Config::from_coven(joined.clone(), store_dir_b.to_path_buf());
        let handle_b = coven::Coven::builder(store_dir_b, joined)
            .synced_tables(crate::sync::synced_tables())
            .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
            .migrations(crate::migrations::all())
            .open()
            .expect("open device B's joined store");
        let b = device(dir_b, app_b, config_b, handle_b);
        connect_b(b.database(), &home).await;
        wait_for_cycle(b.database()).await;
        Self { a, b, home }
    }

    /// Stop both sync loops: whatever each device writes next, the other does
    /// not see until [`Self::resume`].
    pub(crate) fn pause(&self) {
        self.a.database().stop_sync_for_test();
        self.b.database().stop_sync_for_test();
    }

    /// Reconnect both devices to the home, starting their loops again. A
    /// restart would rebuild each home from its configured provider, which a
    /// test home has none of.
    pub(crate) async fn resume(&self) {
        connect_a(self.a.database(), &self.home).await;
        connect_b(self.b.database(), &self.home).await;
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
            settled("A", self.a.database().clone()).await
                && settled("B", self.b.database().clone()).await
        })
        .await;
    }

    /// Run cycles until both devices answer `query` with the same rows and
    /// neither holds anything back: each has pulled what the other wrote.
    pub(crate) async fn converge(&self, query: &str) {
        self.settle_on(&format!("the rows of {query:?}"), || async {
            sorted_rows(self.a.database(), query).await
                == sorted_rows(self.b.database(), query).await
        })
        .await;
    }

    /// Wait for `condition`, then for two more completed cycles on each
    /// device — so the verdict is a cycle that began after the condition held
    /// — and require the condition still holds and nothing is held back.
    async fn settle_on<F, Fut>(&self, what: &str, condition: F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let deadline = tokio::time::Instant::now() + SETTLE;
        let mut settling: Option<(u64, u64)> = None;
        loop {
            self.a.database().sync_now();
            self.b.database().sync_now();
            let (completed_a, completed_b) = (self.a.completed(), self.b.completed());
            if !condition().await {
                settling = None;
            } else {
                match settling {
                    None => settling = Some((completed_a, completed_b)),
                    Some((from_a, from_b))
                        if completed_a >= from_a + 2 && completed_b >= from_b + 2 =>
                    {
                        if self.a.held().is_empty() && self.b.held().is_empty() {
                            return;
                        }
                    }
                    Some(_) => {}
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!(
                    "{what} never settled.\nA held {:?} after {}\nB held {:?} after {}",
                    self.a.held(),
                    self.a.last(),
                    self.b.held(),
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
    held: Vec<coven::HeldStorePosition>,
    last: String,
}

impl TestDevice {
    /// The positions this device's latest completed cycle held back from its
    /// library, each with the reason it cannot apply.
    pub(crate) fn held(&self) -> Vec<coven::HeldStorePosition> {
        self.cycles.lock().expect("cycle record").held.clone()
    }

    fn completed(&self) -> u64 {
        self.cycles.lock().expect("cycle record").completed
    }

    fn last(&self) -> String {
        self.cycles.lock().expect("cycle record").last.clone()
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
            let (held, last) = match &current {
                coven::SyncLoopStatus::Synchronized(success)
                | coven::SyncLoopStatus::Blocked { success, .. } => (
                    success.alerts.held_positions.clone(),
                    format!("{:?}", success.alerts),
                ),
                coven::SyncLoopStatus::Failed { error } => (Vec::new(), format!("failed: {error}")),
                _ => continue,
            };
            let mut cycles = recorded.lock().expect("cycle record");
            cycles.completed += 1;
            cycles.held = held;
            cycles.last = last;
        }
    });
    (cycles, watcher)
}

async fn connect_a(database: &Database, home: &Arc<coven::InMemoryCloudHome>) {
    database
        .connect_sync_with_test_home(
            home.clone(),
            coven::CloudCipher::Encrypted(coven::EncryptionService::from_key(STORE_KEY)),
        )
        .await
        .expect("connect device A to the home");
}

async fn connect_b(database: &Database, home: &Arc<coven::InMemoryCloudHome>) {
    database
        .connect_sync_with_test_home_custody_for_test(home.clone())
        .await
        .expect("connect device B to the home");
}

fn device(dir: TempDir, app_dir: AppDir, config: Config, handle: coven::CovenHandle) -> TestDevice {
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let ids: coven::IdRef = Arc::new(coven::UuidProvider);
    let database = Database::from_handle(handle, clock.clone());
    let manager = LibraryManager::new(
        database.clone(),
        app_dir,
        Arc::new(ConfigHandle::new(config)),
        clock,
        ids,
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );
    let (cycles, watcher) = record_cycles(&database);
    TestDevice {
        manager,
        database,
        cycles,
        _watcher: watcher,
        _dir: dir,
    }
}

/// Wait for one completed cycle, so the library's Store and the snapshot a
/// joining device bootstraps from exist.
async fn wait_for_cycle(database: &Database) {
    let mut status = database.subscribe_sync_status();
    database.sync_now();
    tokio::time::timeout(SETTLE, async {
        loop {
            match &*status.borrow_and_update() {
                coven::SyncLoopStatus::Synchronized(_) => return,
                coven::SyncLoopStatus::Failed { error } => panic!("sync cycle failed: {error}"),
                _ => {}
            }
            status.changed().await.expect("sync status stays open");
        }
    })
    .await
    .expect("a sync cycle completes");
}

/// Admit a new device through pairing: `owner` hosts the pairing session and
/// approves the request, the joining side installs the library under `layout`.
async fn join(
    owner: &Database,
    layout: &coven::StoreLayout,
    home: Arc<coven::InMemoryCloudHome>,
) -> coven::Config {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the pairing listener");
    let endpoint = listener.local_addr().expect("pairing endpoint");
    let pairing_key = coven::UserKeypair::generate();
    let offer = coven::DevicePairingOffer::new(
        &pairing_key,
        vec![endpoint],
        "Two Devices".to_string(),
        coven::CloudProvider::S3,
        1_900_000_000,
    )
    .expect("pairing offer");
    let journal = TempDir::new().expect("pairing journal directory");
    let host = coven::DevicePairingHost::start(
        listener,
        offer.clone(),
        pairing_key,
        journal.path().join("pairing.json"),
        Arc::new(coven::SystemClock),
    )
    .await
    .expect("start the pairing host");
    let pairing = coven::PreparedDevicePairing::open_or_create(&offer.encode(), None, layout)
        .expect("prepare the joining identity");
    // Coven reads a closed cancellation channel as a failed pairing, so both
    // senders live as long as the join does.
    let (_cancel_join, cancel) = tokio::sync::watch::channel(false);
    let (_cancel_approval, cancel_approval) = tokio::sync::watch::channel(false);
    let joining = coven_domain::joining::join_with_device_pairing_over_test_home(
        &pairing,
        layout.clone(),
        crate::sync::synced_tables(),
        crate::migrations::all(),
        coven::CovenMigrationPolicy::ApplyPending,
        Arc::new(coven::SystemClock),
        home,
        coven::DeviceJoinTransportTiming {
            poll: Duration::from_millis(2),
            deadline: SETTLE,
        },
        Arc::new(|_| {}),
        &cancel,
    );
    let admitting = async {
        let request = host
            .wait_for_request()
            .await
            .expect("receive the joining device's request");
        owner
            .approve_device_pairing(&host, &request, &|_| {}, cancel_approval)
            .await
    };
    let (joined, admitted) = tokio::join!(Box::pin(joining), Box::pin(admitting));
    let joined =
        joined.unwrap_or_else(|error| panic!("device B joins: {error:?} (A: {admitted:?})"));
    assert!(matches!(
        admitted.expect("device A admits device B"),
        coven::DeviceJoinDriveOutcome::Activated(_)
    ));
    match joined {
        coven::DeviceJoinTransportOutcome::Joined(config) => config,
        outcome => panic!("device B did not install the library: {outcome:?}"),
    }
}
