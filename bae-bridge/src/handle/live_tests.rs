use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// A library created in `root` and opened, as a host opens one.
fn open_library(root: &tempfile::TempDir) -> Arc<AppHandle> {
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let opened = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(host.create_library(Some("Subscribed Library".into())))
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
    let subscription = app.live_subscription(move |_, _| async move {
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

/// Records a members subscription's deliveries: the first value, and how many
/// errors arrived.
struct MembersDeliveries {
    first_value: Mutex<Option<mpsc::Sender<()>>>,
    errors: Arc<AtomicUsize>,
}

impl crate::handle::MembershipCallback for MembersDeliveries {
    fn on_value(&self, _value: crate::types::BridgeMembership) {
        if let Some(first) = self.first_value.lock().unwrap().take() {
            first.send(()).expect("the test waits for the first value");
        }
    }

    fn on_error(&self, _error: BridgeError) {
        self.errors.fetch_add(1, Ordering::SeqCst);
    }
}

/// A library closed while the host still holds one of its live subscriptions
/// closes: the subscription ends with the library's other work, before the
/// store closes, so it never answers from a closed store.
#[test]
fn closing_a_library_ends_its_live_subscriptions_before_its_store_closes() {
    let root = tempfile::tempdir().unwrap();
    let app = open_library(&root);
    let (first_value, delivered) = mpsc::channel();
    let errors = Arc::new(AtomicUsize::new(0));
    let members = app.subscribe_members(Box::new(MembersDeliveries {
        first_value: Mutex::new(Some(first_value)),
        errors: errors.clone(),
    }));
    delivered
        .recv_timeout(Duration::from_secs(20))
        .expect("the subscription delivers the library's members");

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
    assert_eq!(
        errors.load(Ordering::SeqCst),
        0,
        "the subscription heard nothing from the closed store"
    );
    drop(members);
}
