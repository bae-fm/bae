use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Notify};

const DEADLINE: Duration = Duration::from_secs(3);
type Value = (i64, BTreeMap<String, TriageRuntimeFacts>);
type Values = mpsc::UnboundedReceiver<Result<Value, LibraryError>>;

struct SubscriptionLifetime(Option<oneshot::Sender<()>>);
impl Drop for SubscriptionLifetime {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            // A timed-out test may have already dropped its observation.
            let _ = sender.send(());
        }
    }
}

fn open_store() -> (tempfile::TempDir, coven::CovenHandle) {
    crate::config::install_test_keyring();
    let directory = tempfile::tempdir().expect("test store directory");
    let handle = coven::Coven::builder(
        coven::StoreDir::new_ephemeral(directory.path()),
        coven::Config::with_defaults("subscription".to_string(), "device".to_string(), "Test".to_string()),
    )
    .synced_tables(Vec::new())
    .migrations(vec![coven::Migration::sql(1, "subscription_value", "CREATE TABLE subscription_value (id TEXT PRIMARY KEY, value INTEGER NOT NULL) STRICT; INSERT INTO subscription_value VALUES ('selected', 7);")])
    .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
    .clock(Arc::new(coven::FixedClock(chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").expect("test clock").with_timezone(&chrono::Utc))))
    .open().expect("open test store");
    (directory, handle)
}

fn read_value(sql: coven::SqlReadContext<'_>) -> coven::CovenResult<i64> {
    Ok(sql.query_row(
        "SELECT value FROM subscription_value WHERE id = 'selected'",
        [],
        |row| row.get(0),
    )?)
}

fn observe(
    query: coven::LiveQuery<i64>,
    changes: broadcast::Receiver<CandidateRuntimeChange>,
    reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot> + Send + 'static,
) -> (Values, oneshot::Receiver<()>) {
    let (closed, completion) = oneshot::channel();
    let lifetime = SubscriptionLifetime(Some(closed));
    let values = subscribe(
        &tokio::runtime::Handle::current(),
        ["selected".to_string()].into_iter().collect(),
        query,
        HashMap::new(),
        changes,
        reread,
        move |value, facts| {
            assert!(
                lifetime.0.is_some(),
                "the task owns its resolver until it exits"
            );
            (*value, facts.clone())
        },
    );
    (values, completion)
}

#[tokio::test]
async fn closing_an_idle_value_receiver_ends_its_subscription() {
    let (_directory, handle) = open_store();
    let (runtime_changes, changes) = broadcast::channel(8);
    let (mut values, closed) = observe(handle.subscribe(read_value), changes, HashMap::new);
    assert_eq!(
        tokio::time::timeout(DEADLINE, values.recv())
            .await
            .expect("first value")
            .expect("open receiver")
            .expect("read succeeds")
            .0,
        7
    );
    drop(values);
    tokio::time::timeout(DEADLINE, closed)
        .await
        .expect("closing an idle receiver must end the task without another event")
        .expect("subscription exited");
    assert_eq!(runtime_changes.receiver_count(), 0);
}

#[tokio::test]
async fn closing_a_receiver_cancels_its_queued_read_without_waiting_for_a_worker() {
    let (_directory, handle) = open_store();
    let mut releases = Vec::new();
    let mut blocked = Vec::new();
    for _ in 0..4 {
        let started = Arc::new(Notify::new());
        let worker_started = started.clone();
        let (release, wait) = std::sync::mpsc::channel();
        let reader = handle.clone();
        blocked.push(tokio::spawn(async move {
            reader
                .read(move |sql| {
                    let value = read_value(sql)?;
                    worker_started.notify_one();
                    wait.recv_timeout(Duration::from_secs(15))
                        .expect("test releases reader");
                    Ok(value)
                })
                .await
        }));
        tokio::time::timeout(DEADLINE, started.notified())
            .await
            .expect("reader started");
        releases.push(release);
    }
    let ran = Arc::new(AtomicBool::new(false));
    let query_ran = ran.clone();
    let query = handle.subscribe(move |sql| {
        query_ran.store(true, Ordering::SeqCst);
        read_value(sql)
    });
    let (runtime_changes, changes) = broadcast::channel(1);
    // Lagging the runtime stream makes reread acknowledge that the task polled
    // its pending SQL future, even though no database worker can start it.
    runtime_changes
        .send(CandidateRuntimeChange::Removed {
            key: "first".to_string(),
        })
        .expect("receiver exists");
    runtime_changes
        .send(CandidateRuntimeChange::Removed {
            key: "second".to_string(),
        })
        .expect("receiver exists");
    let polled = Arc::new(Notify::new());
    let reread_polled = polled.clone();
    let (values, closed) = observe(query, changes, move || {
        reread_polled.notify_one();
        HashMap::new()
    });
    tokio::time::timeout(DEADLINE, polled.notified())
        .await
        .expect("subscription polled its queued read");
    drop(values);
    let exited = tokio::time::timeout(DEADLINE, closed).await;
    for release in releases {
        release.send(()).expect("release reader");
    }
    for task in blocked {
        task.await.expect("reader task").expect("reader result");
    }
    handle
        .read(|_| Ok(()))
        .await
        .expect("drain past the cancelled read");
    exited
        .expect("closing a receiver must not wait for a read worker")
        .expect("subscription exited");
    assert_eq!(runtime_changes.receiver_count(), 0);
    assert!(
        !ran.load(Ordering::SeqCst),
        "cancelled queued SQL never executes"
    );
}

#[tokio::test]
async fn closing_a_receiver_during_processing_ends_its_subscription() {
    let (_directory, handle) = open_store();
    let started = Arc::new(Notify::new());
    let process_started = started.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let wait = std::sync::Mutex::new(wait);
    let query = handle.subscribe_processed(read_value, move |value| {
        process_started.notify_one();
        wait.lock()
            .expect("test wait lock")
            .recv_timeout(Duration::from_secs(15))
            .expect("test releases processing");
        Ok(value)
    });
    let (runtime_changes, changes) = broadcast::channel(8);
    let (values, closed) = observe(query, changes, HashMap::new);
    tokio::time::timeout(DEADLINE, started.notified())
        .await
        .expect("processing started");
    drop(values);
    let exited = tokio::time::timeout(DEADLINE, closed).await;
    release.send(()).expect("release processing");
    exited
        .expect("closing a receiver must not wait for processing")
        .expect("subscription exited");
    assert_eq!(runtime_changes.receiver_count(), 0);
}

#[tokio::test]
async fn runtime_updates_preserve_the_pending_read_and_processing_result() {
    let (_directory, handle) = open_store();
    let reads = Arc::new(AtomicUsize::new(0));
    let query_reads = reads.clone();
    let started = Arc::new(Notify::new());
    let process_started = started.clone();
    let (release, wait) = std::sync::mpsc::channel();
    let wait = std::sync::Mutex::new(wait);
    let query = handle.subscribe_processed(
        move |sql| {
            query_reads.fetch_add(1, Ordering::SeqCst);
            read_value(sql)
        },
        move |value| {
            process_started.notify_one();
            wait.lock()
                .expect("test wait lock")
                .recv_timeout(Duration::from_secs(15))
                .expect("test releases processing");
            Ok(value)
        },
    );
    let snapshot = CandidateRuntimeSnapshot {
        identify: Some(crate::import::CandidateIdentifyRuntime::automatic_queue()),
        import: None,
        search: None,
    };
    let reread_snapshot = snapshot.clone();
    let acknowledged = Arc::new(Notify::new());
    let reread_acknowledged = acknowledged.clone();
    let (runtime_changes, changes) = broadcast::channel(1);
    let (mut values, closed) = observe(query, changes, move || {
        reread_acknowledged.notify_one();
        [("selected".to_string(), reread_snapshot.clone())]
            .into_iter()
            .collect()
    });
    tokio::time::timeout(DEADLINE, started.notified())
        .await
        .expect("processing started");
    // Both sends happen without yielding, so the task observes a lag and reads
    // the new runtime while the original processor remains blocked.
    runtime_changes
        .send(CandidateRuntimeChange::Removed {
            key: "unrelated".to_string(),
        })
        .expect("receiver exists");
    runtime_changes
        .send(CandidateRuntimeChange::Updated {
            key: "selected".to_string(),
            runtime: snapshot,
        })
        .expect("receiver exists");
    tokio::time::timeout(DEADLINE, acknowledged.notified())
        .await
        .expect("runtime change consumed during processing");
    release.send(()).expect("release original processing");
    let value = tokio::time::timeout(DEADLINE, values.recv())
        .await
        .expect("original processing delivered")
        .expect("receiver open")
        .expect("read succeeds");
    assert_eq!(value.0, 7);
    assert!(value
        .1
        .get("selected")
        .expect("latest selected runtime")
        .identification
        .is_some());
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "runtime changes must not cancel and repeat database work"
    );
    drop(values);
    tokio::time::timeout(DEADLINE, closed)
        .await
        .expect("subscription closes")
        .expect("task exited");
}
