//! A static HTTP server on 127.0.0.1 for the boards. The canvas runtime
//! fetches the page's own URL and each `dc-import` as `./<name>.dc.html`, so a
//! board cannot be loaded from `file:`.
//!
//! The served tree: `design/mocks/**` at `/`, the runtime copy at `/support.js`
//! and `/storage/support.js`, and the built token sheet, style sheet and script
//! at `/ds/bae/…` and `/storage/ds/bae/…`.

use crate::design::{BUNDLE_CSS, BUNDLE_JS, MOCKS, TOKENS};
use crate::render::runtime;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The file a URL path is served from, if it names one.
fn route(root: &Path, url_path: &str) -> Option<PathBuf> {
    let path = url_path.split(['?', '#']).next()?;
    let path = path.strip_prefix("/storage").filter(|p| p.starts_with('/')).unwrap_or(path);
    match path {
        "/support.js" => return Some(runtime::copy_path(root)),
        "/ds/bae/tokens.css" => return Some(root.join(TOKENS)),
        "/ds/bae/components/bundle.css" => return Some(root.join(BUNDLE_CSS)),
        "/ds/bae/components/bundle.js" => return Some(root.join(BUNDLE_JS)),
        _ => {}
    }
    let relative = Path::new(path.strip_prefix('/')?);
    if relative.as_os_str().is_empty()
        || !relative.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let file = root.join(MOCKS).join(relative);
    file.is_file().then_some(file)
}

fn content_type(file: &Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

async fn respond(root: &Path, mut stream: TcpStream) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buffer[..n]);
    }
    let head = String::from_utf8_lossy(&request);
    let mut line = head.lines().next().unwrap_or("").split(' ');
    let (method, target) = (line.next().unwrap_or(""), line.next().unwrap_or(""));
    let found = if method == "GET" || method == "HEAD" {
        match route(root, target) {
            Some(file) => tokio::fs::read(&file).await.ok().map(|b| (content_type(&file), b)),
            None => None,
        }
    } else {
        None
    };
    let (status, kind, body) = match found {
        Some((kind, body)) => ("200 OK", kind, body),
        None => ("404 Not Found", "text/plain", format!("not found: {target}").into_bytes()),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    if method != "HEAD" {
        stream.write_all(&body).await?;
    }
    stream.shutdown().await
}

pub(crate) struct Server {
    address: SocketAddr,
}

impl Server {
    /// The URL of the board `name`.
    pub(crate) fn board_url(&self, name: &str) -> String {
        format!("http://{}/{name}.dc.html", self.address)
    }
}

/// Answers requests from `listener` until it fails; the connections are
/// served together, in this future.
async fn serve(root: &Path, listener: TcpListener) -> std::io::Error {
    let mut connections = FuturesUnordered::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => connections.push(respond(root, stream)),
                Err(error) => return error,
            },
            Some(done) = connections.next() => {
                if let Err(error) = done {
                    eprintln!("bae-shots: serving a request failed: {error}");
                }
            }
        }
    }
}

/// Serves the tree of `root` on a free port while `work` runs with the
/// server, and stops with it. The requests are answered in this future, so no
/// task outlives the call.
pub(crate) async fn with_server<T>(
    root: &Path,
    work: impl AsyncFnOnce(&Server) -> T,
) -> std::io::Result<T> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let server = Server { address: listener.local_addr()? };
    tokio::select! {
        out = work(&server) => Ok(out),
        error = serve(root, listener) => Err(error),
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
