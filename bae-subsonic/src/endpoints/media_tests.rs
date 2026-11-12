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

/// A `maxBitRate` is a limit, so one past a `u32` asks for the most instead of
/// wrapping to a small bitrate (4294967424 truncates to 128).
#[test]
fn a_max_bit_rate_past_u32_asks_for_the_most() {
    assert_eq!(requested_kbps(None), 128);
    assert_eq!(requested_kbps(Some(0)), 128);
    assert_eq!(requested_kbps(Some(192)), 192);
    assert_eq!(requested_kbps(Some(4_294_967_424)), u32::MAX);
    assert_eq!(requested_kbps(Some(i64::MAX)), u32::MAX);
    assert_eq!(
        requested_kbps(Some(4_294_967_424)).clamp(*MP3_STREAM_KBPS.start(), *MP3_STREAM_KBPS.end()),
        320
    );
}
