//! Helpers for the tests that load boards in Chrome.

use crate::render::chrome::{self, Chrome};
use crate::render::serve::{self, Server};
use std::path::Path;

/// Runs `work` with the tree of `root` served and a browser started.
pub(crate) async fn with_browser<T>(
    root: &Path,
    work: impl AsyncFnOnce(&Server, &Chrome) -> T,
) -> T {
    serve::with_server(root, async |server| {
        chrome::with_chrome(async |chrome| work(server, chrome).await).await.unwrap()
    })
    .await
    .unwrap()
}
