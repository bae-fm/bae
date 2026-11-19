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

/// A tree holding the runtime and the design system, and each of `boards`
/// (name, the page after the shared head).
pub(crate) fn tree(boards: &[(&str, &str)]) -> bae_util::test_files::TestFolder {
    let root = bae_util::test_files::TestFolder::new();
    let runtime =
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("canvas/dc-runtime.js");
    root.write("bae-shots/canvas/dc-runtime.js", bae_util::test_files::read_fixture(runtime));
    root.write("design/system/tokens.css", ":root{--text-primary:#fff}");
    root.write("design/system/components/bundle.css", "");
    root.write(
        "design/system/components/bundle.js",
        "window.Bae={Box:function(){return React.createElement('div',{id:'inner'},'hello')}};",
    );
    for (name, page) in boards {
        root.write(format!("design/mocks/{name}.dc.html"), format!("{HEAD}{page}</body></html>"));
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
