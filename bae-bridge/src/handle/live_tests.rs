use super::*;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

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
    let opened = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(host.create_library(
            Some("Subscribed Library".into()),
            crate::BridgeLibrarySetup::new(crate::BridgeDiagnostics::noop()),
        ))
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

/// A delivery that waits until it is stopped, with the receivers that say
/// when it started and when it ended.
fn waiting_delivery(
    app: &AppHandle,
) -> (
    Arc<crate::LiveSubscription>,
    mpsc::Receiver<()>,
    mpsc::Receiver<()>,
) {
    let (started_tx, started) = mpsc::channel();
    let (ended_tx, ended) = mpsc::channel();
    let subscription = app.live_subscription(move |_| async move {
        let _ended = DropSignal(ended_tx);
        started_tx.send(()).expect("the test waits for the start");
        std::future::pending::<()>().await;
    });
    (subscription, started, ended)
}

#[test]
fn cancelling_a_subscription_ends_its_delivery() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    let (subscription, started, ended) = waiting_delivery(&app);
    started
        .recv_timeout(Duration::from_secs(10))
        .expect("the delivery starts");

    subscription.cancel();

    ended
        .recv_timeout(Duration::from_secs(10))
        .expect("cancelling ends the delivery");
    app.close_library().unwrap();
}

#[test]
fn dropping_a_subscription_ends_its_delivery() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    let (subscription, started, ended) = waiting_delivery(&app);
    started
        .recv_timeout(Duration::from_secs(10))
        .expect("the delivery starts");

    drop(subscription);

    ended
        .recv_timeout(Duration::from_secs(10))
        .expect("dropping ends the delivery");
    app.close_library().unwrap();
}

#[test]
fn a_subscription_opened_on_a_closed_library_never_runs() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    app.close_library().unwrap();

    let (_subscription, started, _ended) = waiting_delivery(&app);

    assert_eq!(
        started.recv(),
        Err(mpsc::RecvError),
        "the delivery is dropped without starting"
    );
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
    caller
        .block_on(tokio::time::timeout(
            Duration::from_secs(20),
            members.clone().next(),
        ))
        .expect("the first pull answers")
        .expect("the subscription reads the library's members");

    let (answered_tx, answered) = mpsc::channel();
    let waiting = members.clone();
    std::thread::spawn(move || {
        let answer = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(waiting.next());
        answered_tx
            .send(answer.map(|_| ()))
            .expect("the test waits for the answer");
    });
    let (closed_tx, closed) = mpsc::channel();
    std::thread::spawn(move || {
        closed_tx
            .send(app.close_library())
            .expect("the test waits for the close");
    });
    closed
        .recv_timeout(Duration::from_secs(30))
        .expect("closing the library finishes")
        .expect("the library closes");
    let answer = answered
        .recv_timeout(Duration::from_secs(30))
        .expect("the waiting pull answers");
    assert!(
        matches!(answer, Err(BridgeError::Cancelled)),
        "the waiting pull answers that it was cancelled: {answer:?}"
    );
}

/// Sends the operation of each bug reported, for a test to wait on.
struct HeardBugs(Mutex<mpsc::Sender<String>>);

impl bae_core::diagnostics::BugReporter for HeardBugs {
    fn report(&self, operation: &str, _detail: &str) {
        self.0
            .lock()
            .unwrap()
            .send(operation.to_string())
            .expect("the test hears the report");
    }
}

/// A delivery that panics — as a host callback that throws does, through
/// uniffi — ends with its panic reported, and the library's other
/// subscriptions go on.
#[test]
fn a_delivery_that_panics_is_reported_and_others_go_on() {
    let root = tempfile::tempdir().unwrap();
    let (reported, reports) = mpsc::channel();
    let app = open_library_reporting(
        &root,
        crate::BridgeDiagnostics::reporting_bugs_to(Arc::new(HeardBugs(Mutex::new(reported)))),
    );

    let _panicked = app.live_subscription(|_| async {
        panic!("Callback interface failure: the host threw");
    });
    assert_eq!(
        reports
            .recv_timeout(Duration::from_secs(10))
            .expect("the panic is reported"),
        "deliver a live subscription"
    );

    let (_subscription, started, _ended) = waiting_delivery(&app);
    started
        .recv_timeout(Duration::from_secs(10))
        .expect("another delivery still starts");
    app.close_library().unwrap();
    assert!(reports.try_recv().is_err(), "the panic is reported once");
}
