//! How a released handle's library close ends when it panics.

use std::sync::{mpsc, Arc, Mutex};

use super::{Session, SessionState};

/// A session open on a runtime of its own.
fn open_session() -> Session {
    Session(Mutex::new(SessionState::Open(
        tokio::runtime::Runtime::new().expect("build the session's runtime"),
    )))
}

/// A close that breaks its guarantee partway.
async fn panicking_close() -> super::Closing {
    if true {
        panic!("the close broke its guarantee");
    }
    Ok(())
}

/// What a session's `failed` was told, in order.
type Told = Arc<Mutex<Vec<String>>>;

/// A `failed` that records what it is told, and the record.
fn told() -> (impl Fn(&dyn std::fmt::Display) + Send + Sync + 'static, Told) {
    let heard = Told::default();
    let telling = heard.clone();
    (move |error: &dyn std::fmt::Display| telling.lock().unwrap().push(error.to_string()), heard)
}

/// Released on a runtime thread, the close runs on a thread of its own; its
/// panic is caught there and told to `failed`, so the open that later waits
/// for that thread is not failed by it.
#[test]
fn a_released_close_that_panics_on_its_thread_is_told_not_raised_into_the_next_open() {
    let session = open_session();
    let (failed, heard) = told();
    let (thread_tx, thread_rx) = mpsc::channel();
    let caller = tokio::runtime::Runtime::new().expect("build the caller's runtime");

    caller.block_on(async {
        session.end_released(panicking_close(), failed, |thread| {
            thread_tx.send(thread).expect("the test waits for the thread");
        });
    });

    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let store = coven::StoreId("00000000-0000-0000-0000-000000000001".parse().expect("a store id"));
    host.library_closing(store, thread_rx.recv().expect("the close ran on a thread"));
    host.wait_for_closing(store);

    let heard = heard.lock().unwrap().clone();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert!(heard[0].contains("the close broke its guarantee"), "{heard:?}");
}

/// Released off any runtime, the close runs inside the handle's `Drop`; its
/// panic is caught there and told to `failed`, never unwinding out of the
/// `Drop`.
#[test]
fn a_released_close_that_panics_inline_is_told_not_unwound() {
    let session = open_session();
    let (failed, heard) = told();

    session.end_released(panicking_close(), failed, |_| {
        panic!("a close off any runtime runs inline, on no thread of its own")
    });

    let heard = heard.lock().unwrap().clone();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert!(heard[0].contains("the close broke its guarantee"), "{heard:?}");
}
