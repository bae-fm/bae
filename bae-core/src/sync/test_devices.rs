//! Two isolated installations of one library, restored over shared memory storage.
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
}

/// Run `test` on a runtime with a deep stack: restoration and snapshot installation
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
    pub(crate) async fn new() -> Self {
        let clock: coven::ClockRef = Arc::new(coven::SystemClock);
        let storage = Arc::new(
            coven::MemoryStorage::builder()
                .clock(clock)
                .build()
                .unwrap(),
        );
        let dir_a = TempDir::new().unwrap();
        let app_a = AppDir::at(dir_a.path()).unwrap();
        let installation_a = coven::TestCoven::new();
        let handle_a =
            crate::library::configure_store(installation_a.builder(app_a.store_layout()))
                .storage_connector(storage.clone())
                .create_store("Two Devices")
                .await
                .unwrap();
        handle_a
            .setup_s3_storage(
                storage.config(),
                "Device A",
                "owner-key".into(),
                coven::SecretText::new("owner-secret".into()),
            )
            .await
            .unwrap();
        let mut status = handle_a.subscribe_sync_status();
        tokio::time::timeout(
            SETTLE,
            status.wait_for(|status| matches!(status, coven::SyncStatus::Synced { .. })),
        )
        .await
        .unwrap()
        .unwrap();
        let code = handle_a.restore_code().await.unwrap();
        let config_a = Config::with_defaults(
            handle_a.store_id(),
            app_a.registered_library(&handle_a.store_id().to_string()),
        );
        let a = device(dir_a, app_a, config_a, handle_a);

        let dir_b = TempDir::new().unwrap();
        let app_b = AppDir::at(dir_b.path()).unwrap();
        let installation_b = coven::TestCoven::new();
        let builder = crate::library::configure_store(installation_b.builder(app_b.store_layout()))
            .storage_connector(Arc::new(storage.for_device()));
        let (_sender, cancel) = tokio::sync::watch::channel(false);
        let handle_b = coven::restore_from_code(
            builder,
            &code,
            Some(coven::S3Credentials {
                access_key_id: "owner-key".into(),
                secret_access_key: coven::SecretText::new("owner-secret".into()),
            }),
            "Device B",
            |_| {},
            &cancel,
        )
        .await
        .unwrap();
        let config_b = Config::with_defaults(
            handle_b.store_id(),
            app_b.registered_library(&handle_b.store_id().to_string()),
        );
        let b = device(dir_b, app_b, config_b, handle_b);
        b.database.start_sync().await.unwrap();
        Self { a, b }
    }

    /// Wait until both loops have stopped before making concurrent offline edits.
    pub(crate) async fn pause(&self) {
        for device in [&self.a, &self.b] {
            device.database.stop_sync();
            let mut status = device.database.subscribe_sync_status();
            tokio::time::timeout(
                SETTLE,
                status.wait_for(|status| matches!(status, coven::SyncStatus::Stopped)),
            )
            .await
            .unwrap()
            .unwrap();
        }
    }

    pub(crate) async fn resume(&self) {
        for device in [&self.a, &self.b] {
            device.database.start_sync().await.unwrap();
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
            self.a.database().sync_now();
            self.b.database().sync_now();
            let quiet = self.a.database.unsent_writes_for_test().await.unwrap() == 0
                && self.b.database.unsent_writes_for_test().await.unwrap() == 0
                && condition().await;
            let (completed_a, completed_b) = (self.a.completed(), self.b.completed());
            if !quiet {
                settling = None;
            } else {
                match settling {
                    None => settling = Some((completed_a, completed_b)),
                    Some((from_a, from_b))
                        if completed_a >= from_a + 2 && completed_b >= from_b + 2 =>
                    {
                        if self.a.database.pending().await.unwrap().is_empty()
                            && self.b.database.pending().await.unwrap().is_empty()
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
                    self.a.database.pending().await.unwrap(),
                    self.a.database.unsent_writes_for_test().await.unwrap(),
                    self.a.last(),
                    self.b.database.pending().await.unwrap(),
                    self.b.database.unsent_writes_for_test().await.unwrap(),
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

fn device(dir: TempDir, app_dir: AppDir, config: Config, handle: coven::CovenHandle) -> TestDevice {
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
        crate::remote_image::RemoteImageCache::for_test(bae_util::http::Http::for_test()),
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
