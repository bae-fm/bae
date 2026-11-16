//! A static HTTP server on 127.0.0.1 for the boards. The canvas runtime
//! fetches the page's own URL and each `dc-import` as `./<name>.dc.html`, so a
//! board cannot be loaded from `file:`.
//!
//! The served tree: `design/mocks/**` at `/`, the runtime copy at `/support.js`
//! and `/storage/support.js`, the built token sheet, style sheet and script
//! at `/ds/bae/…` and `/storage/ds/bae/…`, React at `/react.js` and
//! `/react-dom.js`, and each component's preview page at
//! `/components/<Name>.html`.

use crate::design::{BUNDLE_CSS, BUNDLE_JS, COMPONENTS, MOCKS, TOKENS};
use crate::render::{preview, runtime};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What a URL path is served from.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    File(PathBuf),
    /// A component's `preview.html`, with the design system loaded ahead of
    /// its script.
    Preview {
        component: String,
        file: PathBuf,
    },
}

/// The component named by `/components/<Name>.html`.
fn preview_component(path: &str) -> Option<&str> {
    let name = path.strip_prefix("/components/")?.strip_suffix(".html")?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric())).then_some(name)
}

/// What a URL path is served from, if it names something.
fn route(root: &Path, url_path: &str) -> Option<Route> {
    let path = url_path.split(['?', '#']).next()?;
    let path = path.strip_prefix("/storage").filter(|p| p.starts_with('/')).unwrap_or(path);
    let file = match path {
        "/support.js" => Some(runtime::copy_path(root)),
        "/react.js" => Some(root.join(preview::REACT)),
        "/react-dom.js" => Some(root.join(preview::REACT_DOM)),
        "/ds/bae/tokens.css" => Some(root.join(TOKENS)),
        "/ds/bae/components/bundle.css" => Some(root.join(BUNDLE_CSS)),
        "/ds/bae/components/bundle.js" => Some(root.join(BUNDLE_JS)),
        _ => None,
    };
    if let Some(file) = file {
        return Some(Route::File(file));
    }
    if let Some(component) = preview_component(path) {
        let file = root.join(COMPONENTS).join(component).join("preview.html");
        return file.is_file().then(|| Route::Preview { component: component.to_string(), file });
    }
    let relative = Path::new(path.strip_prefix('/')?);
    if relative.as_os_str().is_empty()
        || !relative.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let file = root.join(MOCKS).join(relative);
    file.is_file().then_some(Route::File(file))
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
            Some(Route::File(file)) => {
                tokio::fs::read(&file).await.ok().map(|b| (content_type(&file), b))
            }
            Some(Route::Preview { component, file }) => tokio::fs::read_to_string(&file)
                .await
                .ok()
                .and_then(|html| preview::assemble(&component, &html).ok())
                .map(|html| ("text/html; charset=utf-8", html.into_bytes())),
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
    /// The URL of the preview page of `component`.
    pub(crate) fn preview_url(&self, component: &str) -> String {
        format!("http://{}/components/{component}.html", self.address)
    }

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
