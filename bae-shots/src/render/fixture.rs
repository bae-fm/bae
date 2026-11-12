//! A served design tree for the tests that load boards in Chrome: the real
//! runtime copy, a one-component design system, and the boards a test gives.

use crate::render::chrome::{self, Chrome};
use crate::render::serve::{self, Server};
use std::path::Path;

pub(crate) const HEAD: &str = r#"<!doctype html><html><head><meta charset="utf-8">
<script src="./support.js"></script>
<link rel="stylesheet" href="ds/bae/tokens.css">
<link rel="stylesheet" href="ds/bae/components/bundle.css">
<script src="ds/bae/components/bundle.js"></script></head><body>"#;

pub(crate) fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A tree holding the runtime and the design system, and each of `boards`
/// (name, the page after the shared head).
pub(crate) fn tree(boards: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("canvas/dc-runtime.js");
    write(r, "bae-shots/canvas/dc-runtime.js", &std::fs::read_to_string(runtime).unwrap());
    write(r, "design/system/tokens.css", ":root{--text-primary:#fff}");
    write(r, "design/system/components/bundle.css", "");
    write(
        r,
        "design/system/components/bundle.js",
        "window.Bae={Box:function(){return React.createElement('div',{id:'inner'},'hello')}};",
    );
    for (name, page) in boards {
        write(r, &format!("design/mocks/{name}.dc.html"), &format!("{HEAD}{page}</body></html>"));
    }
    root
}

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
