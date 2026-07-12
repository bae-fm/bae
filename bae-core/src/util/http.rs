//! Shared outbound HTTP client configuration.

use reqwest::redirect::Policy;
use std::time::Duration;

/// User-agent sent on every bae-originated HTTP request (MusicBrainz, Discogs,
/// the Cover Art Archive, artist images). Some APIs reject requests without a
/// descriptive agent, so keep it identifying.
pub(crate) const USER_AGENT: &str = "bae/1.0 +https://github.com/bae-fm/bae";

/// Total per-request ceiling for a bounded JSON API call.
pub(crate) const API_TIMEOUT: Duration = Duration::from_secs(30);

/// Bounded time to establish a TCP + TLS connection to any provider.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Stall guard for any provider socket after the request starts.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum redirects any provider request may follow.
pub(crate) const MAX_REDIRECTS: usize = 10;

/// Ceiling for image/cover bodies read into memory.
pub(crate) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// A `reqwest` client builder pre-set with bae's outbound HTTP policy. Callers
/// add their endpoint's settings and call `.build()` themselves, so each keeps
/// its own error handling.
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(Policy::limited(MAX_REDIRECTS))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HttpBodyError {
    #[error("HTTP body too large (limit {limit} bytes)")]
    TooLarge { limit: usize },
    #[error("failed to read HTTP body: {0}")]
    Read(reqwest::Error),
}

pub(crate) async fn read_body_capped(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, HttpBodyError> {
    if let Some(len) = response.content_length() {
        if len > max_bytes as u64 {
            return Err(HttpBodyError::TooLarge { limit: max_bytes });
        }
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(HttpBodyError::Read)? {
        if chunk.len() > max_bytes.saturating_sub(body.len()) {
            return Err(HttpBodyError::TooLarge { limit: max_bytes });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

    async fn response_from_raw(raw_response: Vec<u8>) -> reqwest::Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("test listener should have an address")
        );
        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("test request should connect");
            stream
                .write_all(&raw_response)
                .await
                .expect("test response should write");
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("test listener should have an address")
        );
        let (accepted_tx, accepted_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (_stream, _) = listener
                .accept()
                .await
                .expect("test request should connect");
            accepted_tx
                .send(())
                .expect("test should receive accepted signal");
            std::future::pending::<()>().await;
        });
        let client = client_builder()
            .read_timeout(Duration::from_millis(10))
            .build()
            .expect("test HTTP client should build");
        let request_url = url.clone();
        let request = tokio::spawn(async move { client.get(&request_url).send().await });

        accepted_rx
            .await
            .expect("test listener should accept the request");
        let result = timeout(Duration::from_secs(1), request)
            .await
            .expect("request should finish before the outer guard")
            .expect("request task should finish");

        assert!(result
            .expect_err("stalled response should fail")
            .is_timeout());
    }

    #[tokio::test]
    async fn read_body_capped_rejects_stream_past_limit() {
        let mut raw_response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
        raw_response.extend([0xAB; 129]);
        let response = response_from_raw(raw_response).await;

        let error = read_body_capped(response, 128)
            .await
            .expect_err("over-limit stream should fail");

        assert!(matches!(error, HttpBodyError::TooLarge { limit: 128 }));
    }
}
