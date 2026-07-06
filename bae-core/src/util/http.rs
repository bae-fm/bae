//! Shared outbound HTTP client configuration.

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

/// A `reqwest` client builder pre-set with bae's user-agent. Callers add any
/// further settings (redirect policy, timeouts) and call `.build()` themselves
/// so they keep their own error handling.
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;
    use tokio::time::{timeout, Duration};

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
}
