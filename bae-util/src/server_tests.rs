use std::future::Future;

use super::*;

/// A serve future that panics is reported to the owner's bug sink and leaves
/// the status `Error`, never `Running` with nothing serving.
#[tokio::test]
async fn a_serve_future_that_panics_is_reported_and_leaves_the_server_in_error() {
    let bugs = coven::task::HeardBugs::shared();
    let controller = ServerController::<u16>::new("Test", bugs.clone());
    controller
        .start(1, "http://test".to_string(), |_| async move {
            if true {
                panic!("the server broke its guarantee");
            }
            Ok::<(), std::io::Error>(())
        })
        .await;
    // The serve task moves the state when its future ends.
    let status = loop {
        let ended = controller.ended.notified();
        tokio::pin!(ended);
        ended.as_mut().enable();
        match controller.status().await {
            ServerStatus::Running { .. } => ended.await,
            status => break status,
        }
    };

    let ServerStatus::Error { error } = status else {
        panic!("a panicked server is in error, got {status:?}");
    };
    assert!(matches!(&error, ServerError::ServerFailed { .. }), "{error:?}");
    assert!(error.detail().contains("the server broke its guarantee"), "{error:?}");
    let reported = bugs.take();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].operation, "Test");
}

/// `start` takes the controller's lock before it spawns anything and holds it
/// until `Running` is stored, so a serve future that fails at once records its
/// stop after `Running` and the controller ends in `Error`, not `Running`.
#[tokio::test]
async fn a_serve_future_that_fails_at_once_leaves_the_server_in_error() {
    let controller = ServerController::<u16>::new("Test", coven::task::HeardBugs::shared());

    // Hold the lock: a `start` that spawns before taking it would get to its
    // serve future now.
    let held = controller.inner.lock().await;
    let built = std::sync::atomic::AtomicBool::new(false);
    let mut starting = std::pin::pin!(controller.start(1, "http://test".to_string(), |_| {
        built.store(true, std::sync::atomic::Ordering::SeqCst);
        async { Err::<(), _>(std::io::Error::other("bind lost")) }
    }));
    let first_poll =
        std::future::poll_fn(|cx| std::task::Poll::Ready(starting.as_mut().poll(cx))).await;
    assert!(first_poll.is_pending());
    assert!(
        !built.load(std::sync::atomic::Ordering::SeqCst),
        "start built its serve future before it held the lock"
    );
    drop(held);

    starting.await;
    let status = loop {
        let ended = controller.ended.notified();
        tokio::pin!(ended);
        ended.as_mut().enable();
        match controller.status().await {
            ServerStatus::Running { .. } => ended.await,
            status => break status,
        }
    };

    let ServerStatus::Error { error } = status else {
        panic!("a server that failed at once is in error, got {status:?}");
    };
    assert!(error.detail().contains("bind lost"), "{error:?}");
}
