//! Rendering the design boards.

pub(crate) mod board;
pub(crate) mod chrome;
#[cfg(test)]
mod fixture;
pub(crate) mod runtime;
pub(crate) mod serve;

use crate::design;
use crate::names::Name;
use std::path::Path;

/// Pixels per point the boards are drawn at.
const BOARD_SCALE: f64 = 1.0;

/// Renders the board of each screen shot in `names` into `out` as
/// `<name>.png`, after the design builds and the runtime checks.
pub(crate) fn boards(root: &Path, names: &[Name], out: &Path) -> Result<(), String> {
    let runtime = runtime::check(root).map_err(|e| e.to_string())?;
    design::build(root).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| format!("cannot prepare {}: {e}", out.display()))?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(draw(root, names, out, &runtime))
}

async fn draw(
    root: &Path,
    names: &[Name],
    out: &Path,
    runtime: &runtime::Checked,
) -> Result<(), String> {
    serve::with_server(root, async |server| {
        chrome::with_chrome(async |chrome| {
            draw_with(root, names, out, runtime, server, chrome).await
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn draw_with(
    root: &Path,
    names: &[Name],
    out: &Path,
    runtime: &runtime::Checked,
    server: &serve::Server,
    chrome: &chrome::Chrome,
) -> Result<(), String> {
    for name in names.iter().filter(|n| matches!(n, Name::Screen(_))) {
        let board = name.to_string();
        let frame = design::frame(root, &board).map_err(|e| e.to_string())?;
        let hash = design::board_hash(root, &board, &runtime.sha256, chrome.version())
            .map_err(|e| e.to_string())?;
        let page = chrome.new_page().await.map_err(|e| e.to_string())?;
        let rendered = board::render(&page, &board, &server.board_url(&board), frame, BOARD_SCALE)
            .await
            .map_err(|e| e.to_string());
        if let Err(error) = page.close().await {
            eprintln!("bae-shots: closing a page failed: {error}");
        }
        let file = out.join(format!("{board}.png"));
        std::fs::write(&file, rendered?.png)
            .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        eprintln!("bae-shots: rendered {board} (board {hash})");
    }
    Ok(())
}
