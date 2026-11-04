use super::*;
use bae_util::test_channels::HANG_GUARD;
use std::sync::{mpsc, Arc, Mutex};

/// A library created in `root` and opened, as a host opens one.
fn open_library(root: &tempfile::TempDir) -> Arc<AppHandle> {
    open_library_reporting(root, crate::BridgeDiagnostics::noop())
}

/// [`open_library`], reporting its bugs to `diagnostics`.
fn open_library_reporting(
    root: &tempfile::TempDir,
    diagnostics: Arc<crate::BridgeDiagnostics>,
) -> Arc<AppHandle> {
    let host = crate::BridgeHost::with_test_keyring(
        diagnostics,
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let setup = crate::BridgeLibrarySetup::new(host.clone());
    let opened = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(host.create_library(Some("Subscribed Library".into()), setup))
        .unwrap();
    opened.open(50, false).unwrap()
}

/// Says it was dropped, so a test knows the delivery holding it ended.
struct DropSignal(mpsc::Sender<()>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.send(()).expect("the test waits for the drop");
    }
}

/// A feed that waits until it is stopped, with the token that stops it and
/// the receivers that say when it started and when it ended.
fn waiting_feed(
    app: &AppHandle,
) -> (bae_library::CancellationToken, mpsc::Receiver<()>, mpsc::Receiver<()>) {
    let (started_tx, started) = mpsc::channel();
    let (ended_tx, ended) = mpsc::channel();
    let stop = bae_library::CancellationToken::new();
    app.deliver_until(stop.clone(), move |_| async move {
        let _ended = DropSignal(ended_tx);
        started_tx.send(()).expect("the test waits for the start");
        std::future::pending::<()>().await;
    });
    (stop, started, ended)
}

#[test]
fn stopping_a_feed_ends_it() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    let (stop, started, ended) = waiting_feed(&app);
    started.recv_timeout(HANG_GUARD).expect("the feed starts");

    stop.cancel();

    ended.recv_timeout(HANG_GUARD).expect("stopping ends the feed");
    app.close_library(None).unwrap();
}

#[test]
fn closing_the_library_ends_a_feed() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    let (_stop, started, ended) = waiting_feed(&app);
    started.recv_timeout(HANG_GUARD).expect("the feed starts");

    app.close_library(None).unwrap();

    ended.recv_timeout(HANG_GUARD).expect("closing the library ends the feed");
}

#[test]
fn a_feed_started_on_a_closed_library_never_runs() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    app.close_library(None).unwrap();

    let (_stop, started, _ended) = waiting_feed(&app);

    assert_eq!(started.recv(), Err(mpsc::RecvError), "the feed is dropped without starting");
}

/// A library closed while the host still pulls one of its subscriptions
/// closes: the pull waiting on it answers that it was cancelled, before the
/// store closes, so it never answers from a closed store.
#[test]
fn closing_a_library_cancels_a_waiting_pull_before_its_store_closes() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    let members = app.subscribe_members();
    let caller = tokio::runtime::Runtime::new().unwrap();
    // The timeout's timer is made on the runtime, inside the future it runs.
    caller
        .block_on(async { tokio::time::timeout(HANG_GUARD, members.clone().next()).await })
        .expect("the first pull answers")
        .expect("the subscription reads the library's members");

    let (answered_tx, answered) = mpsc::channel();
    let waiting = members.clone();
    std::thread::spawn(move || {
        let answer = tokio::runtime::Runtime::new().unwrap().block_on(waiting.next());
        answered_tx.send(answer.map(|_| ())).expect("the test waits for the answer");
    });
    let (closed_tx, closed) = mpsc::channel();
    std::thread::spawn(move || {
        closed_tx.send(app.close_library(None)).expect("the test waits for the close");
    });
    closed
        .recv_timeout(HANG_GUARD)
        .expect("closing the library finishes")
        .expect("the library closes");
    let answer = answered.recv_timeout(HANG_GUARD).expect("the waiting pull answers");
    assert!(
        matches!(answer, Err(BridgeError::Cancelled)),
        "the waiting pull answers that it was cancelled: {answer:?}"
    );
}

/// Sends the operation of each bug reported, for a test to wait on.
struct HeardBugs(Mutex<mpsc::Sender<String>>);

impl bae_diagnostics::BugReporter for HeardBugs {
    fn report(&self, operation: &str, _detail: &str) {
        self.0.lock().unwrap().send(operation.to_string()).expect("the test hears the report");
    }
}

/// A feed that panics — as a host callback that throws does, through
/// uniffi — ends with its panic reported, and the library's other feeds go
/// on.
#[test]
fn a_feed_that_panics_is_reported_and_others_go_on() {
    let root = tempfile::tempdir().unwrap();
    let (reported, reports) = mpsc::channel();
    let app = open_library_reporting(
        &root,
        crate::BridgeDiagnostics::reporting_bugs_to(Arc::new(HeardBugs(Mutex::new(reported)))),
    );

    app.deliver_until(bae_library::CancellationToken::new(), |_| async {
        panic!("Callback interface failure: the host threw");
    });
    assert_eq!(reports.recv_timeout(HANG_GUARD).expect("the panic is reported"), "feed a stream");

    let (_stop, started, _ended) = waiting_feed(&app);
    started.recv_timeout(HANG_GUARD).expect("another feed still starts");
    app.close_library(None).unwrap();
    assert!(reports.try_recv().is_err(), "the panic is reported once");
}
