use super::*;
use crate::test_channels::HANG_GUARD;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

async fn response_from_raw(raw_response: Vec<u8>) -> reqwest::Response {
    let listener =
        tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test listener should bind");
    let url =
        format!("http://{}", listener.local_addr().expect("test listener should have an address"));
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("test request should connect");
        stream.write_all(&raw_response).await.expect("test response should write");
        // Half-close and then drain the client's side to EOF rather than
        // dropping the stream outright. A bare drop with data still in flight
        // makes Windows send an RST, which the client surfaces as a
        // ConnectionAborted request error before it ever reads the response.
        stream.flush().await.ok();
        stream.shutdown().await.ok();
        let mut discard = Vec::new();
        stream.read_to_end(&mut discard).await.ok();
    });
    client_builder()
        .build()
        .expect("test HTTP client should build")
        .get(url)
        .send()
        .await
        .expect("test response should be received")
}

#[tokio::test]
async fn client_builder_times_out_when_response_stalls() {
    let listener =
        tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test listener should bind");
    let url =
        format!("http://{}", listener.local_addr().expect("test listener should have an address"));
    let (accepted_tx, accepted_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("test request should connect");
        accepted_tx.send(()).expect("test should receive accepted signal");
        std::future::pending::<()>().await;
    });
    let client = client_builder()
        .read_timeout(Duration::from_millis(10))
        .build()
        .expect("test HTTP client should build");
    let request_url = url.clone();
    let request = tokio::spawn(async move { client.get(&request_url).send().await });

    accepted_rx.await.expect("test listener should accept the request");
    let result = timeout(HANG_GUARD, request)
        .await
        .expect("request should finish before the outer guard")
        .expect("request task should finish");

    assert!(result.expect_err("stalled response should fail").is_timeout());
}

#[tokio::test]
async fn read_body_capped_rejects_stream_past_limit() {
    let mut raw_response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    raw_response.extend([0xAB; 129]);
    let response = response_from_raw(raw_response).await;

    let error = read_body_capped(response, 128).await.expect_err("over-limit stream should fail");

    assert!(matches!(error, HttpBodyError::TooLarge { limit: 128 }));
}

#[test]
fn a_told_wait_reads_seconds_and_dates() {
    let now = chrono::DateTime::parse_from_rfc2822("Sat, 26 Sep 2026 06:51:45 GMT")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(told_wait_at("120", None, now), Some(Duration::from_secs(120)));
    assert_eq!(told_wait_at(" 0 ", None, now), Some(Duration::ZERO));
    // A date is measured from the response's own clock, not this one.
    assert_eq!(
        told_wait_at("Sat, 26 Sep 2026 07:00:00 GMT", Some("Sat, 26 Sep 2026 06:59:30 GMT"), now,),
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        told_wait_at("Sat, 26 Sep 2026 06:52:00 GMT", None, now),
        Some(Duration::from_secs(15))
    );
    assert_eq!(
        told_wait_at("Sat, 26 Sep 2026 06:00:00 GMT", None, now),
        Some(Duration::ZERO),
        "a date already past asks for no wait"
    );
    assert_eq!(told_wait_at("soon", None, now), None);
    assert_eq!(told_wait_at("-5", None, now), None);
}

#[test]
#[cfg(desktop)]
fn a_wait_until_an_epoch_instant_is_measured_from_the_responses_clock() {
    let now = chrono::DateTime::parse_from_rfc2822("Sat, 26 Sep 2026 06:51:45 GMT")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let epoch = |date: &str| chrono::DateTime::parse_from_rfc2822(date).unwrap().timestamp();
    assert_eq!(
        wait_until_epoch_at(
            epoch("Sat, 26 Sep 2026 07:00:10 GMT"),
            Some("Sat, 26 Sep 2026 07:00:00 GMT"),
            now,
        ),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        wait_until_epoch_at(epoch("Sat, 26 Sep 2026 06:51:47 GMT"), None, now),
        Some(Duration::from_secs(2))
    );
    assert_eq!(
        wait_until_epoch_at(epoch("Sat, 26 Sep 2026 06:00:00 GMT"), None, now),
        Some(Duration::ZERO),
        "an instant already past asks for no wait"
    );

    let mut headers = reqwest::header::HeaderMap::new();
    assert_eq!(wait_until_epoch(&headers, "x-ratelimit-reset", now.into()), None);
    headers.insert("x-ratelimit-reset", reqwest::header::HeaderValue::from_static("soon"));
    assert_eq!(wait_until_epoch(&headers, "x-ratelimit-reset", now.into()), None);
    // With no `Date`, the instant is measured from the clock reading handed in.
    let reset = epoch("Sat, 26 Sep 2026 06:52:05 GMT").to_string();
    headers.insert("x-ratelimit-reset", reqwest::header::HeaderValue::from_str(&reset).unwrap());
    assert_eq!(
        wait_until_epoch(&headers, "x-ratelimit-reset", now.into()),
        Some(Duration::from_secs(20))
    );
}

#[test]
fn a_told_wait_is_read_off_the_headers() {
    let now: std::time::SystemTime =
        chrono::DateTime::parse_from_rfc2822("Sat, 26 Sep 2026 06:51:45 GMT").unwrap().into();
    let mut headers = reqwest::header::HeaderMap::new();
    assert_eq!(told_wait(&headers, now), None);
    headers.insert(reqwest::header::RETRY_AFTER, reqwest::header::HeaderValue::from_static("7"));
    assert_eq!(told_wait(&headers, now), Some(Duration::from_secs(7)));
    // With no `Date`, a told date is measured from the clock reading handed in.
    headers.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_static("Sat, 26 Sep 2026 06:52:00 GMT"),
    );
    assert_eq!(told_wait(&headers, now), Some(Duration::from_secs(15)));
}

#[tokio::test]
async fn transport_failure_names_how_a_request_failed() {
    assert_eq!(TransportFailure::of(&unbuildable_request_error()), TransportFailure::Unmakeable,);
    assert_eq!(TransportFailure::of(&refused_request_error().await), TransportFailure::Network);
}
