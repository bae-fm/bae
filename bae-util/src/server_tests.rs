use std::sync::Mutex as StdMutex;

use super::*;

#[derive(Clone, Debug)]
struct Failed(String);

impl ServerError for Failed {
    fn detail(&self) -> &str {
        &self.0
    }
}

/// A serve future that panics is reported through the owner's `bug` and
/// leaves the status `Error`, never `Running` with nothing serving.
#[tokio::test]
async fn a_serve_future_that_panics_is_reported_and_leaves_the_server_in_error() {
    let reported = Arc::new(StdMutex::new(Vec::new()));
    let reporting = reported.clone();
    let controller = ServerController::<u16, Failed>::new(
        "Test",
        Arc::new(move |failed| {
            reporting.lock().unwrap().push(failed.clone());
            failed
        }),
    );
    controller
        .start(1, "http://test".to_string(), Failed, |_| async move {
            if true {
                panic!("the server broke its guarantee");
            }
            Ok::<(), std::io::Error>(())
        })
        .await;
    // The serve task runs once this test yields to it.
    let status = loop {
        match controller.status().await {
            ServerStatus::Running { .. } => tokio::task::yield_now().await,
            status => break status,
        }
    };

    let ServerStatus::Error { error } = status else {
        panic!("a panicked server is in error, got {status:?}");
    };
    assert!(error.0.contains("the server broke its guarantee"), "{error:?}");
    let reported = reported.lock().unwrap().clone();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(reported[0].operation, "Test");
}
