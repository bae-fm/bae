use super::*;

/// A body whose producer panics partway ends with an error after what it
/// sent, never as if the stream were whole. The panic hook reported the
/// panic, so the body doesn't report it again.
#[tokio::test]
async fn a_body_whose_producer_panics_ends_with_an_error() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(bugs.clone());

    let mut body = produce_body(&diagnostics, "test stream", |tx| {
        tx.blocking_send(Ok(Bytes::from_static(b"first"))).expect("the test reads the body");
        panic!("the producer broke its guarantee");
    });

    assert_eq!(
        body.recv().await.expect("a first chunk").expect("data"),
        Bytes::from_static(b"first")
    );
    let ended = body
        .recv()
        .await
        .expect("the body does not end cleanly")
        .expect_err("the body ends with an error");
    assert!(ended.to_string().contains("the producer broke its guarantee"), "{ended}");
    assert!(body.recv().await.is_none());
    assert_eq!(bugs.reports(), Vec::new());
}
