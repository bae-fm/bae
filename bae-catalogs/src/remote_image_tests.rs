//! What the image tests share: a test image, a direct transport, and local
//! hosts answering as an artwork host would.

use bae_util::http::Http;
use std::sync::Arc;

pub(super) fn test_png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16, 16)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("test image encodes");
    bytes.into_inner()
}

/// A transport that sends each request where its URL says: these tests serve
/// images from their own local servers and ask for them by those addresses.
pub(super) fn direct_http() -> Http {
    Http::new().expect("the test HTTP client builds")
}

/// Spawn a localhost HTTP server that returns the given responses in order,
/// clamping to the last once exhausted.
pub(super) async fn start_mock(responses: Vec<(u16, Vec<u8>)>) -> String {
    use axum::extract::State;
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct Mock {
        responses: Arc<Vec<(u16, Vec<u8>)>>,
        index: Arc<AtomicUsize>,
    }

    async fn handler(State(mock): State<Mock>) -> (StatusCode, Vec<u8>) {
        let index = mock.index.fetch_add(1, Ordering::SeqCst).min(mock.responses.len() - 1);
        let (status, body) = &mock.responses[index];
        (StatusCode::from_u16(*status).unwrap(), body.clone())
    }

    let state = Mock { responses: Arc::new(responses), index: Arc::new(AtomicUsize::new(0)) };
    let app = axum::Router::new().fallback(handler).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("the test server serves until the test ends");
    });
    format!("http://{address}/cover.jpg")
}

#[derive(Clone)]
pub(super) struct CountingHost {
    status: u16,
    body: Arc<Vec<u8>>,
    hits: Arc<std::sync::atomic::AtomicUsize>,
}

impl CountingHost {
    pub(super) fn hits(&self) -> usize {
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub(super) async fn start_counting_host(status: u16, body: Vec<u8>) -> (CountingHost, String) {
    use axum::extract::State;
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn handler(State(host): State<CountingHost>) -> (StatusCode, Vec<u8>) {
        host.hits.fetch_add(1, Ordering::SeqCst);
        (StatusCode::from_u16(host.status).expect("valid test status"), host.body.as_ref().clone())
    }

    let host = CountingHost { status, body: Arc::new(body), hits: Arc::new(AtomicUsize::new(0)) };
    let app = axum::Router::new().fallback(handler).with_state(host.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("the test server serves until the test ends");
    });
    (host, format!("http://{address}/cover.jpg"))
}

pub(super) async fn start_declared_length_response(content_length: usize) -> String {
    use tokio::io::AsyncWriteExt;

    let listener =
        tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test listener should bind");
    let address = listener.local_addr().expect("test listener should have an address");
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("test request should connect");
        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\n\r\n");
        stream.write_all(response.as_bytes()).await.expect("test response headers should write");
        std::future::pending::<()>().await;
    });
    format!("http://{address}/cover.jpg")
}

/// A successful response whose body ends before its declared length.
pub(super) async fn truncated_body_url() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let received = stream.read(&mut request).await.unwrap();
        assert!(received > 0, "client must send a request before the response");
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 256\r\nConnection: close\r\n\r\nx")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
        let mut remaining = Vec::new();
        stream.read_to_end(&mut remaining).await.unwrap();
    });
    format!("http://{address}/cover")
}
