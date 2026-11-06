use super::{LiveRead, LiveRequest};
use crate::live_query::{CancellableLiveQuery, LiveQueryError};
use crate::{LibraryPageWindow, LibraryPageWindows, Schema, Store};
use bae_util::test_channels::HANG_GUARD;
use std::sync::{mpsc, Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// A read the query holds in flight until the test lets it go.
struct Held {
    started: tokio::sync::mpsc::UnboundedReceiver<()>,
    release: mpsc::Sender<()>,
}

/// A query answering each request with the request itself, whose read for
/// `held` waits until [`Held::release`] lets it finish.
fn holding<Request: LiveRequest>(
    store: &Store,
    initial: Request,
    held: Request,
) -> (CancellableLiveQuery<Request, Request>, Held) {
    let (started_tx, started) = tokio::sync::mpsc::unbounded_channel();
    let (release, released) = mpsc::channel();
    let released = Mutex::new(released);
    let query = store.subscribe_reconfigurable(initial, move |request: &Request, _read| {
        if *request == held {
            started_tx.send(()).expect("the test waits for the held read");
            released.lock().unwrap().recv().expect("the test lets the held read go");
        }
        Ok(request.clone())
    });
    (
        CancellableLiveQuery::new(
            query,
            CancellationToken::new(),
            coven::task::HeardBugs::shared(),
        ),
        Held { started, release },
    )
}

/// Set `held`, wait until its read is in flight, set `current`, then let the
/// held read finish: the value the waiting `next` delivers.
async fn replaced_while_reading<Request: LiveRequest + std::fmt::Debug>(
    initial: Request,
    held: Request,
    current: Request,
) -> Request {
    let (store, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let (query, mut hold) = holding(&store, initial.clone(), held.clone());
    let read = Arc::new(LiveRead::new(query, |_, value| Some(value)));
    assert_eq!(read.next().await.unwrap(), initial);
    read.set(held).unwrap();
    let waiting = tokio::spawn({
        let read = read.clone();
        async move { read.next().await }
    });
    tokio::time::timeout(HANG_GUARD, hold.started.recv())
        .await
        .expect("the held read starts")
        .unwrap();

    read.set(current).unwrap();
    hold.release.send(()).unwrap();

    tokio::time::timeout(HANG_GUARD, waiting)
        .await
        .expect("the read delivers")
        .unwrap()
        .expect("the read resolves")
}

/// A read for an id the reader has since moved off is passed over, and the
/// read for the id now set is delivered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_for_a_replaced_id_is_passed_over() {
    let current = Some("current".to_string());
    let delivered = replaced_while_reading(None, Some("held".to_string()), current.clone()).await;
    assert_eq!(delivered, current);
}

/// A read for windows the reader has since scrolled off is still delivered:
/// each window it names holds that window's rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_for_replaced_windows_is_delivered() {
    let windows = |offset| -> LibraryPageWindows {
        [LibraryPageWindow { offset, limit: 10 }].into_iter().collect()
    };
    let delivered =
        replaced_while_reading(LibraryPageWindows::new(), windows(0), windows(10)).await;
    assert_eq!(delivered, windows(0));
}

/// A read whose query ended once its store closed ends with it, and a new
/// request to it reads nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_whose_query_ended_takes_a_new_request_and_reads_nothing() {
    let (store, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let query = store
        .subscribe_reconfigurable(None::<String>, |request: &Option<String>, _read| {
            Ok::<_, crate::DbError>(request.clone())
        });
    let read = LiveRead::new(
        CancellableLiveQuery::new(
            query,
            CancellationToken::new(),
            coven::task::HeardBugs::shared(),
        ),
        |_, value| Some(value),
    );
    store.close().await.unwrap();
    // Reads made before the close may still be delivered.
    tokio::time::timeout(HANG_GUARD, async {
        while !matches!(read.next().await, Err(LiveQueryError::Cancelled)) {}
    })
    .await
    .expect("the read ends with its store");

    read.set(Some("after".to_string())).expect("the read takes the request");

    assert!(
        matches!(read.next().await, Err(LiveQueryError::Cancelled)),
        "the ended read reads nothing"
    );
}
