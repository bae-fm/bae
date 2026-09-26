//! Shared outbound HTTP client configuration.

use reqwest::redirect::Policy;
use std::time::Duration;

/// User-agent sent on every bae-originated HTTP request (MusicBrainz, Discogs,
/// the Cover Art Archive, artist images). Some APIs reject requests without a
/// descriptive agent, so keep it identifying.
pub(crate) const USER_AGENT: &str = "bae/1.0 +https://github.com/bae-fm/bae";

/// Total per-request ceiling for a bounded JSON API call.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) const API_TIMEOUT: Duration = Duration::from_secs(30);

/// Bounded time to establish a TCP + TLS connection to any provider.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Stall guard for any provider socket after the request starts.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum redirects any provider request may follow.
pub(crate) const MAX_REDIRECTS: usize = 10;

/// Ceiling for image/cover bodies read into memory.
pub(crate) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// How long a response asks its client to wait before asking again, read off
/// its `Retry-After`: a number of seconds, or an HTTP-date. A date is measured
/// from the response's own `Date` — the server's clock, so this machine's
/// being off does not stretch or cut the wait — and from this machine's clock
/// only when the response states no date. `None` when the response has no
/// `Retry-After` or one that says nothing readable; a date already past asks
/// for no wait.
///
/// The one reader of the header: every provider client hands the retry loop
/// what this returns.
pub(crate) fn told_wait(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let retry_after = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let date = headers
        .get(reqwest::header::DATE)
        .and_then(|value| value.to_str().ok());
    told_wait_at(retry_after, date, chrono::Utc::now())
}

fn told_wait_at(
    retry_after: &str,
    date: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Duration> {
    let retry_after = retry_after.trim();
    if let Ok(seconds) = retry_after.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let until = http_date(retry_after)?;
    let from = date.and_then(http_date).unwrap_or(now);
    Some((until - from).to_std().unwrap_or(Duration::ZERO))
}

/// An HTTP-date in its preferred form, `Sun, 06 Nov 1994 08:49:37 GMT`, which
/// is an RFC 2822 date.
fn http_date(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc2822(value.trim())
        .ok()
        .map(|date| date.with_timezone(&chrono::Utc))
}

/// One provider response worth keeping: the status it came back with and the
/// whole body. The MusicBrainz and Discogs clients hold these keyed by request
/// URL, so asking a provider the same question twice in one session costs one
/// round trip.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone)]
pub(crate) struct CachedResponse {
    pub(crate) status: u16,
    pub(crate) body: String,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl CachedResponse {
    pub(crate) fn is_success(&self) -> bool {
        is_success(self.status)
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// Whether a status is the provider's answer rather than its momentary state. A
/// 2xx and a 404 are stable — asking again returns the same thing. A 429, a 5xx
/// and every transport failure say only that this attempt did not land, so the
/// next caller asks again. A 401 answers a question about the key in the
/// request's own headers, which no URL names, so it is not this URL's answer
/// either.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn is_cacheable(status: u16) -> bool {
    is_success(status) || status == 404
}

/// A response's cache key: the request URL as reqwest renders it. A test puts a
/// canned answer at this key and the request that looks for it computes the
/// same one, so the two cannot drift apart over URL normalization.
#[cfg(all(
    any(test, feature = "test-utils"),
    not(any(target_os = "ios", target_os = "android"))
))]
pub(crate) fn response_key(url: &str) -> String {
    reqwest::Url::parse(url)
        .expect("a provider request URL parses")
        .to_string()
}

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

/// How bae's outbound requests reach the network: one client with bae's user
/// agent, timeouts and redirect policy, whose connection pool every provider
/// shares. Built once when the app starts and handed to each provider.
///
/// Requests are always built against the real addresses. A test builds one
/// whose requests go to local servers instead — [`Http::for_test`] sends every
/// host to a port nothing listens on, and [`Http::serve`] routes a host to a
/// fake — so a test never reaches the network, and the URLs a response is
/// cached under, or a cover is stored under, are the ones production uses.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    #[cfg(any(test, feature = "test-utils"))]
    routes: Option<std::sync::Arc<TestRoutes>>,
}

/// Where a test's requests go, by host: a host with a route goes to its
/// origin, and every other host to `unrouted`.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone)]
struct TestRoutes {
    by_host: std::collections::HashMap<String, reqwest::Url>,
    unrouted: reqwest::Url,
}

impl Http {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client_builder().build()?,
            #[cfg(any(test, feature = "test-utils"))]
            routes: None,
        })
    }

    /// A transport whose every request goes to a port nothing listens on, so
    /// it fails fast and locally until [`Self::serve`] routes its host.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test() -> Self {
        Self {
            client: client_builder()
                .build()
                .expect("the test HTTP client builds"),
            routes: Some(std::sync::Arc::new(TestRoutes {
                by_host: std::collections::HashMap::new(),
                unrouted: reqwest::Url::parse("http://127.0.0.1:9").expect("the dead port parses"),
            })),
        }
    }

    /// This transport with requests for `host` sent to `origin`, a local
    /// server's scheme, host and port.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn serve(mut self, host: &str, origin: &str) -> Self {
        let routes = std::sync::Arc::make_mut(
            self.routes
                .as_mut()
                .expect("only a test transport routes hosts"),
        );
        routes.by_host.insert(
            host.to_string(),
            reqwest::Url::parse(origin).expect("a test origin parses"),
        );
        self
    }

    /// This transport with every host not routed on its own sent to `origin`.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn serve_every_host(mut self, origin: &str) -> Self {
        let routes = std::sync::Arc::make_mut(
            self.routes
                .as_mut()
                .expect("only a test transport routes hosts"),
        );
        routes.unrouted = reqwest::Url::parse(origin).expect("a test origin parses");
        self
    }

    pub(crate) fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.client.get(url)
    }

    /// Send `request`. The URL it was built with is the one it is known by;
    /// only where it is sent changes in a test.
    pub(crate) async fn execute(
        &self,
        request: reqwest::Request,
    ) -> reqwest::Result<reqwest::Response> {
        #[cfg(any(test, feature = "test-utils"))]
        let request = self.routed(request);
        self.client.execute(request).await
    }

    /// `request` sent to the origin its host is routed to. A transport built
    /// with [`Self::new`] routes nothing.
    #[cfg(any(test, feature = "test-utils"))]
    fn routed(&self, mut request: reqwest::Request) -> reqwest::Request {
        let Some(routes) = &self.routes else {
            return request;
        };
        let origin = request
            .url()
            .host_str()
            .and_then(|host| routes.by_host.get(host))
            .unwrap_or(&routes.unrouted)
            .clone();
        let url = request.url_mut();
        url.set_scheme(origin.scheme())
            .expect("a test origin's scheme applies");
        url.set_host(origin.host_str())
            .expect("a test origin's host applies");
        url.set_port(origin.port())
            .expect("a test origin's port applies");
        request
    }
}

/// The origin of a local server that answers every request 404: a host with
/// nothing at any address a test reaches. It runs on the calling test's
/// runtime and ends with it.
#[cfg(test)]
pub(crate) async fn serve_not_found() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("the not-found server binds");
    let origin = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("the not-found server has an address")
    );
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                // Read the whole request head before answering, so the client
                // is not answered mid-send.
                let mut head = Vec::new();
                let mut chunk = [0u8; 1024];
                while !head.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = stream
                        .read(&mut chunk)
                        .await
                        .expect("the not-found server reads a request");
                    assert!(read > 0, "the client closed before sending a request");
                    head.extend_from_slice(&chunk[..read]);
                }
                stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .expect("the not-found server answers");
            });
        }
    });
    origin
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

    #[test]
    fn a_told_wait_reads_seconds_and_dates() {
        let now = chrono::DateTime::parse_from_rfc2822("Sat, 26 Sep 2026 06:51:45 GMT")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(
            told_wait_at("120", None, now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(told_wait_at(" 0 ", None, now), Some(Duration::ZERO));
        // A date is measured from the response's own clock, not this one.
        assert_eq!(
            told_wait_at(
                "Sat, 26 Sep 2026 07:00:00 GMT",
                Some("Sat, 26 Sep 2026 06:59:30 GMT"),
                now,
            ),
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
    fn a_told_wait_is_read_off_the_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(told_wait(&headers), None);
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("7"),
        );
        assert_eq!(told_wait(&headers), Some(Duration::from_secs(7)));
    }
}
